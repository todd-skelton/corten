//! Browser security defaults: a Content-Security-Policy with a fresh nonce per request, the usual
//! hardening headers, a same-origin check for writes (CSRF), and session cookie helpers.
//!
//! ```ignore
//! let app = corten::security::Policy::new().script_src("https://js.stripe.com").apply(router);
//! ```
//!
//! Corten's own inline tag (the import map) is the same on every page of a deploy, so the CSP
//! allows it by its SHA-256 hash. [`crate::Req::nonce`] gives the layout a nonce for any
//! inline script the app adds. Nothing else inline runs, so an HTML injection cannot execute
//! script even if a template escapes wrongly.
//!
//! The nonce is made the first time something asks for it (the layout, or a handler that takes
//! [`Nonce`]). A response nothing asked for (most pages, JSON, a patch, a static file) gets the
//! policy without a nonce source: it allows less, costs no random bytes, and the same page is the
//! same bytes for every request, so a cache may share it ([`crate::App`], caching).

use std::{
    cell::RefCell,
    convert::Infallible,
    sync::{Arc, OnceLock},
};

use axum::{
    extract::{FromRequestParts, Request},
    http::{header, request::Parts, HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};

/// The CSP nonce for this request: `req.nonce()` in a layout, or `nonce: Nonce` as an
/// extractor in a handler that writes HTML itself. Asking for it is what puts it in the
/// response's CSP.
#[derive(Clone, Debug)]
pub struct Nonce(pub Arc<str>);

impl<S: Send + Sync> FromRequestParts<S> for Nonce {
    type Rejection = Infallible;

    /// Without a [`Policy`] nothing checks a nonce, so any fresh one is as good.
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(parts.extensions.get::<NonceSlot>().map_or_else(Nonce::generate, |slot| slot.get().clone()))
    }
}

/// Where the request's nonce is made, at most once, when something asks for it.
#[derive(Clone, Default)]
pub(crate) struct NonceSlot(Arc<OnceLock<Nonce>>);

impl NonceSlot {
    pub fn get(&self) -> &Nonce {
        self.0.get_or_init(Nonce::generate)
    }

    pub fn taken(&self) -> Option<&Nonce> {
        self.0.get()
    }
}

impl Nonce {
    /// 128 random bits from the operating system, hex encoded (hex is valid base64 text).
    pub fn generate() -> Self {
        Nonce(std::str::from_utf8(&random_hex()).expect("hex is ASCII").into())
    }
}

