use blaze_ssl_async::{stream::BlazeStream, BlazeError};
use futures_util::{SinkExt, StreamExt};
use log::{debug, error};
use reqwest;
use serde::Deserialize;
use std::{fmt::Display, net::Ipv4Addr};
use tdf::{DecodeError, GroupSlice, TdfDeserialize, TdfDeserializeOwned, TdfSerialize, TdfTyped};
use thiserror::Error;
use tokio::io;
use tokio_util::codec::Framed;

use crate::servers::{components::redirector, packet::PacketDebug};

use super::packet::{Packet, PacketCodec, PacketHeader, PacketType};

pub struct InstanceRequest;

impl TdfSerialize for InstanceRequest {
    fn serialize<S: tdf::TdfSerializer>(&self, w: &mut S) {
        w.tag_str(b"BSDK", "3.15.6.0");
        w.tag_str(b"BTIM", "Dec 21 2012 12:47:10");
        w.tag_str(b"CLNT", "MassEffect3-pc");
        w.tag_u8(b"CLTP", 0);
        w.tag_str(b"CSKU", "134845");
        w.tag_str(b"CVER", "05427.124");
        w.tag_str(b"DSDK", "8.14.7.1");
        w.tag_str(b"ENV", "prod");
        w.tag_union_unset(b"FPID");
        w.tag_u32(b"LOC", 0x656e4e5a);
        w.tag_str(b"NAME", "masseffect-3-pc");
        w.tag_str(b"PLAT", "Windows");
        w.tag_str(b"PROF", "standardSecure_v3");
    }
}

/// Networking information for an instance. Contains the
/// host address and the port
#[derive(TdfTyped)]
#[tdf(group)]
pub struct InstanceAddress {
    pub host: InstanceHost,
    pub port: u16,
}

impl TdfSerialize for InstanceAddress {
    fn serialize<S: tdf::TdfSerializer>(&self, w: &mut S) {
        w.group_body(|w| {
            self.host.serialize(w);
            w.tag_u16(b"PORT", self.port);
        });
    }
}

impl TdfDeserializeOwned for InstanceAddress {
    fn deserialize_owned(r: &mut tdf::TdfDeserializer<'_>) -> tdf::DecodeResult<Self> {
        let host: InstanceHost = InstanceHost::deserialize_owned(r)?;
        let port: u16 = r.tag(b"PORT")?;
        GroupSlice::deserialize_content_skip(r)?;
        Ok(Self { host, port })
    }
}

pub enum InstanceHost {
    Host(String),
    Address(Ipv4Addr),
}

impl From<InstanceHost> for String {
    fn from(value: InstanceHost) -> Self {
        match value {
            InstanceHost::Address(value) => value.to_string(),
            InstanceHost::Host(value) => value,
        }
    }
}

impl TdfSerialize for InstanceHost {
    fn serialize<S: tdf::TdfSerializer>(&self, w: &mut S) {
        match self {
            InstanceHost::Host(value) => w.tag_str(b"HOST", value),
            InstanceHost::Address(value) => w.tag_u32(b"IP", (*value).into()),
        }
    }
}

impl TdfDeserializeOwned for InstanceHost {
    fn deserialize_owned(r: &mut tdf::TdfDeserializer<'_>) -> tdf::DecodeResult<Self> {
        let host: Option<String> = r.try_tag(b"HOST")?;
        if let Some(host) = host {
            return Ok(Self::Host(host));
        }
        let ip: u32 = r.tag(b"IP")?;
        Ok(Self::Address(Ipv4Addr::from(ip)))
    }
}

/// Details about an instance. This is used for the redirector system
/// to both encode for redirections and decode for the retriever system
#[derive(TdfDeserialize)]
pub struct InstanceDetails {
    /// The networking information for the instance
    #[tdf(tag = "ADDR")]
    pub net: InstanceNet,
    /// Whether the host requires a secure connection (SSLv3)
    #[tdf(tag = "SECU")]
    pub secure: bool,
    #[tdf(tag = "XDNS")]
    pub xdns: bool,
}

#[derive(Default, TdfSerialize, TdfDeserialize, TdfTyped)]
pub enum InstanceNet {
    #[tdf(key = 0x0, tag = "VALU")]
    InstanceAddress(InstanceAddress),
    #[tdf(unset)]
    Unset,
    #[default]
    #[tdf(default)]
    Default,
    // IpAddress = 0x0,
    // XboxServer = 0x1,
}

