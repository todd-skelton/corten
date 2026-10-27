//! A route that switches its connection to another protocol: a WebSocket, or a tunnel that relays
//! an upstream's switch. See [`Upgrade`].

use std::{
    future::Future,
    io,
    pin::{pin, Pin},
    task::{Context, Poll},
};

use axum::{
    extract::{FromRequestParts, OptionalFromRequestParts},
    http::{header, request::Parts, HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
};
use hyper::upgrade::{OnUpgrade, Upgraded};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::{Abort, Error, Signal};

/// A request that asks to switch protocols (`Connection: upgrade`, `Upgrade: websocket`): the
/// handler answers it with a [`Switch`], and its work then runs on the connection's [`Stream`].
/// It is a route like any other, so it takes `Caller<T>` and is in a routes module, an area or an
/// API, and a browser's switch from another site is refused (403) as a cross-site write is. A
/// request that does not ask to switch is answered 426, and a route that does not take
/// `Upgrade` never switches. A URL that is both a page (or a relayed answer) and a socket takes
/// `Option<Upgrade>`, which is `None` for a request that does not ask, and answers
/// [`SwitchOr`].
///
/// Corten reads the handshake and hands over the bytes; frames are the app's to read, for
/// example with `tokio_tungstenite::WebSocketStream::from_raw_socket(stream, Role::Server, config)`,
/// whose config sets the largest message and how much may wait to be sent.
///
/// ```
/// use corten::{prelude::*, upgrade::{Switch, Upgrade}};
/// use tokio::io::{AsyncReadExt, AsyncWriteExt};
///
/// async fn echo(upgrade: Upgrade) -> corten::Result<Switch> {
///     upgrade.websocket(|mut stream, signal| async move {
///         let mut buf = [0u8; 1024];
///         // Raw bytes here; a WebSocket library reads them as frames.
///         loop {
///             let read = tokio::select! {
///                 read = stream.read(&mut buf) => read,
///                 // The app is shutting down: a WebSocket would send its close frame here.
///                 _ = signal.aborted() => break,
///             };
///             match read {
///                 Ok(0) | Err(_) => break,
///                 Ok(n) => {
///                     if stream.write_all(&buf[..n]).await.is_err() {
///                         break;
///                     }
///                 }
///             }
///         }
///     })
/// }
/// # let _: Methods = get(echo);
/// ```
pub struct Upgrade {
    on: OnUpgrade,
    protocol: HeaderValue,
    key: Option<HeaderValue>,
    version: Option<HeaderValue>,
}

/// Whether a request asks to switch protocols: a `GET` with `Upgrade` and `upgrade` among its
/// `Connection` tokens.
pub(crate) fn asks(method: &Method, headers: &HeaderMap) -> bool {
    *method == Method::GET
        && headers.contains_key(header::UPGRADE)
        && headers
            .get_all(header::CONNECTION)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
}

impl<S: Send + Sync> FromRequestParts<S> for Upgrade {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Response> {
        if !asks(&parts.method, &parts.headers) {
            return Err(Error::new("this route switches protocols: send a GET with Connection: upgrade and an Upgrade header")
                .with_status(StatusCode::UPGRADE_REQUIRED)
                .into_response());
        }
        Upgrade::take(parts)
    }
}

/// `None` for a request that does not ask to switch; one that asks is held to what [`Upgrade`]
/// is (a switch from another site's page is a 403).
impl<S: Send + Sync> OptionalFromRequestParts<S> for Upgrade {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Option<Self>, Response> {
        if !asks(&parts.method, &parts.headers) {
            return Ok(None);
        }
        Upgrade::take(parts).map(Some)
    }
}

impl Upgrade {
    /// The switch a request asks for, once it is allowed.
    fn take(parts: &mut Parts) -> Result<Upgrade, Response> {
        // A WebSocket's handshake is a GET that carries the user's cookies with no CORS check, so
        // it is held to what a write is: the app's own pages, an origin the policy allows, or a
        // client that is not a browser. Checked here, where it costs other requests nothing:
        // looking for `Upgrade` on every GET cost 130 instructions a request, hyper's mark 50.
        let shared = parts.extensions.get::<Signal>().and_then(Signal::shared);
        let allowed = shared.map_or(&[][..], |s| &s.allowed_origins[..]);
        // Under an API, the sites its `allow_origin` names too, as for its writes.
        let api = shared.and_then(|s| s.api_origins.iter().find(|(prefix, _)| crate::api::under(prefix, parts.uri.path())));
        if !crate::security::write_allowed(allowed, &parts.headers)
            && !api.is_some_and(|(_, origins)| crate::security::allowed(&parts.headers, origins))
        {
            return Err(Error::new("a switch from another site's page is refused, as a cross-site write is")
                .with_status(StatusCode::FORBIDDEN)
                .into_response());
        }
        let Some(on) = parts.extensions.remove::<OnUpgrade>() else {
            return Err(Error::new(
                "this connection cannot switch protocols: an HTTP/1.1 connection served by corten::serve can (not HTTP/2, not a test's oneshot)",
            )
            .with_status(StatusCode::UPGRADE_REQUIRED)
            .into_response());
        };
        let protocol = parts.headers.get(header::UPGRADE).cloned().unwrap_or_else(|| HeaderValue::from_static(""));
        let key = parts.headers.get(header::SEC_WEBSOCKET_KEY).cloned();
        let version = parts.headers.get(header::SEC_WEBSOCKET_VERSION).cloned();
        Ok(Upgrade { on, protocol, key, version })
    }

    /// The protocol asked for (the `Upgrade` header): `websocket`, or another a tunnel relays.
    pub fn protocol(&self) -> &str {
        self.protocol.to_str().unwrap_or("")
    }

    /// Accepts a WebSocket handshake (RFC 6455): a 101 with its `Sec-WebSocket-Accept`, and
    /// `run` on the connection once it has switched. A handshake that is not a WebSocket's, or
    /// not version 13, is a 400.
    pub fn websocket<F, Fut>(self, run: F) -> crate::Result<Switch>
    where
        F: FnOnce(Stream, Signal) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        if !self.protocol().eq_ignore_ascii_case("websocket") {
            return Err(Error::new(format!("the request asks to switch to {:?}, not websocket", self.protocol()))
                .with_status(StatusCode::BAD_REQUEST));
        }
        if self.version.as_ref().is_none_or(|v| v.as_bytes() != b"13") {
            return Err(Error::new("a WebSocket handshake must be version 13").with_status(StatusCode::BAD_REQUEST));
        }
        // 16 random bytes, base64: 24 characters, the last two padding.
        let key = match &self.key {
            Some(key) if key.len() == 24 && key.as_bytes().ends_with(b"==") => key.as_bytes(),
            _ => return Err(Error::new("a WebSocket handshake needs a Sec-WebSocket-Key").with_status(StatusCode::BAD_REQUEST)),
        };
        let mut headers = HeaderMap::with_capacity(3);
        headers.insert(header::SEC_WEBSOCKET_ACCEPT, HeaderValue::try_from(accept(key)).expect("base64 is a header value"));
        Ok(self.switch(headers, run))
    }

    /// Switches with `headers` (an upstream's 101, relayed) and runs `run` on the connection
    /// once it has: for a tunnel that passes bytes both ways, or a protocol of the app's own.
    /// `Connection` and `Upgrade` are set from the request when `headers` has none.
    pub fn switch<F, Fut>(self, mut headers: HeaderMap, run: F) -> Switch
    where
        F: FnOnce(Stream, Signal) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        headers.entry(header::CONNECTION).or_insert(HeaderValue::from_static("upgrade"));
        headers.entry(header::UPGRADE).or_insert(self.protocol);
        let on = self.on;
        tokio::spawn(async move {
            // Not 101 after all (a middleware answered instead): nothing switched.
            let Ok(upgraded) = on.await else { return };
            let signal = Signal::default();
            let mut work = pin!(run(Stream(TokioIo::new(upgraded)), signal.clone()));
            let draining = pin!(crate::draining());
            if let futures_util::future::Either::Right(((), work)) = futures_util::future::select(work.as_mut(), draining).await {
                signal.abort(Abort::Shutdown);
                work.await;
            }
        });
        let mut res = StatusCode::SWITCHING_PROTOCOLS.into_response();
        *res.headers_mut() = headers;
        Switch(res)
    }
}

