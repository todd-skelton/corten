//! [`Api`]: routes that answer JSON and nothing else, mounted under a path prefix with
//! [`crate::App::api`].
//!
//! A page route answers every request mode, so how it fails depends on what the client asked
//! for: the error page for a browser, JSON for `Accept: application/json`. A route with no screen
//! (a webhook, an OAuth callback, a call between services, a public API) has only programs for
//! clients, most of which send `Accept: */*` or nothing. So under an API prefix:
//!
//! - Every failure is one JSON shape, `{"error":{"status":404,"message":"Not Found"}}`: a
//!   handler's [`crate::Error`], an extractor's rejection (a body that does not parse), an
//!   unknown URL, a wrong method, the deadline. A 4xx message is shown, a 5xx message only under
//!   `corten dev`; the response's other headers (`Allow`, `WWW-Authenticate`) are kept. A failure
//!   the handler answered with a JSON body of its own passes through unchanged.
//! - Responses carry a fixed CSP, `default-src 'none'; frame-ancestors 'none'`, and no nonce.
//!   The shell and the error page are skipped. A route that needs to know who is calling takes a
//!   [`crate::Caller`], as any handler does.
//! - The deadline, the request [`crate::Signal`], compression, shutdown and the same-origin
//!   check on writes work as they do for pages.
//!
//! ```ignore
//! #[corten::routes(api = ApiRoot)]
//! mod api {
//!     #[route("/api")]
//!     pub struct ApiRoot;
//!
//!     #[route("/api/items/{id}/price", get = price)]
//!     pub struct ItemPrice<'a> { pub id: &'a str }
//! }
//!
//! async fn price(Path(id): Path<String>) -> Json<serde_json::Value> {
//!     Json(serde_json::json!({ "id": id, "lowest": "$0.50" }))
//! }
//!
//! corten::serve(App::new(pages).api(|routes| api::api(routes).allow_origin("https://partner.example"))).await
//! ```
//!
//! The same example compiles in `docs/guide/app.md` (JSON-only routes).
//!
//! A screen that a native app or an agent reads is not an API route: it stays a page that answers
//! with [`crate::Req::screen`], so every surface shares one handler.

use std::marker::PhantomData;

use axum::{
    body::{Body, HttpBody},
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    Router,
};

use crate::{
    html::json_string,
    route::{Prefix, Route as TypedRoute},
};

/// Routes that answer only JSON, with their settings, under the prefix `P`: what
/// [`crate::App::api`] gives its function to fill in.
pub struct Api<P> {
    pub(crate) routes: Router,
    pub(crate) origins: Vec<String>,
    deadline: Option<Option<std::time::Duration>>,
    errors: Option<ErrorShape>,
    types: PhantomData<fn() -> P>,
}

/// How an API writes the failures Corten answers for it ([`Api::errors`]): a status and a
/// message to a JSON body.
#[derive(Clone)]
pub(crate) struct ErrorShape(std::sync::Arc<dyn Fn(StatusCode, &str) -> Vec<u8> + Send + Sync>);

impl<P: Prefix> Api<P> {
    pub(crate) fn new() -> Self {
        Api { routes: Router::new(), origins: Vec::new(), deadline: None, errors: None, types: PhantomData }
    }

    /// A route of the API added by hand, at `R::PATH`, for one a routes module cannot name: an
    /// endpoint built from a value, like the MCP endpoint, `.route::<Mcp>(tools.route())`. A
    /// function goes in the API's `#[corten::routes(api = ..)]` module instead. Otherwise
    /// `.route::<ItemJson>(get(item_json))`, with
    /// `.with_state(state)` on the [`crate::Methods`] when its handlers take `State`. `R`'s path is at
    /// or under the prefix, or the app does not build, so no route of the API is served outside
    /// it (and its settings).
    ///
    /// ```compile_fail,E0080
    /// # use corten::prelude::*;
    /// #[derive(Route)]
    /// #[route("/api")]
    /// struct ApiRoot;
    ///
    /// #[derive(Route)]
    /// #[route("/apis/items")]
    /// struct Items;
    ///
    /// let app = App::new(Routes::new()).api::<ApiRoot>(|api| api.route::<Items>(get(|| async { "items" })));
    /// # let _ = app.router();
    /// ```
    pub fn route<R: TypedRoute>(mut self, methods: crate::Methods) -> Self {
        const {
            assert!(
                under_const(P::PREFIX, R::PATH),
                "corten: a route of an API is at or under its prefix (the note below names the API's prefix and the route)"
            )
        };
        self.routes = self.routes.route(R::PATH, methods.0);
        self
    }

