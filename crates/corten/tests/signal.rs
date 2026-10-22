//! The request signal against a real server: a client that leaves, a handler past its deadline,
//! and a request that is answered.
use std::{io::Write, time::Duration};

use axum::{body::Body, extract::State, http::Request, routing::get, Router};
use corten::{Abort, App, Signal};
use http_body_util::BodyExt;
use tokio::sync::mpsc;
use tower::ServiceExt;

type Tx = mpsc::UnboundedSender<Abort>;

/// Reports how the request's signal ended, from a task that outlives the handler.
fn report(signal: Signal, tx: Tx) {
    tokio::spawn(async move {
        let _ = tx.send(signal.aborted().await);
    });
}

fn app(deadline: Duration) -> (Router, mpsc::UnboundedReceiver<Abort>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let routes = Router::new()
        .route(
            "/slow",
            get(|signal: Signal, State(tx): State<Tx>| async move {
                report(signal, tx);
                std::future::pending::<&str>().await
            }),
        )
        .route(
            "/quick",
            get(|req: corten::Req, State(tx): State<Tx>| async move {
                report(req.signal().clone(), tx);
                "done"
            }),
        )
        .with_state(tx);
    (App::new(routes).deadline(deadline).router(), rx)
}

async fn next(rx: &mut mpsc::UnboundedReceiver<Abort>) -> Abort {
    tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.expect("the signal never aborted").unwrap()
}

#[tokio::test]
async fn a_client_that_closes_the_connection_aborts_the_request() {
    let (router, mut rx) = app(Duration::from_secs(60));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await });
    let started = std::time::Instant::now();
    tokio::task::spawn_blocking(move || {
        let mut socket = std::net::TcpStream::connect(addr).unwrap();
        socket.write_all(b"GET /slow HTTP/1.1\r\nhost: localhost\r\n\r\n").unwrap();
        std::thread::sleep(Duration::from_millis(100));
    })
    .await
    .unwrap();
    assert_eq!(next(&mut rx).await, Abort::Disconnected);
    println!("HTTP/1.1 disconnect seen {:?} after connecting (100 ms of that before closing)", started.elapsed());
}

#[tokio::test]
async fn a_handler_past_the_deadline_gets_the_error_page() {
    let (router, mut rx) = app(Duration::from_millis(50));
    let res = router.oneshot(Request::get("/slow").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), 503);
    let body = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(body.contains("<html"), "the error page, not an empty 503: {body}");
    assert_eq!(next(&mut rx).await, Abort::Deadline);
}

#[tokio::test]
async fn an_answered_request_ends_its_work() {
    let (router, mut rx) = app(Duration::from_secs(60));
    let res = router.oneshot(Request::get("/quick").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(next(&mut rx).await, Abort::Responded);
}