/// `Sec-WebSocket-Accept` for a key: SHA-1 of the key and the protocol's GUID, base64.
fn accept(key: &[u8]) -> String {
    let mut text = key.to_vec();
    text.extend_from_slice(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    crate::security::base64(&sha1(&text))
}

/// SHA-1 (FIPS 180-4), which the WebSocket handshake names; not used for anything secret.
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for (word, bytes) in w.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, w) in w.iter().enumerate() {
            let (f, k) = match i {
                0..20 => ((b & c) | (!b & d), 0x5A827999),
                20..40 => (b ^ c ^ d, 0x6ED9EBA1),
                40..60 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*w);
            (e, d, c, b, a) = (d, c, b.rotate_left(30), a, t);
        }
        for (state, add) in h.iter_mut().zip([a, b, c, d, e]) {
            *state = state.wrapping_add(add);
        }
    }
    let mut out = [0u8; 20];
    for (bytes, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *bytes = word.to_be_bytes();
    }
    out
}

/// The answer that switches the connection: a 101, from [`Upgrade::websocket`] or
/// [`Upgrade::switch`]. Its work starts once the 101 is sent.
pub struct Switch(Response);

impl Switch {
    /// The answer with one more header, such as the `Sec-WebSocket-Protocol` the route chose.
    pub fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.0.headers_mut().insert(name, value);
        self
    }
}

