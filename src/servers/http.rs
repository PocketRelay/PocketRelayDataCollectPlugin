use crate::constants::HTTP_PORT;
use hyper::body::Body;
use hyper::header::HOST;
use hyper::service::service_fn;
use hyper::{server::conn::Http, Request};
use hyper::{Response, StatusCode};
use log::{debug, error};
use native_windows_gui::error_message;
use reqwest::Client;
use std::convert::Infallible;
use std::net::Ipv4Addr;
use tokio::net::TcpListener;

pub async fn start_server() {
    // Initializing the underlying TCP listener
    let listener = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, HTTP_PORT)).await {
        Ok(value) => value,
        Err(err) => {
            error_message("Failed to start http", &err.to_string());
            error!("Failed to start http: {}", err);
            return;
        }
    };

    // Accept incoming connections
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(value) => value,
            Err(_) => break,
        };

        tokio::task::spawn(async move {
            if let Err(err) = Http::new()
                .serve_connection(stream, service_fn(proxy_http))
                .await
            {
                eprintln!("Failed to serve http connection: {:?}", err);
            }
        });
    }
}

async fn proxy_http(req: Request<hyper::body::Body>) -> Result<Response<Body>, Infallible> {
    let path = req
        .uri()
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or_default();

    let req_headers = req.headers();
    let host = match req_headers.get(HOST).and_then(|value| value.to_str().ok()) {
        Some(value) => value,
        None => {
            error!("Failed to send HTTP request: Missing host");
            let mut error_response = Response::new(hyper::Body::empty());
            *error_response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            return Ok(error_response);
        }
    };

    let target_url = format!("https://{}{}", host, path);

    debug!("Client HTTP request: {:?}", &req);

    let client = Client::new();
    let proxy_response = match client.get(target_url).send().await {
        Ok(value) => value,
        Err(err) => {
            error!("Failed to send HTTP request: {}", err);
            let mut error_response = Response::new(hyper::Body::empty());
            *error_response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            return Ok(error_response);
        }
    };

    debug!("Server HTTP response: {:?}", &proxy_response);
    let status = proxy_response.status();
    let headers = proxy_response.headers().clone();

    let body = match proxy_response.bytes().await {
        Ok(value) => value,
        Err(err) => {
            error!("Failed to read HTTP response body: {}", err);
            let mut error_response = Response::new(hyper::Body::empty());
            *error_response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            return Ok(error_response);
        }
    };
    debug!("Server HTTP response body: {:?}", &body);

    let mut response = Response::new(hyper::body::Body::from(body));
    *response.status_mut() = status;
    *response.headers_mut() = headers;

    Ok(response)
}
