//! What the tests of each screen share: the app's router and requests to it.
use corten::axum::{Router, body::Body, http::Request, response::Response};
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::{
    alerts::AlertStore,
    app::{app, app_with},
    market::Market,
};

/// A visitor id, as the app writes one.
pub(crate) const VISITOR: &str = "visitor=0123456789abcdef0123456789abcdef";

pub(crate) fn router() -> Router {
    app(Market::new()).router()
}

/// The router with an alert store whose writes land `lag` after they are made: at once for a test
/// that reads one back, after the test for one that reads while they land.
pub(crate) fn router_with_lag(lag: std::time::Duration) -> Router {
    app_with(Market::new(), AlertStore::new(lag)).router()
}

pub(crate) async fn call_on(router: &Router, req: Request<Body>) -> Response {
    router.clone().oneshot(req).await.unwrap()
}

/// Signs in on `router` and gives the cookie that carries the new session (`session=...`).
pub(crate) async fn sign_in(router: &Router) -> String {
    let res = call_on(router, post("/sign-in", None, None, "")).await;
    let set = res.headers()["set-cookie"].to_str().unwrap();
    set.split(';').next().unwrap().to_string()
}

pub(crate) async fn call(req: Request<Body>) -> Response {
    call_on(&router(), req).await
}

pub(crate) async fn text(res: Response) -> String {
    String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

pub(crate) async fn send(req: Request<Body>) -> (String, String) {
    let res = call(req).await;
    let kind = res.headers()["content-type"].to_str().unwrap().to_string();
    (kind, text(res).await)
}

pub(crate) fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

pub(crate) fn get_with(uri: &str, cookie: &str) -> Request<Body> {
    Request::get(uri).header("cookie", cookie).body(Body::empty()).unwrap()
}

pub(crate) fn post(uri: &str, mode: Option<&str>, accept: Option<&str>, body: &'static str) -> Request<Body> {
    post_with(uri, mode, accept, None, body)
}

pub(crate) fn post_with(
    uri: &str,
    mode: Option<&str>,
    accept: Option<&str>,
    cookie: Option<&str>,
    body: &'static str,
) -> Request<Body> {
    let mut req = Request::post(uri).header("content-type", "application/x-www-form-urlencoded");
    for (name, value) in [("corten", mode), ("accept", accept), ("cookie", cookie)] {
        if let Some(value) = value {
            req = req.header(name, value);
        }
    }
    req.body(Body::from(body)).unwrap()
}