    /// How long these routes may take to start their answer, in place of [`crate::App::deadline`]
    /// for every request under the prefix: `.deadline(Duration::from_secs(300))` for an API that
    /// waits on a slow upstream (a model, a report), so the rest of the app keeps its shorter one.
    /// Past it the answer is the JSON 503 and the request's [`crate::Signal`] aborts with
    /// [`crate::Abort::Deadline`], as at the app's. `None` removes it, for routes that bound
    /// themselves.
    pub fn deadline(mut self, deadline: impl Into<Option<std::time::Duration>>) -> Self {
        self.deadline = Some(deadline.into());
        self
    }

    /// The JSON every failure Corten answers under the prefix is written as, in place of
    /// `{"error":{"status","message"}}`, for an API that speaks another protocol's error shape:
    /// an unknown URL (404), a wrong method (405, with `Allow`), a cross-site write (403), the
    /// deadline (503), a request an extractor rejects, a `Caller` that refuses (401 or 403, with
    /// `WWW-Authenticate`) and a handler's `Error`. `message` is what the client may be told: a
    /// 4xx's message, a 5xx's reason (and its message under `corten dev`). A failure with a JSON
    /// body of the handler's own, and a [`crate::Relay`]'s, pass through as they are.
    ///
    /// ```
    /// use corten::{axum::http::StatusCode, prelude::*};
    ///
    /// #[derive(ToJson)]
    /// struct Failure { r#type: &'static str, error: Detail }
    ///
    /// #[derive(ToJson)]
    /// struct Detail { r#type: &'static str, message: String }
    ///
    /// fn shape(status: StatusCode, message: &str) -> Failure {
    ///     let kind = match status.as_u16() {
    ///         401 => "authentication_error",
    ///         403 => "permission_error",
    ///         404 => "not_found_error",
    ///         429 => "rate_limit_error",
    ///         400..=499 => "invalid_request_error",
    ///         _ => "api_error",
    ///     };
    ///     Failure { r#type: "error", error: Detail { r#type: kind, message: message.to_string() } }
    /// }
    ///
    /// #[corten::routes(api = V1)]
    /// mod v1 {
    ///     #[route("/v1")]
    ///     pub struct V1;
    /// }
    ///
    /// let app = App::new(Routes::new()).api(|api| v1::api(api).errors(shape));
    /// # let _ = app.router();
    /// ```
    #[cfg(feature = "json")]
    pub fn errors<T: crate::ToJson>(mut self, write: impl Fn(StatusCode, &str) -> T + Send + Sync + 'static) -> Self {
        self.errors = Some(ErrorShape(std::sync::Arc::new(move |status, message| crate::json::to_vec(&write(status, message)))));
        self
    }

    /// A site whose pages may call these routes from the browser (CORS), like
    /// `https://partner.example`. Corten answers its preflight requests, adds
    /// `Access-Control-Allow-Origin` to the responses, and lets its writes through the same-origin
    /// check, for these routes only: the app's pages still refuse writes from it. The site
    /// authenticates with a token: a write from it that carries cookies is refused, and no
    /// `Access-Control-Allow-Credentials` is sent.
    pub fn allow_origin(mut self, origin: &str) -> Self {
        let origin = origin.trim_end_matches('/').to_ascii_lowercase();
        let host = origin.split_once("://").map_or("", |(_, rest)| rest);
        assert!(
            !host.is_empty() && !host.contains(['/', '*', ',', ' ']),
            "corten: an allowed origin is a scheme and a host, like https://partner.example: {origin:?}"
        );
        self.origins.push(origin);
        self
    }
}

/// An [`Api`] in use: its prefix and settings, without the routes.
pub(crate) struct Mount {
    pub prefix: String,
    origins: Vec<String>,
    /// The API's own deadline, when it set one ([`Api::deadline`]).
    pub deadline: Option<Option<std::time::Duration>>,
    /// How its failures are written, when it says ([`Api::errors`]).
    pub errors: Option<ErrorShape>,
}

impl Mount {
    pub fn new<P>(prefix: String, api: &Api<P>) -> Self {
        Mount { prefix, origins: api.origins.clone(), deadline: api.deadline, errors: api.errors.clone() }
    }