/// 128 random bits from the operating system as 32 lowercase hex digits.
pub(crate) fn random_hex() -> [u8; 32] {
    // One system call per 64 nonces: each thread reads 1 KiB of OS randomness and hands it out
    // 16 bytes at a time, never twice. The bytes are as unpredictable as a fresh read.
    thread_local! {
        static POOL: RefCell<([u8; 1024], usize)> = const { RefCell::new(([0; 1024], 1024)) };
    }
    let bytes: [u8; 16] = POOL.with_borrow_mut(|(pool, used)| {
        if *used == pool.len() {
            getrandom::fill(pool).expect("the OS random source is available");
            *used = 0;
        }
        let bytes = pool[*used..*used + 16].try_into().expect("16 bytes");
        pool[*used..*used + 16].fill(0);
        *used += 16;
        bytes
    });
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = [0u8; 32];
    for (i, b) in bytes.iter().enumerate() {
        text[2 * i] = HEX[(b >> 4) as usize];
        text[2 * i + 1] = HEX[(b & 15) as usize];
    }
    text
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
    form_action: Vec<String>,
    style_attributes: bool,
    hsts: bool,
    allowed_origins: Vec<String>,
    report_only: bool,
    popups: bool,
    /// The names the app answers to besides `localhost` and IP addresses; `None`: any.
    hosts: Option<Vec<String>>,
    /// The proxies whose `X-Forwarded-For` [`crate::ClientIp`] follows.
    proxies: Vec<crate::client::Network>,
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
            form_action: Vec::new(),
            style_attributes: false,
            hsts: false,
            allowed_origins: Vec::new(),
            report_only: false,
            popups: false,
            hosts: Some(Vec::new()),
            proxies: Vec::new(),
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

    /// Another origin a form may send the user to (`form-action`): a hosted checkout, an identity
    /// provider's `form_post`, a sibling app. It also allows a form's handler to redirect there
    /// (`req.redirect(&uri)`), since Chromium and WebKit apply `form-action` to the redirects a
    /// submission follows. Without it a form goes to this origin only. A form posted straight to
    /// another app also needs that app to accept posts from this origin (most frameworks refuse a
    /// cross-origin post as CSRF), so posting here and redirecting is usually simpler. Writes
    /// coming in from another site are [`Policy::allow_origin`].
    pub fn form_action(mut self, source: &str) -> Self {
        self.form_action.push(checked(source));
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

    /// A name the app answers to (`shop.example`; `.shop.example` for it and every subdomain).
    /// By default a request is answered only when its `Host` is `localhost` (or a name under
    /// it), an IP address or missing; any other gets `421 Misdirected Request` before routing.
    /// Otherwise a page on a name someone else controls could point that name at this machine
    /// (DNS rebinding) and use every route as if it were the app's own origin, the same-origin
    /// check included, which matters most for an app on loopback, where "only this machine can
    /// reach it" is the whole access model. A deployed app lists the names it is served under
    /// (behind a proxy that rewrites `Host`, the name the proxy sends). A name with a port, a
    /// scheme or a path is a startup panic:
    ///
    /// ```should_panic
    /// corten::security::Policy::new().host("https://shop.example");
    /// ```
    pub fn host(mut self, name: &str) -> Self {
        let name = name.trim_end_matches('.').to_ascii_lowercase();
        let bare = name.strip_prefix('.').unwrap_or(&name);
        assert!(
            !bare.is_empty() && bare.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.'),
            "corten: Policy::host takes a name (shop.example, or .shop.example for its subdomains too), not {name:?}"
        );
        if let Some(hosts) = &mut self.hosts {
            hosts.push(name);
        }
        self
    }

    /// Answers whatever `Host` a request names, for an app whose names are not known in advance
    /// (one served under each customer's own domain) and that checks them itself. See
    /// [`Policy::host`] for what this gives up.
    pub fn any_host(mut self) -> Self {
        self.hosts = None;
        self
    }

    /// True when the app answers to the request's host: `localhost` or a name under it, an IP
    /// address, a name given to [`Policy::host`], or no host at all (HTTP/1.0; a test's request).
    pub(crate) fn answers(&self, req: &Request) -> bool {
        let Some(hosts) = &self.hosts else { return true };
        let host = match req.uri().host() {
            Some(host) => host.as_bytes(),
            None => match req.headers().get(header::HOST) {
                Some(host) => host.as_bytes(),
                None => return true,
            },
        };
        let named = answers(host, hosts);
        if !named {
            refused_host(host);
        }
        named
    }

    /// A proxy in front of the app (`10.0.0.5`), or a network of them (`10.0.0.0/8`, `fc00::/7`),
    /// whose `X-Forwarded-For` [`ClientIp`](crate::ClientIp) follows back to the client. Without
    /// one the client is the connection's peer, and the header is not read: anyone can write it.
    /// List only the addresses requests come from when they come through the proxy: a client
    /// that can reach the app from a listed address can name any address it likes. A deploy sets
    /// `PROXIES` (comma separated) where it sets `HOST` and `PORT`. Anything but an address or a
    /// network, or a network with host bits set, is a startup panic:
    ///
    /// ```should_panic
    /// corten::security::Policy::new().proxy("10.0.0.1/8");
    /// ```
    pub fn proxy(mut self, network: &str) -> Self {
        let parsed = crate::client::Network::parse(network.trim());
        let Some(parsed) = parsed else {
            panic!("corten: Policy::proxy takes an address (10.0.0.5) or a network (10.0.0.0/8) with no host bits set, not {network:?}")
        };
        self.proxies.push(parsed);
        self
    }

    /// The proxies [`Policy::proxy`] trusts.
    pub(crate) fn proxies(&self) -> &[crate::client::Network] {
        &self.proxies
    }

    /// Sends the CSP as `Content-Security-Policy-Report-Only`, to try a policy on a live site.
    pub fn report_only(mut self) -> Self {
        self.report_only = true;
        self
    }

    /// The CSP header value for one nonce.
    pub fn csp(&self, nonce: &str) -> String {
        self.csp_with(Some(nonce), &crate::import_map(None, None))
    }

    /// The CSP, with `'nonce-...'` sources only when there is a nonce. `import_map` is the text of
    /// the page's import map, allowed by its hash.
    fn csp_with(&self, nonce: Option<&str>, import_map: &str) -> String {
        let list = |extra: &[String]| extra.iter().fold(String::new(), |acc, s| acc + " " + s);
        let nonce = nonce.map_or_else(String::new, |n| format!(" 'nonce-{n}'"));
        let import_map = hash_source(import_map);
        let mut csp = format!(
            "default-src 'self'; script-src 'self' {import_map}{nonce}{}; style-src 'self'{nonce}{}; img-src 'self'{}; \
             connect-src 'self'{}; font-src 'self'{}; frame-src {}; object-src 'none'; base-uri 'self'; form-action 'self'{}; \
             frame-ancestors 'none'; worker-src 'self'; manifest-src 'self'",
            list(&self.script),
            list(&self.style),
            list(&self.img),
            list(&self.connect),
            list(&self.font),
            if self.frame.is_empty() { "'none'".to_string() } else { list(&self.frame).trim_start().to_string() },
            list(&self.form_action),
        );
        if self.style_attributes {
            csp.push_str("; style-src-attr 'unsafe-inline'");
        }
        csp
    }

    /// Wraps every route of `router`: writes from other sites get 403, every response gets the
    /// headers, and handlers see the request's [`Nonce`]. [`crate::App`] applies its policy
    /// itself; this is for a router served without one.
    pub fn apply<S: Clone + Send + Sync + 'static>(self, router: Router<S>) -> Router<S> {
        let policy = Arc::new(Applied::new(self, &crate::import_map(None, None)));
        router.layer(middleware::from_fn(move |mut req: Request, next: Next| {
            let policy = policy.clone();
            async move {
                if !policy.answers(&req) {
                    return misdirected();
                }
                if let Some(refused) = policy.refuse(req.method(), req.headers(), &[]) {
                    return refused;
                }
                let nonce = NonceSlot::default();
                req.extensions_mut().insert(nonce.clone());
                let mut res = next.run(req).await;
                policy.headers(&mut res, Some(&nonce));
                res
            }
        }))
    }
}