impl IntoResponse for Switch {
    fn into_response(self) -> Response {
        self.0
    }
}

impl crate::Answer for Switch {
    const FORMAT: crate::answer::Format = crate::answer::Format::Html;
    type Inputs = crate::answer::AnyInputs;
}

/// What a route that takes `Option<Upgrade>` answers: the [`Switch`] a request asked for, or the
/// route's answer `A` to one that did not. The route's failures are answered as `A`'s are.
///
/// ```
/// use corten::{prelude::*, upgrade::{SwitchOr, Upgrade}, HtmlResponse};
///
/// async fn chat(req: Req, upgrade: Option<Upgrade>) -> corten::Result<SwitchOr<HtmlResponse>> {
///     let Some(upgrade) = upgrade else {
///         // A plain GET: the page, whose script opens the socket at the same URL.
///         return Ok(SwitchOr::Answer(req.page(html! {
///             <!DOCTYPE html>
///             <html><head><title>"Chat"</title><Corten /></head><body><main></main></body></html>
///         })?));
///     };
///     Ok(upgrade.websocket(|_stream, _signal| async {})?.into())
/// }
/// # let _: Methods = get(chat);
/// ```
pub enum SwitchOr<A> {
    /// The connection switches.
    Switch(Switch),
    /// The request did not ask to switch.
    Answer(A),
}

impl<A> From<Switch> for SwitchOr<A> {
    fn from(switch: Switch) -> Self {
        SwitchOr::Switch(switch)
    }
}

impl<A: IntoResponse> IntoResponse for SwitchOr<A> {
    fn into_response(self) -> Response {
        match self {
            SwitchOr::Switch(switch) => switch.0,
            SwitchOr::Answer(answer) => answer.into_response(),
        }
    }
}

impl<A: crate::Answer> crate::Answer for SwitchOr<A> {
    const FORMAT: crate::answer::Format = A::FORMAT;
    type Inputs = A::Inputs;
}

/// The connection once switched: bytes both ways (`tokio::io::AsyncRead` and `AsyncWrite`).
/// The work on it ends when the client closes it (a read gives 0) or the work returns, and its
/// [`Signal`] aborts with [`Abort::Shutdown`] when the app begins to shut down; the work is
/// not waited for then.
pub struct Stream(TokioIo<Upgraded>);

impl AsyncRead for Stream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for Stream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }

    fn poll_write_vectored(mut self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[io::IoSlice<'_>]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.0.is_write_vectored()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_accept_key_is_the_rfcs() {
        // RFC 6455, section 1.3.
        assert_eq!(accept(b"dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
        assert_eq!(
            crate::security::base64(&sha1(b"abc")),
            crate::security::base64(&[
                0xa9, 0x99, 0x3e, 0x36, 0x47, 0x06, 0x81, 0x6a, 0xba, 0x3e, 0x25, 0x71, 0x78, 0x50, 0xc2, 0x6c, 0x9c, 0xd0, 0xd8,
                0x9d
            ])
        );
        // FIPS 180 examples: empty, and two blocks.
        let hex = |bytes: [u8; 20]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(hex(sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }
}