    pub fn contains(&self, path: &str) -> bool {
        under(&self.prefix, path)
    }

    pub fn origins(&self) -> &[String] {
        &self.origins
    }

    /// The request's `Origin` when it is one of this mount's, for the CORS headers.
    pub fn allowed_origin(&self, headers: &HeaderMap) -> Option<HeaderValue> {
        if self.origins.is_empty() {
            return None;
        }
        let origin = headers.get(header::ORIGIN)?;
        let text = origin.to_str().ok()?.trim_end_matches('/');
        self.origins.iter().any(|o| o.eq_ignore_ascii_case(text)).then(|| origin.clone())
    }

    /// The answer to a CORS preflight from one of this mount's origins.
    pub fn preflight(&self, method: &Method, headers: &HeaderMap) -> Option<Response> {
        if method != Method::OPTIONS {
            return None;
        }
        let asked = headers.get(header::ACCESS_CONTROL_REQUEST_METHOD)?.clone();
        let origin = self.allowed_origin(headers)?;
        let mut res = StatusCode::NO_CONTENT.into_response();
        let out = res.headers_mut();
        out.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        out.insert(header::ACCESS_CONTROL_ALLOW_METHODS, asked);
        if let Some(asked) = headers.get(header::ACCESS_CONTROL_REQUEST_HEADERS) {
            out.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, asked.clone());
        }
        out.insert(header::ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("3600"));
        out.insert(header::VARY, HeaderValue::from_static(PREFLIGHT_VARY));
        Some(res)
    }

    /// Adds the CORS headers to a response, when this mount allows any origin.
    pub fn cors(&self, origin: Option<HeaderValue>, res: &mut Response) {
        if self.origins.is_empty() {
            return;
        }
        let headers = res.headers_mut();
        headers.append(header::VARY, HeaderValue::from_static("origin"));
        if let Some(origin) = origin {
            headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        }
    }
}

const PREFLIGHT_VARY: &str = "origin, access-control-request-method, access-control-request-headers";

/// True when `path` is `prefix` or below it (`/api` holds `/api` and `/api/items`, not `/apis`).
pub(crate) fn under(prefix: &str, path: &str) -> bool {
    prefix == "/" || path.strip_prefix(prefix).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// [`under`] when the app compiles: `path` is `prefix` or below it.
pub(crate) const fn under_const(prefix: &str, path: &str) -> bool {
    let (prefix, path) = (prefix.as_bytes(), path.as_bytes());
    if prefix.len() == 1 && prefix[0] == b'/' {
        return true;
    }
    if path.len() < prefix.len() {
        return false;
    }
    let mut i = 0;
    while i < prefix.len() {
        if prefix[i] != path[i] {
            return false;
        }
        i += 1;
    }
    path.len() == prefix.len() || path[prefix.len()] == b'/'
}

/// Whether `prefix` may be given to `App::api` or `App::area`, checked when the app compiles:
/// `"/"` when `root` allows it, or a path like `/admin`: letters, digits, `-`, `_`, `.` and `~`
/// between slashes, without a trailing slash, parameters or `.` and `..` segments. One rule for
/// both, since an area's prefix also names its stylesheet's file.
pub(crate) const fn plain_prefix(prefix: &str, root: bool) -> bool {
    let bytes = prefix.as_bytes();
    if bytes.len() == 1 && bytes[0] == b'/' {
        return root;
    }
    if bytes.len() < 2 || bytes[0] != b'/' {
        return false;
    }
    // Each segment: its start, and whether it is only dots.
    let (mut start, mut i) = (1, 1);
    while i <= bytes.len() {
        if i == bytes.len() || bytes[i] == b'/' {
            let len = i - start;
            let dots = len > 0 && len <= 2 && bytes[start] == b'.' && bytes[i - 1] == b'.';
            if len == 0 || dots {
                return false;
            }
            start = i + 1;
        } else {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~') {
                return false;
            }
        }
        i += 1;
    }
    true
}

