//! `corten::shutdown` against a real server, in a process of its own (the shutdown it begins is
//! the process's): a route asks for it and is answered, then the app's task is told, a live stream
//! ends, a request in progress finishes, and `serve_on` returns.

use std::{net::SocketAddr, time::Duration};

use corten::{axum::http::StatusCode, Abort, App, Signal};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

#[corten::routes]
mod url {
    #[route("/stop", post = stop)]
    pub struct Stop;

    #[route("/live", get = stream)]
    pub struct Live;

    #[route("/slow", get = slow)]
    pub struct Slow;
}

/// What a supervisor calls: the shutdown begins, and this answer is still sent.
async fn stop() -> StatusCode {
    corten::shutdown();
    corten::shutdown();
    StatusCode::ACCEPTED
}

/// A live stream that would never end by itself.
async fn stream() -> corten::LiveResponse {
    corten::live(futures_util::stream::pending::<String>())
}

/// A request still in progress when the shutdown begins.
async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_millis(500)).await;
    "done"
}

async fn send(addr: SocketAddr, request: &str) -> TcpStream {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    stream
}

async fn head(stream: &mut TcpStream) -> String {
    let mut head = [0u8; 12];
    stream.read_exact(&mut head).await.unwrap();
    String::from_utf8_lossy(&head).into_owned()
}

#[tokio::test]
async fn the_app_begins_the_shutdown_a_signal_would() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (told, mut heard) = mpsc::unbounded_channel();
    let app = App::new(url::router()).task("watch", move |signal: Signal| async move {
        let _ = told.send(signal.aborted().await);
    });
    let server = tokio::spawn(corten::serve_on(vec![listener], app));

    let mut live = send(addr, "GET /live HTTP/1.1\r\nhost: localhost\r\n\r\n").await;
    assert_eq!(head(&mut live).await, "HTTP/1.1 200");
    let mut slow = send(addr, "GET /slow HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n").await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut asked = send(addr, "POST /stop HTTP/1.1\r\nhost: localhost\r\ncontent-length: 0\r\n\r\n").await;
    assert_eq!(head(&mut asked).await, "HTTP/1.1 202", "the route that asks is answered");

    let told = tokio::time::timeout(Duration::from_secs(5), heard.recv()).await.expect("the task was not told");
    assert_eq!(told, Some(Abort::Shutdown));
    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), live.read_to_end(&mut rest)).await.expect("the live stream did not end").unwrap();
    let mut answer = String::new();
    tokio::time::timeout(Duration::from_secs(5), slow.read_to_string(&mut answer)).await.expect("the slow request").unwrap();
    assert!(answer.starts_with("HTTP/1.1 200") && answer.ends_with("done"), "a request in progress finishes: {answer}");
    tokio::time::timeout(Duration::from_secs(5), server).await.expect("serve_on did not return").unwrap().unwrap();
    assert!(TcpStream::connect(addr).await.is_err(), "the listener is closed");
}
