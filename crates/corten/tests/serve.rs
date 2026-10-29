//! `corten::serve_on` against real sockets and a real `SIGTERM`, in a process of its own (the
//! shutdown it starts is the process's): a listener type of the app's own, two of them, a live
//! stream that shutdown ends, then both listeners closed.
#![cfg(unix)]

use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use corten::App;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

/// A listener of the app's own (as a TLS acceptor would be), counting the connections it accepts.
struct Counted(TcpListener, Arc<AtomicUsize>);

impl corten::axum::serve::Listener for Counted {
    type Io = TcpStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (TcpStream, SocketAddr) {
        let accepted = corten::axum::serve::Listener::accept(&mut self.0).await;
        self.1.fetch_add(1, Ordering::SeqCst);
        accepted
    }

    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.0.local_addr()
    }
}

#[corten::routes]
mod url {
    #[route("/", get = home)]
    pub struct Home;

    #[route("/live", get = stream)]
    pub struct Live;
}

async fn home() -> &'static str {
    "hi"
}

/// A live stream that would never end by itself.
async fn stream() -> corten::LiveResponse {
    corten::live(futures_util::stream::pending::<String>())
}

async fn get_text(addr: SocketAddr, path: &str) -> TcpStream {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nhost: localhost\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    stream
}

#[tokio::test]
async fn shutdown_ends_live_streams_then_every_listener() {
    let accepted = Arc::new(AtomicUsize::new(0));
    let mut listeners = Vec::new();
    for _ in 0..2 {
        listeners.push(Counted(TcpListener::bind("127.0.0.1:0").await.unwrap(), accepted.clone()));
    }
    let addrs: Vec<SocketAddr> = listeners.iter().map(|l| l.0.local_addr().unwrap()).collect();
    let server = tokio::spawn(corten::serve_on(listeners, App::new(url::router())));

    // Each listener serves the app.
    for addr in &addrs {
        let mut stream = get_text(*addr, "/").await;
        let mut head = [0u8; 15];
        stream.read_exact(&mut head).await.unwrap();
        assert_eq!(&head, b"HTTP/1.1 200 OK", "{addr}");
    }
    assert_eq!(accepted.load(Ordering::SeqCst), 2, "the app's own listener type accepted them");

    // A live stream that would never end by itself.
    let mut live = get_text(addrs[1], "/live").await;
    let mut head = [0u8; 15];
    live.read_exact(&mut head).await.unwrap();
    assert_eq!(&head, b"HTTP/1.1 200 OK");

    let killed = std::process::Command::new("kill").args(["-TERM", &std::process::id().to_string()]).status().unwrap();
    assert!(killed.success());

    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), live.read_to_end(&mut rest))
        .await
        .expect("shutdown did not end the live stream")
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("serve_on did not return after the live stream ended")
        .unwrap()
        .unwrap();
    for addr in &addrs {
        assert!(TcpStream::connect(addr).await.is_err(), "{addr} still accepts after shutdown");
    }
}