/// The JSON body of every failure: `{"error":{"status":404,"message":"Not Found"}}`.
pub(crate) fn error_body(status: StatusCode, message: &str) -> String {
    format!(r#"{{"error":{{"status":{},"message":{}}}}}"#, status.as_u16(), json_string(message))
}

/// What a JSON client is told: the detail for a 4xx (and for anything under `corten dev`), the
/// status's reason for a 5xx.
pub(crate) fn shown(status: StatusCode, detail: Option<&str>, dev: bool) -> &str {
    match detail {
        Some(detail) if dev || !status.is_server_error() => detail,
        _ => status.canonical_reason().unwrap_or("Error"),
    }
}

/// `res` with its failure as the JSON error body (in `shape`, when the API gave one); a success,
/// or a failure with a body of the handler's own that is not plain text, is returned unchanged.
pub(crate) async fn json_failure(res: Response, dev: bool, shape: Option<&ErrorShape>) -> Response {
    let status = res.status();
    if !(status.is_client_error() || status.is_server_error()) || res.extensions().get::<crate::answer::Relayed>().is_some() {
        return res;
    }
    let failed = res.extensions().get::<crate::Failed>().map(|f| f.0.clone());
    let text = failed.is_none() && is_plain_text(res.headers());
    if failed.is_none() && !text && res.body().size_hint().exact() != Some(0) {
        return res;
    }
    let (mut parts, body) = res.into_parts();
    let detail = match failed {
        Some(message) => Some(message.to_string()),
        None if text => {
            let bytes = axum::body::to_bytes(body, 16 * 1024).await.unwrap_or_default();
            let text = String::from_utf8_lossy(&bytes).trim().to_string();
            if status.is_server_error() {
                tracing::error!(status = status.as_u16(), "corten: request failed: {text}");
            } else {
                tracing::debug!(status = status.as_u16(), "corten: request rejected: {text}");
            }
            Some(text).filter(|t| !t.is_empty())
        }
        None => None,
    };
    let shown = shown(status, detail.as_deref(), dev);
    let body = match shape {
        Some(ErrorShape(write)) => write(status, shown),
        None => error_body(status, shown).into_bytes(),
    };
    parts.headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(JSON));
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.remove(HeaderName::from_static("corten-page"));
    Response::from_parts(parts, Body::from(body))
}

pub(crate) const JSON: &str = "application/json";

pub(crate) fn is_plain_text(headers: &HeaderMap) -> bool {
    headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("text/plain"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(crate::Route)]
    #[route("/api")]
    struct ApiRoot;

    #[derive(crate::Route)]
    #[route("/api/v2")]
    struct V2;

    #[test]
    fn a_prefix_holds_its_own_path_and_those_below_it() {
        for (prefix, path, held) in [
            ("/api", "/api", true),
            ("/api", "/api/items/1", true),
            ("/api", "/api/items/{id}", true),
            ("/api", "/apis", false),
            ("/api", "/", false),
            ("/api", "/items", false),
            ("/", "/", true),
            ("/", "/anything", true),
        ] {
            assert_eq!(under(prefix, path), held, "{prefix} {path}");
            assert_eq!(under_const(prefix, path), held, "{prefix} {path}, when the app compiles");
        }
    }

    #[test]
    #[should_panic(expected = "overlap")]
    fn prefixes_cannot_overlap() {
        let _ = crate::App::new(crate::Routes::new()).api::<ApiRoot>(|api| api).api::<V2>(|api| api);
    }

    #[test]
    fn a_prefix_is_a_plain_path() {
        for good in ["/api", "/admin/billing", "/a-b_c.d~e", "/v1.2"] {
            assert!(plain_prefix(good, false), "{good}");
        }
        for bad in ["/api/", "api", "", "//api", "/items/{id}", "/a b", "/.", "/a/..", "/a?b"] {
            assert!(!plain_prefix(bad, true), "{bad}");
        }
        assert!(plain_prefix("/", true) && !plain_prefix("/", false), "the root is an API prefix but not an area prefix");
    }

    #[test]
    #[should_panic(expected = "a scheme and a host")]
    fn an_allowed_origin_is_one_origin() {
        let _ = Api::<ApiRoot>::new().allow_origin("*");
    }

    #[test]
    fn messages_hide_server_errors_outside_dev_and_escape() {
        assert_eq!(shown(StatusCode::NOT_FOUND, Some("no item 7"), false), "no item 7");
        assert_eq!(shown(StatusCode::INTERNAL_SERVER_ERROR, Some("db password"), false), "Internal Server Error");
        assert_eq!(shown(StatusCode::INTERNAL_SERVER_ERROR, Some("db down"), true), "db down");
        assert_eq!(
            error_body(StatusCode::BAD_REQUEST, "a \"b\" <c>"),
            r#"{"error":{"status":400,"message":"a \"b\" \u003cc>"}}"#
        );
    }
}
