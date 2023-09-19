use crate::{
    constants::MAIN_PORT,
    servers::packet::{PacketCodec, PacketDebug},
};
use futures_util::{SinkExt, StreamExt};
use log::{debug, error};
use native_windows_gui::error_message;
use std::{
    net::Ipv4Addr,
    sync::{atomic::AtomicU32, Arc},
};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
};
use tokio_util::codec::Framed;

use super::{packet::Packet, retriever::OfficialInstance};

pub static SESSION_ID: AtomicU32 = AtomicU32::new(1);

/// Starts the main server proxy. This creates a connection to the Pocket Relay
/// which is upgraded and then used as the main connection fro the game.
pub async fn start_server() {
    // Initializing the underlying TCP listener
    let listener = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, MAIN_PORT)).await {
        Ok(value) => value,
        Err(err) => {
            error_message("Failed to start main", &err.to_string());
            error!("Failed to start main: {}", err);
            return;
        }
    };

    let instance = match OfficialInstance::obtain().await {
        Ok(value) => value,
        Err(err) => {
            error_message("Failed to create official instance", &err.to_string());
            error!("Failed to create official instance: {}", err);
            return;
        }
    };

    let ret = Arc::new(instance);

    // Accept incoming connections
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(value) => value,
            Err(err) => {
                error!("Failed to accept main connection: {}", err);
                break;
            }
        };

        debug!("Main connection ->");

        // Spawn off a new handler for the connection
        _ = tokio::spawn(handle_blaze(stream, ret.clone())).await;
    }
}

async fn handle_blaze(client: TcpStream, ret: Arc<OfficialInstance>) {
    let server = match ret.stream().await {
        Ok(value) => value,
        Err(err) => {
            error!("Failed to obtain session with official server: {}", err);
            return;
        }
    };

    let id = SESSION_ID.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    debug!("Starting session {}", id);

    let mut client_framed = Framed::new(client, PacketCodec);
    let mut server_framed = Framed::new(server, PacketCodec);

    loop {
        select! {
            packet = client_framed.next() => {
                if let Some(Ok(packet)) = packet {
                    debug_log_packet(&packet, "Send");
                    _= server_framed.send(packet).await;
                }
            }
            packet = server_framed.next() => {
                if let Some(Ok(packet)) = packet {
                    debug_log_packet(&packet, "Receive");
                    _ = client_framed.send(packet).await;
                }
            }
        }
    }
}

fn debug_log_packet(packet: &Packet, action: &str) {
    let debug = PacketDebug { packet };
    debug!("\nOfficial: {}\n{:?}", action, debug);
}