/// Connection details for an official server instance
pub struct OfficialInstance {
    /// The host address of the official server
    pub host: String,
    /// The port of the official server.
    pub port: u16,
}

/// Errors that could occur while attempting to obtain
/// an official server instance details
#[derive(Debug, Error)]
pub enum InstanceError {
    #[error("Failed to request lookup from cloudflare: {0}")]
    LookupRequest(#[from] reqwest::Error),
    #[error("Failed to lookup server response empty")]
    MissingValue,
    #[error("Failed to connect to server: {0}")]
    Blaze(#[from] BlazeError),
    #[error("Failed to retrieve instance: {0}")]
    InstanceRequest(#[from] RetrieverError),
    #[error("Server response missing address")]
    MissingAddress,
}

impl OfficialInstance {
    const REDIRECTOR_HOST: &str = "gosredirector.ea.com";
    const REDIRECT_PORT: u16 = 42127;

    pub async fn obtain() -> Result<OfficialInstance, InstanceError> {
        let host = Self::lookup_host().await?;
        debug!("Completed host lookup: {}", &host);

        // Create a session to the redirector server
        let mut session = OfficialSession::connect(&host, Self::REDIRECT_PORT).await?;

        // Request the server instance
        let instance: InstanceDetails = session
            .request(
                redirector::COMPONENT,
                redirector::GET_SERVER_INSTANCE,
                InstanceRequest,
            )
            .await?;

        // Extract the host and port turning the host into a string
        let (host, port) = match instance.net {
            InstanceNet::InstanceAddress(addr) => (addr.host, addr.port),
            _ => return Err(InstanceError::MissingAddress),
        };
        let host: String = host.into();

        debug!(
            "Retriever instance obtained. (Host: {} Port: {})",
            &host, port
        );

        Ok(OfficialInstance { host, port })
    }

    async fn lookup_host() -> Result<String, InstanceError> {
        let host = Self::REDIRECTOR_HOST;

        // Attempt to lookup using the system DNS
        {
            let tokio = tokio::net::lookup_host(host)
                .await
                .ok()
                .and_then(|mut value| value.next());

            if let Some(tokio) = tokio {
                let ip = tokio.ip();
                // Loopback value means it was probbably redirected in the hosts file
                // so those are ignored
                if !ip.is_loopback() {
                    return Ok(format!("{}", ip));
                }
            }
        }

        // Attempt to lookup using cloudflares DNS over HTTP

        let client = reqwest::Client::new();
        let url = format!("https://cloudflare-dns.com/dns-query?name={host}&type=A");
        let mut response: LookupResponse = client
            .get(url)
            .header("Accept", "application/dns-json")
            .send()
            .await?
            .json()
            .await?;

        response
            .answer
            .pop()
            .map(|value| value.data)
            .ok_or(InstanceError::MissingValue)
    }

    /// Creates a stream to the main server and wraps it with a
    /// session returning that session. Will return None if the
    /// stream failed.
    pub async fn stream(&self) -> Result<BlazeStream, BlazeError> {
        BlazeStream::connect((self.host.as_str(), self.port)).await
    }
}

/// Session implementation for a retriever client
pub struct OfficialSession {
    /// The ID for the next request packet
    id: u16,
    /// The underlying SSL / TCP stream connection
    stream: Framed<BlazeStream, PacketCodec>,
}

/// Error type for retriever errors
#[derive(Debug, Error)]
pub enum RetrieverError {
    /// Packet decode errror
    #[error(transparent)]
    Decode(#[from] DecodeError),
    /// IO Error
    #[error(transparent)]
    IO(#[from] io::Error),
    /// Error response packet
    #[error(transparent)]
    Packet(#[from] ErrorPacket),
    /// Stream ended early
    #[error("Reached end of stream")]
    EarlyEof,
}

pub type RetrieverResult<T> = Result<T, RetrieverError>;

impl OfficialSession {
    /// Creates a session with an official server at the provided
    /// `host` and `port`
    async fn connect(host: &str, port: u16) -> Result<OfficialSession, BlazeError> {
        let stream = BlazeStream::connect((host, port)).await?;
        Ok(Self {
            id: 0,
            stream: Framed::new(stream, PacketCodec),
        })
    }
    /// Writes a request packet and waits until the response packet is
    /// recieved returning the contents of that response packet.
    pub async fn request<Req, Res>(
        &mut self,
        component: u16,
        command: u16,
        contents: Req,
    ) -> RetrieverResult<Res>
    where
        Req: TdfSerialize,
        for<'a> Res: TdfDeserialize<'a> + 'a,
    {
        let response = self.request_raw(component, command, contents).await?;
        let contents = response.deserialize::<Res>()?;
        Ok(contents)
    }

    /// Writes a request packet and waits until the response packet is
    /// recieved returning the contents of that response packet.
    pub async fn request_raw<Req: TdfSerialize>(
        &mut self,
        component: u16,
        command: u16,
        contents: Req,
    ) -> RetrieverResult<Packet> {
        let request = Packet::request(self.id, component, command, contents);

        debug_log_packet(&request, "Send");
        let header = request.header;

        self.stream.send(request).await?;

        self.id += 1;
        self.expect_response(&header).await
    }

    /// Writes a request packet and waits until the response packet is
    /// recieved returning the contents of that response packet. The
    /// request will have no content
    pub async fn request_empty<Res>(&mut self, component: u16, command: u16) -> RetrieverResult<Res>
    where
        for<'a> Res: TdfDeserialize<'a> + 'a,
    {
        let response = self.request_empty_raw(component, command).await?;
        let contents = response.deserialize::<Res>()?;
        Ok(contents)
    }

    /// Writes a request packet and waits until the response packet is
    /// recieved returning the raw response packet
    pub async fn request_empty_raw(
        &mut self,
        component: u16,
        command: u16,
    ) -> RetrieverResult<Packet> {
        let request = Packet::request_empty(self.id, component, command);
        debug_log_packet(&request, "Send");
        let header = request.header;
        self.stream.send(request).await?;
        self.id += 1;
        self.expect_response(&header).await
    }

    /// Waits for a response packet to be recieved any notification packets
    /// that are recieved are handled in the handle_notify function.
    async fn expect_response(&mut self, request: &PacketHeader) -> RetrieverResult<Packet> {
        loop {
            let response = match self.stream.next().await {
                Some(value) => value?,
                None => return Err(RetrieverError::EarlyEof),
            };
            debug_log_packet(&response, "Receive");
            let header = &response.header;

            if let PacketType::Response = header.ty {
                if header.path_matches(request) {
                    return Ok(response);
                }
            } else if let PacketType::Error = header.ty {
                return Err(RetrieverError::Packet(ErrorPacket(response)));
            }
        }
    }
}

/// Logs the contents of the provided packet to the debug output along with
/// the header information.
///
/// `component` The component for the packet routing
/// `packet`    The packet that is being logged
/// `direction` The direction name for the packet
fn debug_log_packet(packet: &Packet, action: &str) {
    let debug = PacketDebug { packet };
    debug!("\nOfficial: {}\n{:?}", action, debug);
}

/// Wrapping structure for packets to allow them to be
/// used as errors
#[derive(Debug)]
pub struct ErrorPacket(Packet);

impl std::error::Error for ErrorPacket {}

impl Display for ErrorPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#X}", self.0.header.error)
    }
}

/// Structure for the lookup responses from the google DNS API
///
/// # Structure
///
/// ```
/// {
///   "Status": 0,
///   "TC": false,
///   "RD": true,
///   "RA": true,
///   "AD": false,
///   "CD": false,
///   "Question": [
///     {
///       "name": "gosredirector.ea.com.",
///       "type": 1
///     }
///   ],
///   "Answer": [
///     {
///       "name": "gosredirector.ea.com.",
///       "type": 1,
///       "TTL": 300,
///       "data": "159.153.64.175"
///     }
///   ],
///   "Comment": "Response from 2600:1403:a::43."
/// }
/// ```
#[derive(Deserialize)]
struct LookupResponse {
    #[serde(rename = "Answer")]
    answer: Vec<Answer>,
}

/// Structure for answer portion of request. Only the data value is
/// being used so only that is present here.
///
/// # Structure
/// ```
/// {
///   "name": "gosredirector.ea.com.",
///   "type": 1,
///   "TTL": 300,
///   "data": "159.153.64.175"
/// }
/// ```
#[derive(Deserialize)]
struct Answer {
    data: String,
}
