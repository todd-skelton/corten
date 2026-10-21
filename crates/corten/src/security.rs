//! Browser security defaults: a Content-Security-Policy with a fresh nonce per request, the usual
//! hardening headers, a same-origin check for writes (CSRF), and session cookie helpers.
//!
//! ```ignore
//! let app = corten::security::Policy::new().script_src("https://js.stripe.com").apply(router);
//! ```
//!
//! Corten's own inline tags (the import map, the island style rule) carry the nonce, and
//! [`crate::Head::nonce`] gives it to the shell for any inline script the app adds. Nothing else
//! inline runs, so an HTML injection cannot execute script even if a template escapes wrongly.

use std::{fmt::Write as _, sync::Arc};

use axum::{
    extract::Request,
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};

/// The CSP nonce for this request. [`Policy::apply`] puts it in the request extensions and
/// [`crate::Req`] passes it to [`crate::Head`].
#[derive(Clone, Debug)]
pub struct Nonce(pub Arc<str>);

impl Nonce {
    /// 128 random bits from the operating system, hex encoded (hex is valid base64 text).
    pub fn generate() -> Self {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("the OS random source is available");
        let mut text = String::with_capacity(32);
        for b in bytes {
            let _ = write!(text, "{b:02x}");
        }
        Nonce(text.into())
    }
}

/// Security policy for every response of a router. Build it once at startup.
#[derive(Clone, Debug)]
pub struct Policy {
    script: Vec<String>,
    style: Vec<String>,
    img: Vec<String>,
    connect: Vec<String>,
    frame: Vec<String>,
    font: Vec<String>,
    style_attributes: bool,
    hsts: bool,
    allowed_origins: Vec<String>,
    report_only: bool,
    popups: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self::new()
    }
}

impl Policy {
    /// Same-origin everything, `https:` and `data:` images, no frames, no plugins.
    pub fn new() -> Self {
        Policy {
            script: Vec::new(),
            style: Vec::new(),
            img: vec!["data:".into(), "https:".into()],
            connect: Vec::new(),
            frame: Vec::new(),
            font: Vec::new(),
            style_attributes: false,
            hsts: false,
            allowed_origins: Vec::new(),
            report_only: false,
            popups: false,
        }
    }

    /// Another origin scripts may load from (a payment SDK, for example).
    pub fn script_src(mut self, source: &str) -> Self {
        self.script.push(checked(source));
        self
    }

    pub fn style_src(mut self, source: &str) -> Self {
        self.style.push(checked(source));
        self
    }

    pub fn img_src(mut self, source: &str) -> Self {
        self.img.push(checked(source));
        self
    }

    /// Another origin `fetch` and `EventSource` may reach.
    pub fn connect_src(mut self, source: &str) -> Self {
        self.connect.push(checked(source));
        self
    }

    /// An origin the page may embed in an `<iframe>` (payment fields, for example).
    pub fn frame_src(mut self, source: &str) -> Self {
        self.frame.push(checked(source));
        self
    }

    pub fn font_src(mut self, source: &str) -> Self {
        self.font.push(checked(source));
        self
    }

    /// Allows `style="..."` attributes in markup. Off by default; setting `element.style` from
    /// script is always allowed.
    pub fn allow_style_attributes(mut self) -> Self {
        self.style_attributes = true;
        self
    }

    /// Sends `Strict-Transport-Security` (one year, subdomains). Only for sites served over HTTPS.
    pub fn hsts(mut self) -> Self {
        self.hsts = true;
        self
    }

    /// Another origin allowed to send writes with the user's cookies (`https://admin.example.com`).
    pub fn allow_origin(mut self, origin: &str) -> Self {
        self.allowed_origins.push(origin.trim_end_matches('/').to_ascii_lowercase());
        self
    }

    /// Lets popups this site opens keep `window.opener` (`Cross-Origin-Opener-Policy:
    /// same-origin-allow-popups`), for sign-in or payment flows that report back from a popup.
    /// Without it the policy is `same-origin`.
    pub fn allow_popups(mut self) -> Self {
        self.popups = true;
        self
    }

    /// Sends the CSP as `Content-Security-Policy-Report-Only`, to try a policy on a live site.
    pub fn report_only(mut self) -> Self {
        self.report_only = true;
        self
    }

    /// The CSP header value for one nonce.
    pub fn csp(&self, nonce: &str) -> String {
        let list = |extra: &[String]| extra.iter().fold(String::new(), |acc, s| acc + " " + s);
        let mut csp = format!(
            "default-src 'self'; script-src 'self' 'nonce-{nonce}'{}; style-src 'self' 'nonce-{nonce}'{}; img-src 'self'{}; \
             connect-src 'self'{}; font-src 'self'{}; frame-src {}; object-src 'none'; base-uri 'self'; form-action 'self'; \
             frame-ancestors 'none'; worker-src 'self'; manifest-src 'self'",
            list(&self.script),
            list(&self.style),
            list(&self.img),
            list(&self.connect),
            list(&self.font),
            if self.frame.is_empty() { "'none'".to_string() } else { list(&self.frame).trim_start().to_string() },
        );
        if self.style_attributes {
            csp.push_str("; style-src-attr 'unsafe-inline'");
        }
        csp
    }

