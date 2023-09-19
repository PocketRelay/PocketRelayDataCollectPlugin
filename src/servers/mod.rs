use tokio::join;

pub mod components;
pub mod http;
pub mod main;
pub mod packet;
pub mod redirector;
pub mod retriever;

pub fn start_servers() {
    tokio::spawn(async move {
        join!(
            main::start_server(),
            redirector::start_server(),
            http::start_server()
        );
    });
}