/// `'sha256-...'`, the CSP source that allows the inline element whose text is `text`.
pub(crate) fn hash_source(text: &str) -> String {
    format!("'sha256-{}'", base64(&sha256(text.as_bytes())))
}

/// SHA-256 (FIPS 180-4), for the CSP hashes of Corten's inline tags. They are computed once at
/// startup, so a small plain implementation serves instead of a dependency.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01,
        0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
        0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
        0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08,
        0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (word, bytes) in w.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for (k, w) in K.iter().zip(w) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(choice).wrapping_add(*k).wrapping_add(w);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(majority);
            (hh, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
        }
        for (state, add) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *state = state.wrapping_add(add);
        }
    }
    let mut out = [0u8; 32];
    for (bytes, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *bytes = word.to_be_bytes();
    }
    out
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let byte = |i: usize| u32::from(chunk.get(i).copied().unwrap_or(0));
        let n = (byte(0) << 16) | (byte(1) << 8) | byte(2);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A policy in use: its CSP split around the nonce, and the headers that do not change.
pub(crate) struct Applied {
    policy: Policy,
    csp_parts: Vec<String>,
    without_nonce: HeaderValue,
    api_csp: HeaderValue,
}

impl std::ops::Deref for Applied {
    type Target = Policy;
    fn deref(&self) -> &Policy {
        &self.policy
    }
}

/// The CSP of JSON-only routes ([`crate::App::api`]): nothing loads, nothing frames them.
const API_CSP: &str = "default-src 'none'; frame-ancestors 'none'";

impl Applied {
    /// `import_map` is the text of the app's import map, allowed by its hash.
    pub fn new(policy: Policy, import_map: &str) -> Self {
        // The header is the same text around each nonce, so split it once here.
        let csp_parts = policy.csp_with(Some("\0"), import_map).split('\0').map(str::to_string).collect();
        let without_nonce = HeaderValue::try_from(policy.csp_with(None, import_map)).expect("CSP sources are checked tokens");
        Applied { policy, csp_parts, without_nonce, api_csp: HeaderValue::from_static(API_CSP) }
    }