    /// Wraps every route of `router`: writes from other sites get 403, every response gets the
    /// headers, and handlers see the request's [`Nonce`].
    pub fn apply<S: Clone + Send + Sync + 'static>(self, router: Router<S>) -> Router<S> {
        let policy = Arc::new(self);
        router.layer(middleware::from_fn(move |req: Request, next: Next| {
            let policy = policy.clone();
            async move { policy.handle(req, next).await }
        }))
    }

    async fn handle(&self, mut req: Request, next: Next) -> Response {
        if !self.same_origin(req.method(), req.headers()) {
            return (StatusCode::FORBIDDEN, "Cross-site request refused.").into_response();
        }
        let nonce = Nonce::generate();
        req.extensions_mut().insert(nonce.clone());
        let mut res = next.run(req).await;
        let headers = res.headers_mut();
        let csp_header =
            if self.report_only { header::CONTENT_SECURITY_POLICY_REPORT_ONLY } else { header::CONTENT_SECURITY_POLICY };
        if let Ok(value) = HeaderValue::try_from(self.csp(&nonce.0)) {
            headers.insert(csp_header, value);
        }
        headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
        headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("strict-origin-when-cross-origin"));
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
        let opener = if self.popups { "same-origin-allow-popups" } else { "same-origin" };
        headers.insert(HeaderName::from_static("cross-origin-opener-policy"), HeaderValue::from_static(opener));
        if self.hsts {
            headers.insert(header::STRICT_TRANSPORT_SECURITY, HeaderValue::from_static("max-age=31536000; includeSubDomains"));
        }
        res
    }

    /// CSRF check for writes. Every current browser sends `Sec-Fetch-Site`, which page script
    /// cannot set: `same-origin` is accepted as is (so a proxy that rewrites `Host` cannot break
    /// writes) and anything cross-site is refused. Older browsers send only `Origin`, which must
    /// then match `Host`. A request with neither header is not from a browser (a native app, an
    /// agent, a webhook); those authenticate with tokens, not ambient cookies, so it passes.
    pub fn same_origin(&self, method: &Method, headers: &HeaderMap) -> bool {
        if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
            return true;
        }
        let origin =
            headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()).map(|o| o.trim_end_matches('/').to_ascii_lowercase());
        if let Some(origin) = &origin {
            if self.allowed_origins.iter().any(|allowed| allowed == origin) {
                return true;
            }
        }
        match headers.get("sec-fetch-site").map(HeaderValue::as_bytes) {
            // `none`: the user started it (typed URL, bookmark); no other site can cause it.
            Some(b"same-origin" | b"none") => return true,
            Some(_) => return false,
            None => {}
        }
        match origin {
            None => true,
            Some(origin) if origin == "null" => false,
            Some(origin) => {
                let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("").to_ascii_lowercase();
                origin.split_once("://").is_some_and(|(_, rest)| rest == host)
            }
        }
    }
}

/// Rejects a source that would break out of its directive.
fn checked(source: &str) -> String {
    assert!(
        !source.is_empty() && !source.contains([';', ',', '\r', '\n', ' ']),
        "a CSP source is one token without ';', ',' or spaces: {source:?}"
    );
    source.to_string()
}

/// Cookie helpers. Sessions belong to the app; these make the safe flags the default.
pub mod cookie {
    use axum::http::{header, HeaderMap, HeaderValue};

