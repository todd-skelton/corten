//! `corten::shutdown` ends an open event stream (`corten::event_stream`), as it ends a live
//! stream, so a graceful shutdown is not held open by an `EventSource`. In a process of its own,
//! since the shutdown it begins is the process's.
#![cfg(feature = "json")]

use std::time::Duration;

use corten::{axum::http::StatusCode, App};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[corten::routes]
mod url {
    #[route("/stop", post = stop)]
    pub struct Stop;

    #[route("/events", get = events)]
    pub struct Events;
}

async fn stop() -> StatusCode {
    corten::shutdown();
    StatusCode::ACCEPTED
}

/// An event stream that would never end by itself.
async fn events() -> corten::EventStreamResponse {
    corten::event_stream(futures_util::stream::pending())
}

#[tokio::test]
async fn shutdown_ends_an_event_stream() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(corten::serve_on(vec![listener], App::new(url::router())));

    let mut events = TcpStream::connect(addr).await.unwrap();
    events.write_all(b"GET /events HTTP/1.1\r\nhost: localhost\r\n\r\n").await.unwrap();
    let mut head = [0u8; 12];
    events.read_exact(&mut head).await.unwrap();
    assert_eq!(&head, b"HTTP/1.1 200");

    let mut stop = TcpStream::connect(addr).await.unwrap();
    stop.write_all(b"POST /stop HTTP/1.1\r\nhost: localhost\r\ncontent-length: 0\r\n\r\n").await.unwrap();
    stop.read_exact(&mut head).await.unwrap();
    assert_eq!(&head, b"HTTP/1.1 202");

    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), events.read_to_end(&mut rest))
        .await
        .expect("the event stream did not end")
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), server).await.expect("serve_on did not return").unwrap().unwrap();
}