    /// The 403 for a write another site sent, if this is one. `origins` are the extra origins
    /// allowed for this request's route (an API mount's).
    pub fn refuse(&self, method: &Method, headers: &HeaderMap, origins: &[String]) -> Option<Response> {
        if self.same_origin(method, headers) || allowed(headers, origins) {
            return None;
        }
        Some((StatusCode::FORBIDDEN, "Cross-site request refused.").into_response())
    }

    /// The security headers. `nonce` is the request's slot, or `None` for a response that is
    /// never a page (an API route, a refused write), which gets a CSP that allows nothing.
    pub fn headers(&self, res: &mut Response, nonce: Option<&NonceSlot>) {
        let headers = res.headers_mut();
        // Room for the headers below and `Cache-Control` (`Vary` from compression is appended to an
        // existing one, or added): the map grows at most once, and not at all when a page reserved
        // enough. Asking for more than this reallocates an error page's map for nothing.
        headers.reserve(6 + usize::from(self.hsts) + usize::from(!headers.contains_key(header::VARY)));
        let csp_header =
            if self.report_only { header::CONTENT_SECURITY_POLICY_REPORT_ONLY } else { header::CONTENT_SECURITY_POLICY };
        let csp = match nonce.map(|slot| slot.taken()) {
            None => Some(self.api_csp.clone()),
            Some(None) => Some(self.without_nonce.clone()),
            Some(Some(nonce)) => HeaderValue::try_from(self.csp_parts.join(&*nonce.0)).ok(),
        };
        if let Some(csp) = csp {
            headers.insert(csp_header, csp);
        }
        headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
        headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("strict-origin-when-cross-origin"));
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
        let opener = if self.popups { "same-origin-allow-popups" } else { "same-origin" };
        headers.insert(HeaderName::from_static("cross-origin-opener-policy"), HeaderValue::from_static(opener));
        if self.hsts {
            headers.insert(header::STRICT_TRANSPORT_SECURITY, HeaderValue::from_static("max-age=31536000; includeSubDomains"));
        }
    }
}

/// True when the request's `Origin` is one of `origins` and it carries no cookies: a site allowed
/// for an API authenticates with a token, so a write it sends with the user's cookies (a
/// `SameSite=None` session and `credentials: "include"`) is refused like any cross-site write.
fn allowed(headers: &HeaderMap, origins: &[String]) -> bool {
    !origins.is_empty()
        && !headers.contains_key(header::COOKIE)
        && headers
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|origin| origins.iter().any(|o| o.eq_ignore_ascii_case(origin.trim_end_matches('/'))))
}

