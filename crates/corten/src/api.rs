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
//!   The shell, the error page and [`crate::App::context`] are skipped ([`Api::context`] runs
//!   the context for routes that need the signed-in user).
//! - The deadline, the request [`crate::Signal`], compression, shutdown and the same-origin
//!   check on writes work as they do for pages.
//!
//! ```ignore
//! async fn price(Path(id): Path<String>) -> Json<serde_json::Value> {
//!     Json(serde_json::json!({ "id": id, "lowest": "$0.50" }))
//! }
//!
//! let api = Router::new().route("/api/items/{id}/price", get(price));
//! corten::serve(App::new(pages).api("/api", Api::new(api).allow_origin("https://partner.example"))).await
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

use crate::html::json_string;

/// Routes that answer only JSON, with their settings: what [`crate::App::api`] mounts. A
/// [`Router`] converts into one with no settings, so `.api("/api", routes)` is enough. `C` is the
/// app's context type, which [`Api::guard`] reads, as an [`crate::Area`]'s is. A fallback set on
/// its router is dropped: an unknown URL under the prefix is the JSON 404.
pub struct Api<C> {
    pub(crate) routes: Router,
    pub(crate) origins: Vec<String>,
    pub(crate) context: bool,
    pub(crate) guard: Option<crate::app::GuardFn>,
    context_type: PhantomData<fn() -> C>,
}

impl<C> Api<C> {
    pub fn new(routes: Router) -> Self {
        Api { routes, origins: Vec::new(), context: false, guard: None, context_type: PhantomData }
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

    /// Runs [`crate::App::context`] for these routes too, for an API that serves the app's own
    /// signed-in users (`req.context::<C>()` or axum's `Extension<C>`). Without it, the context
    /// function is not called, which saves its work (a session lookup) on every request that
    /// authenticates another way. An API under a guard runs it anyway, for the guard.
    pub fn context(mut self) -> Self {
        self.context = true;
        self
    }
}

impl<C: Clone + Send + Sync + 'static> Api<C> {
    /// Who may call these routes: every route under the prefix, including unknown URLs and wrong
    /// methods there, needs `check`'s [`crate::Access::ALLOW`], as with [`crate::App::guard`]. A
    /// caller sent to sign in gets a JSON 401, since a JSON client follows no redirect. Given once.
    pub fn guard<F>(mut self, check: F) -> Self
    where
        F: Fn(Option<&C>, &crate::Guarded) -> crate::Access + Send + Sync + 'static,
    {
        assert!(self.guard.is_none(), "corten: an API's guard is given once");
        self.guard = Some(Box::new(move |extensions, req| check(extensions.get::<C>(), req)));
        self
    }
}

impl<C> From<Router> for Api<C> {
    fn from(routes: Router) -> Self {
        Api::new(routes)
    }
}

/// An [`Api`] in use: its prefix and settings, without the routes.
pub(crate) struct Mount {
    pub prefix: String,
    origins: Vec<String>,
    pub context: bool,
}

impl Mount {
    pub fn new<C>(prefix: String, api: &Api<C>) -> Self {
        Mount { prefix, origins: api.origins.clone(), context: api.context }
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

/// Checks a prefix given to `App::api` or `App::area` (`what` names it in the message): `"/"`
/// when `root` allows it, or a path like `/admin`: letters, digits, `-`, `_`, `.` and `~`
/// between slashes, without a trailing slash, parameters or `.` and `..` segments. One rule for
/// both, since an area's prefix also names its stylesheet's file.
pub(crate) fn check_prefix(what: &str, prefix: &str, root: bool) {
    let plain = prefix.len() > 1
        && prefix.starts_with('/')
        && prefix[1..].split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'))
        });
    let root = if root { "\"/\" or " } else { "" };
    assert!(
        plain || (!root.is_empty() && prefix == "/"),
        "corten: {what} prefix is {root}a path like \"/admin\": letters, digits, -, _, . and ~ between slashes, without a trailing slash or parameters: {prefix:?}"
    );
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

/// `res` with its failure as the JSON error body; a success, or a failure with a body of the
/// handler's own that is not plain text, is returned unchanged.
pub(crate) async fn json_failure(res: Response, dev: bool) -> Response {
    let status = res.status();
    if !(status.is_client_error() || status.is_server_error()) {
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
    let body = error_body(status, shown(status, detail.as_deref(), dev));
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

    #[test]
    fn a_prefix_holds_its_own_path_and_those_below_it() {
        assert!(under("/api", "/api") && under("/api", "/api/items/1"));
        assert!(!under("/api", "/apis") && !under("/api", "/") && !under("/api", "/items"));
        assert!(under("/", "/") && under("/", "/anything"));
    }

    #[test]
    #[should_panic(expected = "overlap")]
    fn prefixes_cannot_overlap() {
        let _ = crate::App::new(Router::new()).api("/api", Router::new()).api("/api/v2", Router::new());
    }

    #[test]
    #[should_panic(expected = "without a trailing slash")]
    fn a_prefix_has_no_trailing_slash() {
        check_prefix("an API", "/api/", true);
    }

    #[test]
    fn the_root_is_an_api_prefix_but_not_an_area_prefix() {
        check_prefix("an API", "/", true);
        assert!(std::panic::catch_unwind(|| check_prefix("an area", "/", false)).is_err());
    }

    #[test]
    #[should_panic(expected = "a scheme and a host")]
    fn an_allowed_origin_is_one_origin() {
        let _ = Api::<crate::NoContext>::new(Router::new()).allow_origin("*");
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
