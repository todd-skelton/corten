//! What the tests of each screen share: the app's router and requests to it.
use corten::axum::{Router, body::Body, http::Request, response::Response};
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::{app::app, market::Market};

/// A list id, as the app writes one.
pub(crate) const LIST: &str = "list=0123456789abcdef0123456789abcdef";

pub(crate) fn router() -> Router {
    app(Market::new()).router()
}

pub(crate) async fn call_on(router: &Router, req: Request<Body>) -> Response {
    router.clone().oneshot(req).await.unwrap()
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