impl Policy {
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

/// True when `host` (a `Host` header: a name or an address, and maybe a port) is `localhost` or a
/// name under it, an IP address, or one of `names` (`.name`: it and its subdomains).
fn answers(host: &[u8], names: &[String]) -> bool {
    // `[::1]:3000`: an IPv6 address is always in brackets.
    if host.first() == Some(&b'[') {
        return true;
    }
    let name = match host.iter().rposition(|&b| b == b':') {
        Some(colon) if host[colon + 1..].iter().all(u8::is_ascii_digit) => &host[..colon],
        Some(_) => return false,
        None => host,
    };
    let name = name.strip_suffix(b".").unwrap_or(name);
    if name.eq_ignore_ascii_case(b"localhost") || ends_with(name, b".localhost") {
        return true;
    }
    // An IPv4 address: digits and dots, which no DNS name a browser resolves is.
    if !name.is_empty() && name.iter().all(|&b| b.is_ascii_digit() || b == b'.') {
        return std::str::from_utf8(name).is_ok_and(|n| n.parse::<std::net::Ipv4Addr>().is_ok());
    }
    names.iter().any(|allowed| match allowed.strip_prefix('.') {
        Some(domain) => name.eq_ignore_ascii_case(domain.as_bytes()) || ends_with(name, allowed.as_bytes()),
        None => name.eq_ignore_ascii_case(allowed.as_bytes()),
    })
}

fn ends_with(name: &[u8], suffix: &[u8]) -> bool {
    name.len() > suffix.len() && name[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// Logs the first refused host of the process: a deployed app that has not listed its name
/// answers 421 to everyone, and this says why.
#[cold]
fn refused_host(host: &[u8]) {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        let host = String::from_utf8_lossy(host);
        tracing::warn!(
            "corten: refused a request for host {host:?} (421): if the app is served under that name, list it with Policy::host"
        );
    }
}

/// The answer to a request for a host the app does not answer to.
pub(crate) fn misdirected() -> Response {
    (StatusCode::MISDIRECTED_REQUEST, "Misdirected request.").into_response()
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
    fn the_app_answers_to_localhost_ip_addresses_and_its_own_names() {
        let names = ["shop.example".to_string(), ".corp.example".to_string()];
        for host in [
            "localhost",
            "localhost:3000",
            "LocalHost.",
            "app.localhost:8080",
            "127.0.0.1",
            "127.0.0.1:3917",
            "10.0.0.7:8080",
            "[::1]:3000",
            "[::1]",
            "shop.example",
            "Shop.Example:443",
            "corp.example",
            "admin.corp.example",
        ] {
            assert!(answers(host.as_bytes(), &names), "{host}");
        }
        for host in [
            "rebound.attacker.example:3917",
            "localhost.attacker.example",
            "evil-localhost",
            "1.2.3.4.attacker.example",
            "999.1.1.1",
            "www.shop.example",
            "shop.example.attacker.example",
            "xcorp.example",
            "localhost:3000:1",
            "localhost:abc",
            "",
        ] {
            assert!(!answers(host.as_bytes(), &names), "{host}");
        }
    }

    #[test]
    #[should_panic(expected = "Policy::host takes a name")]
    fn a_host_with_a_port_is_a_startup_panic() {
        let _ = Policy::new().host("shop.example:443");
    }

    #[test]
    fn hash_sources_match_the_browsers_sha256() {
        // Known answers (FIPS 180-4 examples, and Node's crypto for the rest), across block sizes.
        assert_eq!(hash_source(""), "'sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU='");
        assert_eq!(hash_source("abc"), "'sha256-ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0='");
        assert_eq!(
            hash_source("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "'sha256-JI1qYdIGOLjlwCaTDD5gOaM85Flk/yFn9uzt1BnbBsE='"
        );
        assert_eq!(hash_source(&"a".repeat(1000)), "'sha256-Qe3s5C1j6Nm/UVqbppMuHCDLyfWl0TRkWttdsblzfqM='");
        assert_eq!(hash_source("x-a,x-b{display:contents}"), "'sha256-00iQbWJAoniMipHb+WLUVm3oMINcwW9byUcwNquEbzc='");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"a"), "YQ==");
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
        let import_map = hash_source(&crate::import_map(None, None));
        assert!(csp.contains(&format!("script-src 'self' {import_map} 'nonce-abc' https://js.stripe.com;")), "{csp}");
        assert!(csp.contains("style-src 'self' 'nonce-abc';"));
        let plain = Policy::new().csp_with(None, "{}");
        assert!(plain.contains(&format!("script-src 'self' {};", hash_source("{}"))) && !plain.contains("nonce"), "{plain}");
        assert!(csp.contains("frame-src https://js.stripe.com;"));
        assert!(Policy::new().csp("x").contains("frame-src 'none';"));
        assert!(!csp.contains("unsafe-inline"));
        assert!(csp.contains("form-action 'self';"), "a form goes to this origin only: {csp}");
        let checkout = Policy::new().form_action("https://pay.example").form_action("https://id.example").csp("abc");
        assert!(checkout.contains("form-action 'self' https://pay.example https://id.example;"), "{checkout}");
    }

    #[test]
    #[should_panic(expected = "a CSP source is one token")]
    fn sources_cannot_inject_directives() {
        let _ = Policy::new().script_src("https://a.com; script-src *");
    }

    #[test]
    fn nonces_are_unique_hex() {
        // More than one pool's worth, so a refill is crossed.
        let nonces: std::collections::HashSet<String> = (0..200).map(|_| Nonce::generate().0.to_string()).collect();
        assert_eq!(nonces.len(), 200);
        assert!(nonces.iter().all(|n| n.len() == 32 && n.bytes().all(|c| c.is_ascii_hexdigit())));
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