    /// The value of cookie `name` in the request, if present.
    pub fn get<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
        headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|pair| {
            let (k, v) = pair.trim().split_once('=')?;
            (k == name).then_some(v)
        })
    }

    /// A `Set-Cookie` value: `HttpOnly; Secure; SameSite=Lax; Path=/` unless changed.
    #[derive(Clone, Debug)]
    pub struct SetCookie {
        name: String,
        value: String,
        max_age: Option<i64>,
        http_only: bool,
        secure: bool,
        same_site: &'static str,
        path: String,
    }

    impl SetCookie {
        /// Panics if the name or value contains characters a cookie cannot carry; a session token
        /// should be URL-safe base64 or hex.
        pub fn new(name: &str, value: &str) -> Self {
            let token = |s: &str| s.bytes().all(|b| b.is_ascii_graphic() && !matches!(b, b';' | b',' | b'"' | b'\\' | b'='));
            assert!(!name.is_empty() && token(name), "invalid cookie name {name:?}");
            assert!(
                value.bytes().all(|b| b.is_ascii_graphic() && !matches!(b, b';' | b',' | b'"' | b'\\')),
                "invalid cookie value"
            );
            SetCookie {
                name: name.into(),
                value: value.into(),
                max_age: None,
                http_only: true,
                secure: true,
                same_site: "Lax",
                path: "/".into(),
            }
        }

        /// Expires the cookie in the browser (sign-out).
        pub fn remove(name: &str) -> Self {
            Self::new(name, "").max_age(0)
        }

        pub fn max_age(mut self, seconds: i64) -> Self {
            self.max_age = Some(seconds);
            self
        }

        /// For local development over plain HTTP only.
        pub fn insecure(mut self) -> Self {
            self.secure = false;
            self
        }

        /// Readable from script. Never for a session.
        pub fn script_readable(mut self) -> Self {
            self.http_only = false;
            self
        }

        pub fn strict(mut self) -> Self {
            self.same_site = "Strict";
            self
        }

        pub fn path(mut self, path: &str) -> Self {
            assert!(path.starts_with('/') && !path.contains(';'), "invalid cookie path");
            self.path = path.into();
            self
        }

        pub fn header(&self) -> HeaderValue {
            let mut out = format!("{}={}; Path={}; SameSite={}", self.name, self.value, self.path, self.same_site);
            if let Some(age) = self.max_age {
                out.push_str(&format!("; Max-Age={age}"));
            }
            if self.http_only {
                out.push_str("; HttpOnly");
            }
            if self.secure {
                out.push_str("; Secure");
            }
            HeaderValue::try_from(out).expect("checked in new")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.insert(*k, HeaderValue::from_static(v));
        }
        map
    }

    #[test]
    fn writes_from_other_sites_are_refused_and_non_browsers_pass() {
        let p = Policy::new().allow_origin("https://admin.example.com/");
        let post = Method::POST;
        assert!(p.same_origin(&Method::GET, &headers(&[("sec-fetch-site", "cross-site")])), "reads are not checked");
        assert!(
            p.same_origin(&post, &headers(&[("sec-fetch-site", "same-origin"), ("origin", "https://a.com"), ("host", "a.com")]))
        );
        assert!(
            p.same_origin(
                &post,
                &headers(&[("sec-fetch-site", "same-origin"), ("origin", "https://a.com"), ("host", "app:8080")])
            ),
            "a proxy that rewrites Host does not break same-origin writes"
        );
        assert!(!p.same_origin(
            &post,
            &headers(&[("sec-fetch-site", "cross-site"), ("origin", "https://evil.com"), ("host", "a.com")])
        ));
        assert!(!p.same_origin(
            &post,
            &headers(&[("sec-fetch-site", "same-site"), ("origin", "https://sub.a.com"), ("host", "a.com")])
        ));
        assert!(
            !p.same_origin(&post, &headers(&[("origin", "https://evil.com"), ("host", "a.com")])),
            "older browsers: Origin only"
        );
        assert!(!p.same_origin(&post, &headers(&[("origin", "null"), ("host", "a.com")])), "sandboxed frames send null");
        assert!(p.same_origin(&post, &headers(&[("origin", "http://localhost:8080"), ("host", "localhost:8080")])));
        assert!(p.same_origin(
            &post,
            &headers(&[("sec-fetch-site", "same-site"), ("origin", "https://admin.example.com"), ("host", "a.com")])
        ));
        assert!(p.same_origin(&post, &headers(&[])), "native apps and agents send neither header");
    }

    #[test]
    fn csp_carries_the_nonce_and_extra_sources() {
        let csp = Policy::new().script_src("https://js.stripe.com").frame_src("https://js.stripe.com").csp("abc");
        assert!(csp.contains("script-src 'self' 'nonce-abc' https://js.stripe.com;"));
        assert!(csp.contains("style-src 'self' 'nonce-abc';"));
        assert!(csp.contains("frame-src https://js.stripe.com;"));
        assert!(Policy::new().csp("x").contains("frame-src 'none';"));
        assert!(!csp.contains("unsafe-inline"));
    }

    #[test]
    #[should_panic(expected = "a CSP source is one token")]
    fn sources_cannot_inject_directives() {
        let _ = Policy::new().script_src("https://a.com; script-src *");
    }

    #[test]
    fn nonces_are_unique_hex() {
        let (a, b) = (Nonce::generate(), Nonce::generate());
        assert_ne!(a.0, b.0);
        assert_eq!(a.0.len(), 32);
        assert!(a.0.bytes().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn cookies_default_to_safe_flags_and_parse() {
        let set = cookie::SetCookie::new("__Host-session", "abc123").max_age(3600).header();
        assert_eq!(set, "__Host-session=abc123; Path=/; SameSite=Lax; Max-Age=3600; HttpOnly; Secure");
        assert_eq!(cookie::SetCookie::remove("s").header(), "s=; Path=/; SameSite=Lax; Max-Age=0; HttpOnly; Secure");
        let h = headers(&[("cookie", "a=1; __Host-session=abc123; b=2")]);
        assert_eq!(cookie::get(&h, "__Host-session"), Some("abc123"));
        assert_eq!(cookie::get(&h, "c"), None);
    }
}
