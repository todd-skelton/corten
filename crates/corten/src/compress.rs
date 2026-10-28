//! Response compression (feature `compress`), tuned for what Corten serves.
//!
//! Pages, patches and JSON are generated per request, so they are compressed per request at the
//! fastest level that still pays: zstd 1 for clients that accept it (current Chrome, Edge,
//! Firefox and Safari), otherwise brotli 2, otherwise gzip 1. On the item page that costs about
//! 50 µs with zstd, against 400-500 µs for brotli and gzip at common default levels, for a body
//! within 10% of the smallest. The runtime and [`crate::assets`] files never change, so
//! they are compressed once, at brotli 11 and gzip 9, the first time a client asks for them (in
//! a background thread, so startup is not delayed; until then they go out as dynamic responses).
//!
//! Event streams ([`crate::live`], feeds) are compressed as one stream per connection, flushed
//! after every event so it arrives at once, with the context kept between events: a redraw that
//! repeats the last one costs tens of bytes. A stream holds its context while it is open, so
//! streams use small windows (zstd 1 with a 64 KiB window, about 470 KiB; brotli 4, about 800 KiB;
//! gzip clients get the stream as it is) and share a budget of 256 MiB (about 550 streams): past
//! it a new stream goes out uncompressed. An event repeats cheaply when the one before it is within the
//! window, so events up to about 64 KB.
//!
//! `corten::compress::apply(router)` wraps the whole app. Images, fonts, bodies under 1 KiB,
//! another server's answer ([`crate::Relay`]) and responses that are already encoded or marked
//! `no-transform` pass through.

use std::{
    cell::RefCell,
    io::Write,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, OnceLock,
    },
};

use axum::{
    body::{Body, Bytes, HttpBody},
    extract::Request,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::Response,
    Router,
};

/// Bodies smaller than this are sent as they are: compression would not save a packet.
const MIN_SIZE: u64 = 1024;
const ZSTD_LEVEL: i32 = 1;
const BROTLI_LEVEL: u32 = 2;
const GZIP_LEVEL: u32 = 1;

/// Memory all compressed event streams may hold at once, in bytes. Each open stream holds its
/// coder's context ([`StreamCoder`]); a stream that would pass this is sent uncompressed.
const STREAM_BUDGET: usize = 256 << 20;
/// What one stream's context holds, measured with the settings in [`StreamCoder::new`].
const ZSTD_STREAM_BYTES: usize = 480 << 10;
const BROTLI_STREAM_BYTES: usize = 1000 << 10;
static STREAM_BYTES: AtomicUsize = AtomicUsize::new(0);

/// A content coding Corten writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Coding {
    Zstd,
    Brotli,
    Gzip,
}

impl Coding {
    /// Appended to a file's ETag, so each coding has its own.
    pub fn tag(self) -> &'static str {
        match self {
            Coding::Zstd => "zst",
            Coding::Brotli => "br",
            Coding::Gzip => "gz",
        }
    }

    fn header(self) -> HeaderValue {
        HeaderValue::from_static(match self {
            Coding::Zstd => "zstd",
            Coding::Brotli => "br",
            Coding::Gzip => "gzip",
        })
    }
}

/// The codings `Accept-Encoding` allows, in the server's order of preference among those with
/// the highest q-value.
pub(crate) fn accepted(headers: &HeaderMap, offered: &[Coding]) -> Option<Coding> {
    let mut best: Option<(u16, usize, Coding)> = None;
    for value in headers.get_all(header::ACCEPT_ENCODING) {
        let Ok(value) = value.to_str() else { continue };
        for item in value.split(',') {
            let mut parts = item.split(';');
            let name = parts.next().unwrap_or("").trim();
            let q = parts
                .filter_map(|p| p.trim().strip_prefix("q=").or_else(|| p.trim().strip_prefix("Q=")))
                .find_map(|q| q.trim().parse::<f32>().ok())
                .map_or(1000, |q| (q.clamp(0.0, 1.0) * 1000.0) as u16);
            if q == 0 {
                continue;
            }
            let is = |coding: &str| name.eq_ignore_ascii_case(coding);
            let coding = if is("zstd") {
                Coding::Zstd
            } else if is("br") {
                Coding::Brotli
            } else if is("gzip") || is("x-gzip") {
                Coding::Gzip
            } else {
                continue;
            };
            let Some(rank) = offered.iter().position(|c| *c == coding) else { continue };
            if best.is_none_or(|(bq, brank, _)| q > bq || (q == bq && rank < brank)) {
                best = Some((q, rank, coding));
            }
        }
    }
    best.map(|(_, _, coding)| coding)
}

fn compressible(content_type: &str) -> bool {
    let essence = content_type.split(';').next().unwrap_or("").trim();
    (essence.starts_with("text/") && essence != "text/event-stream")
        || matches!(
            essence,
            "application/json"
                | "application/javascript"
                | "application/manifest+json"
                | "application/xml"
                | "application/wasm"
                | "image/svg+xml"
        )
}

thread_local! {
    static ZSTD: RefCell<Option<zstd::bulk::Compressor<'static>>> = const { RefCell::new(None) };
}

/// One-shot compression: at the per-request level, or with `best` at the highest one (for files
/// compressed once).
pub(crate) fn encode(coding: Coding, data: &[u8], best: bool) -> Vec<u8> {
    match coding {
        Coding::Zstd => ZSTD.with_borrow_mut(|slot| {
            let compressor = slot.get_or_insert_with(|| zstd::bulk::Compressor::new(ZSTD_LEVEL).expect("zstd context"));
            compressor.compress(data).expect("zstd compresses in memory")
        }),
        Coding::Brotli => {
            let quality = if best { 11 } else { BROTLI_LEVEL };
            let mut out = Vec::with_capacity(data.len() / 3);
            {
                let mut writer = brotli::CompressorWriter::new(&mut out, 4096, quality, 22);
                writer.write_all(data).expect("brotli writes to memory");
            }
            out
        }
        Coding::Gzip => {
            let level = if best { 9 } else { GZIP_LEVEL };
            let mut writer = flate2::write::GzEncoder::new(Vec::with_capacity(data.len() / 3), flate2::Compression::new(level));
            writer.write_all(data).expect("gzip writes to memory");
            writer.finish().expect("gzip writes to memory")
        }
    }
}

/// Compresses every compressible response of `router` for clients that accept it. Apply it
/// outermost, so it sees every response. [`crate::App`] compresses its responses itself; this is
/// for a router served without one.
pub fn apply<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    router.layer(middleware::from_fn(|req: Request, next: Next| async move {
        let coding = negotiate(req.headers());
        finish(next.run(req).await, coding).await
    }))
}

/// The coding a response to this request may use, read before the request moves on.
pub(crate) fn negotiate(headers: &HeaderMap) -> Option<Coding> {
    accepted(headers, &[Coding::Zstd, Coding::Brotli, Coding::Gzip])
}

/// `res` compressed with `coding` when it is worth it.
pub(crate) async fn finish(res: Response, coding: Option<Coding>) -> Response {
    // Another server's answer goes on as it came: its encoding is the upstream's (#205).
    if res.extensions().get::<crate::answer::Relayed>().is_some() {
        return res;
    }
    let headers = res.headers();
    let content_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    let event_stream = content_type.split(';').next().unwrap_or("").trim() == "text/event-stream";
    if event_stream {
        return stream(res, coding);
    }
    if !compressible(content_type)
        || headers.contains_key(header::CONTENT_ENCODING)
        || headers.contains_key(header::CONTENT_RANGE)
        || matches!(res.status(), StatusCode::NO_CONTENT | StatusCode::NOT_MODIFIED)
        || headers.get(header::CACHE_CONTROL).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("no-transform"))
    {
        return res;
    }
    let (mut parts, body) = res.into_parts();
    vary(&mut parts.headers);
    let size = body.size_hint().exact();
    let Some(coding) = coding.filter(|_| size.is_some_and(|n| n >= MIN_SIZE)) else {
        return Response::from_parts(parts, body);
    };
    // Exact size: a buffered page or JSON body, never a stream, so collecting it does not wait.
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let encoded = encode(coding, &bytes, false);
    parts.headers.insert(header::CONTENT_ENCODING, coding.header());
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.remove(header::ACCEPT_RANGES);
    if let Some(tag) = parts.headers.get(header::ETAG).and_then(|v| v.to_str().ok()).filter(|t| !t.starts_with("W/")) {
        // The encoded bytes differ from the identity ones, so the tag can only be weak.
        if let Ok(weak) = HeaderValue::try_from(format!("W/{tag}")) {
            parts.headers.insert(header::ETAG, weak);
        }
    }
    Response::from_parts(parts, Body::from(encoded))
}

/// An event stream, compressed one event at a time with the context kept between them.
fn stream(res: Response, coding: Option<Coding>) -> Response {
    let headers = res.headers();
    if headers.contains_key(header::CONTENT_ENCODING)
        || headers.get(header::CACHE_CONTROL).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("no-transform"))
    {
        return res;
    }
    let (mut parts, body) = res.into_parts();
    vary(&mut parts.headers);
    let Some(mut coder) = coding.and_then(StreamCoder::new) else {
        return Response::from_parts(parts, body);
    };
    parts.headers.insert(header::CONTENT_ENCODING, coder.coding().header());
    parts.headers.remove(header::CONTENT_LENGTH);
    let chunks = futures_util::StreamExt::filter_map(body.into_data_stream(), move |chunk| {
        let out = chunk.map(|data| coder.event(&data));
        std::future::ready(match out {
            Ok(bytes) if bytes.is_empty() => None,
            other => Some(other),
        })
    });
    Response::from_parts(parts, Body::from_stream(chunks))
}

/// One event stream's coder: everything written so far is its context, so an event like an
/// earlier one costs only what differs. Holds its share of `STREAM_BUDGET` until dropped.
pub(crate) enum StreamCoder {
    Zstd(zstd::zstd_safe::CCtx<'static>),
    Brotli(Box<brotli::CompressorWriter<Vec<u8>>>),
}

impl StreamCoder {
    /// A coder for `coding`, or `None` when the stream should go out as it is: a gzip client
    /// (deflate repeats an event poorly at a level worth its time), or the budget is spent.
    pub(crate) fn new(coding: Coding) -> Option<StreamCoder> {
        let cost = match coding {
            Coding::Zstd => ZSTD_STREAM_BYTES,
            Coding::Brotli => BROTLI_STREAM_BYTES,
            Coding::Gzip => return None,
        };
        if !reserve(&STREAM_BYTES, cost, STREAM_BUDGET) {
            return None;
        }
        Some(match coding {
            Coding::Zstd => {
                use zstd::zstd_safe::CParameter;
                let mut cx = zstd::zstd_safe::CCtx::create();
                for p in [CParameter::CompressionLevel(ZSTD_LEVEL), CParameter::WindowLog(16), CParameter::HashLog(12)] {
                    cx.set_parameter(p).expect("a valid zstd parameter");
                }
                StreamCoder::Zstd(cx)
            }
            _ => StreamCoder::Brotli(Box::new(brotli::CompressorWriter::new(Vec::new(), 4096, 4, 16))),
        })
    }

    fn coding(&self) -> Coding {
        match self {
            StreamCoder::Zstd(_) => Coding::Zstd,
            StreamCoder::Brotli(_) => Coding::Brotli,
        }
    }

    fn cost(&self) -> usize {
        match self {
            StreamCoder::Zstd(_) => ZSTD_STREAM_BYTES,
            StreamCoder::Brotli(_) => BROTLI_STREAM_BYTES,
        }
    }

    /// `data` compressed and flushed, so the client can decode everything sent so far.
    pub(crate) fn event(&mut self, data: &[u8]) -> Bytes {
        match self {
            StreamCoder::Zstd(cx) => {
                use zstd::zstd_safe::{zstd_sys::ZSTD_EndDirective, InBuffer, OutBuffer};
                let mut out = Vec::with_capacity(zstd::zstd_safe::compress_bound(data.len()) + 32);
                let mut input = InBuffer::around(data);
                loop {
                    let at = out.len();
                    let mut buf = OutBuffer::around_pos(&mut out, at);
                    let left = cx
                        .compress_stream2(&mut buf, &mut input, ZSTD_EndDirective::ZSTD_e_flush)
                        .expect("zstd compresses in memory");
                    if left == 0 {
                        break;
                    }
                    out.reserve(left);
                }
                out.into()
            }
            StreamCoder::Brotli(writer) => {
                writer.write_all(data).expect("brotli writes to memory");
                writer.flush().expect("brotli writes to memory");
                std::mem::take(writer.get_mut()).into()
            }
        }
    }
}

/// Adds `cost` to `held` unless that would pass `budget`.
fn reserve(held: &AtomicUsize, cost: usize, budget: usize) -> bool {
    let mut now = held.load(Ordering::Acquire);
    loop {
        if now + cost > budget {
            return false;
        }
        match held.compare_exchange_weak(now, now + cost, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(actual) => now = actual,
        }
    }
}

impl Drop for StreamCoder {
    fn drop(&mut self) {
        STREAM_BYTES.fetch_sub(self.cost(), Ordering::AcqRel);
    }
}

/// Adds `accept-encoding` to `Vary`, so a cache keeps one copy per coding.
pub(crate) fn vary(headers: &mut HeaderMap) {
    let already = headers
        .get_all(header::VARY)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .any(|v| v.split(',').any(|f| f.trim().eq_ignore_ascii_case("accept-encoding") || f.trim() == "*"));
    if !already {
        headers.append(header::VARY, HeaderValue::from_static("accept-encoding"));
    }
}

/// A file that never changes, with its brotli 11 and gzip 9 forms made on first request.
pub(crate) struct Precompressed {
    identity: Bytes,
    content_type: &'static str,
    encoded: Arc<OnceLock<[Bytes; 2]>>,
    started: AtomicBool,
}

impl Precompressed {
    pub fn new(identity: Bytes, content_type: &'static str) -> Self {
        Precompressed { identity, content_type, encoded: Arc::default(), started: AtomicBool::new(false) }
    }

    /// The body for this request and its coding (`None`: identity). The first request that
    /// could use a compressed form starts making them; it and any request before they are done
    /// get the identity bytes (which `apply` compresses at the fast level).
    pub fn pick(&self, headers: &HeaderMap) -> (Bytes, Option<Coding>) {
        if !compressible(self.content_type) || (self.identity.len() as u64) < MIN_SIZE {
            return (self.identity.clone(), None);
        }
        let Some(coding) = accepted(headers, &[Coding::Brotli, Coding::Gzip]) else {
            return (self.identity.clone(), None);
        };
        match self.encoded.get() {
            Some([br, gzip]) => match coding {
                Coding::Brotli => (br.clone(), Some(coding)),
                _ => (gzip.clone(), Some(Coding::Gzip)),
            },
            None => {
                if !self.started.swap(true, Ordering::AcqRel) {
                    let (identity, slot) = (self.identity.clone(), self.encoded.clone());
                    std::thread::spawn(move || {
                        let br = Bytes::from(encode(Coding::Brotli, &identity, true));
                        let gzip = Bytes::from(encode(Coding::Gzip, &identity, true));
                        let _ = slot.set([br, gzip]);
                    });
                }
                (self.identity.clone(), None)
            }
        }
    }

    /// Waits until the compressed forms exist (tests).
    #[cfg(test)]
    pub fn wait(&self) {
        while self.encoded.get().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

/// Sets the headers for a body [`Precompressed::pick`] chose.
pub(crate) fn encoded_headers(headers: &mut HeaderMap, content_type: &str, coding: Option<Coding>) {
    if compressible(content_type) {
        vary(headers);
    }
    if let Some(coding) = coding {
        headers.insert(header::CONTENT_ENCODING, coding.header());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn accept(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn negotiation_follows_q_values_then_server_preference() {
        let all = [Coding::Zstd, Coding::Brotli, Coding::Gzip];
        assert_eq!(accepted(&accept("gzip, deflate, br, zstd"), &all), Some(Coding::Zstd));
        assert_eq!(accepted(&accept("gzip, deflate, br"), &all), Some(Coding::Brotli));
        assert_eq!(accepted(&accept("gzip;q=1, br;q=0.5"), &all), Some(Coding::Gzip));
        assert_eq!(accepted(&accept("zstd;q=0, gzip"), &all), Some(Coding::Gzip));
        assert_eq!(accepted(&accept("identity"), &all), None);
        assert_eq!(accepted(&accept("GZIP, Br;Q=0.5"), &all), Some(Coding::Gzip), "names are case-insensitive");
        assert_eq!(accepted(&HeaderMap::new(), &all), None);
        assert_eq!(accepted(&accept("zstd, gzip"), &[Coding::Brotli, Coding::Gzip]), Some(Coding::Gzip));
    }

    fn app() -> Router {
        let page = "<p>corten</p>".repeat(200);
        apply(
            Router::new()
                .route("/page", get(move || async move { axum::response::Html(page) }))
                .route("/small", get(|| async { axum::response::Html("<p>small</p>") }))
                .route("/png", get(|| async { ([(header::CONTENT_TYPE, "image/png")], vec![0u8; 4096]) }))
                .route("/relay", get(|| async { relayed("text/html; charset=utf-8", Body::from("<p>corten</p>".repeat(200))) }))
                .route(
                    "/relay-live",
                    get(|| async {
                        let events = futures_util::stream::iter(
                            [redraw(1), redraw(2)].map(|e| Ok::<_, std::convert::Infallible>(Bytes::from(e))),
                        );
                        relayed("text/event-stream", Body::from_stream(events))
                    }),
                ),
        )
    }

    fn relayed(content_type: &'static str, body: Body) -> Response {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        axum::response::IntoResponse::into_response(crate::Relay::new(StatusCode::OK, headers, body))
    }

    async fn get_with(uri: &str, encoding: &str) -> (HeaderMap, Vec<u8>) {
        let req = Request::get(uri).header("accept-encoding", encoding).body(Body::empty()).unwrap();
        let res = app().oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        (parts.headers, body.collect().await.unwrap().to_bytes().to_vec())
    }

    #[tokio::test]
    async fn pages_are_compressed_with_the_best_accepted_coding_and_decode_back() {
        let original = "<p>corten</p>".repeat(200);
        let (headers, body) = get_with("/page", "gzip, deflate, br, zstd").await;
        assert_eq!(headers["content-encoding"], "zstd");
        assert_eq!(headers["vary"], "accept-encoding");
        assert_eq!(zstd::bulk::decompress(&body, 1 << 20).unwrap(), original.as_bytes());
        let (headers, body) = get_with("/page", "gzip, br").await;
        assert_eq!(headers["content-encoding"], "br");
        let mut out = Vec::new();
        brotli::BrotliDecompress(&mut &body[..], &mut out).unwrap();
        assert_eq!(out, original.as_bytes());
        let (headers, body) = get_with("/page", "gzip").await;
        assert_eq!(headers["content-encoding"], "gzip");
        let mut out = String::new();
        std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(&body[..]), &mut out).unwrap();
        assert_eq!(out, original);
        let (headers, body) = get_with("/page", "identity").await;
        assert!(!headers.contains_key("content-encoding"));
        assert_eq!(headers["vary"], "accept-encoding", "caches must still key on the coding");
        assert_eq!(body, original.as_bytes());
    }

    #[tokio::test]
    async fn small_bodies_and_images_pass_through() {
        let (headers, _) = get_with("/small", "zstd").await;
        assert!(!headers.contains_key("content-encoding"));
        let (headers, body) = get_with("/png", "zstd").await;
        assert!(!headers.contains_key("content-encoding") && !headers.contains_key("vary"));
        assert_eq!(body.len(), 4096);
    }

    #[tokio::test]
    async fn a_relayed_answer_goes_on_as_it_came() {
        let (headers, body) = get_with("/relay", "gzip, br, zstd").await;
        assert!(!headers.contains_key("content-encoding") && !headers.contains_key("vary"), "{headers:?}");
        assert_eq!(body, "<p>corten</p>".repeat(200).into_bytes());
        // An event stream is not encoded either, so it holds no coder from the stream budget.
        let (headers, body) = get_with("/relay-live", "zstd").await;
        assert!(!headers.contains_key("content-encoding") && !headers.contains_key("vary"), "{headers:?}");
        assert_eq!(body, [redraw(1), redraw(2)].concat().into_bytes());
    }

    /// A row list redrawn: the same markup with one price changed.
    fn redraw(changed: usize) -> String {
        let rows: String = (0..40)
            .map(|i| {
                format!(
                    r#"<li id="row-{i}"><a href="/items/{i}">Item {i}</a><span>{}</span></li>"#,
                    if i == changed { 999 } else { i }
                )
            })
            .collect();
        format!("event: patch\ndata: <ul id=\"rows\">{rows}</ul>\n\n")
    }

    async fn events(encoding: &str) -> (HeaderMap, Vec<Bytes>) {
        let chunks = futures_util::stream::iter(
            [redraw(1), redraw(2), redraw(2)].map(|e| Ok::<_, std::convert::Infallible>(Bytes::from(e))),
        );
        let app = apply(Router::new().route(
            "/live",
            get(move || async move { ([(header::CONTENT_TYPE, "text/event-stream")], Body::from_stream(chunks)) }),
        ));
        let req = Request::get("/live").header("accept-encoding", encoding).body(Body::empty()).unwrap();
        let (parts, body) = app.oneshot(req).await.unwrap().into_parts();
        let mut frames = Vec::new();
        let mut body = body.into_data_stream();
        while let Some(frame) = futures_util::StreamExt::next(&mut body).await {
            frames.push(frame.unwrap());
        }
        (parts.headers, frames)
    }

    #[tokio::test]
    async fn an_event_stream_is_compressed_per_event_with_the_context_kept() {
        let sent = [redraw(1), redraw(2), redraw(2)];
        let (headers, frames) = events("gzip, deflate, br, zstd").await;
        assert_eq!(headers["content-encoding"], "zstd");
        assert_eq!(headers["vary"], "accept-encoding");
        assert_eq!(frames.len(), 3, "one compressed chunk per event");
        // Each chunk decodes as soon as it arrives: the stream is flushed after every event.
        let mut decoder = zstd::stream::write::Decoder::new(Vec::new()).unwrap();
        for (frame, event) in frames.iter().zip(&sent) {
            decoder.write_all(frame).unwrap();
            decoder.flush().unwrap();
            assert_eq!(String::from_utf8(std::mem::take(decoder.get_mut())).unwrap(), *event);
        }
        assert!(frames[0].len() < sent[0].len() / 4, "first event {} of {}", frames[0].len(), sent[0].len());
        assert!(frames[1].len() < sent[1].len() / 10, "a redraw costs about what changed: {}", frames[1].len());
        assert!(frames[2].len() < 40, "a repeated redraw costs next to nothing: {}", frames[2].len());

        let (headers, frames) = events("gzip, br").await;
        assert_eq!(headers["content-encoding"], "br");
        let mut decoder = brotli::DecompressorWriter::new(Vec::new(), 4096);
        for (frame, event) in frames.iter().zip(&sent) {
            decoder.write_all(frame).unwrap();
            decoder.flush().unwrap();
            assert_eq!(String::from_utf8(std::mem::take(decoder.get_mut())).unwrap(), *event);
        }
        assert!(frames[2].len() < 40, "{}", frames[2].len());

        // Deflate repeats a 4 KB event poorly at a level worth its time: gzip clients get it as it is.
        let (headers, frames) = events("gzip").await;
        assert!(!headers.contains_key("content-encoding"));
        assert_eq!(headers["vary"], "accept-encoding");
        assert_eq!(frames.concat(), sent.concat().into_bytes());
    }

    #[test]
    fn stream_contexts_share_a_budget_and_return_it_when_dropped() {
        let held = AtomicUsize::new(0);
        assert!(reserve(&held, 600, 1000));
        assert!(!reserve(&held, 600, 1000), "past the budget the stream goes out uncompressed");
        assert!(reserve(&held, 400, 1000));
        assert_eq!(held.load(Ordering::Acquire), 1000);
        assert!(StreamCoder::new(Coding::Gzip).is_none());
        // What a zstd stream reserves covers its context once it has seen large events.
        let mut coder = StreamCoder::new(Coding::Zstd).unwrap();
        for _ in 0..3 {
            coder.event(redraw(3).repeat(40).as_bytes());
        }
        let StreamCoder::Zstd(cx) = &coder else { unreachable!() };
        assert!(cx.sizeof() <= ZSTD_STREAM_BYTES, "{} bytes", cx.sizeof());
    }

    #[test]
    fn precompressed_files_start_as_identity_then_serve_brotli_11() {
        let file = Precompressed::new(Bytes::from("export const x = 1;\n".repeat(200)), "text/javascript; charset=utf-8");
        let (body, coding) = file.pick(&accept("gzip, br, zstd"));
        assert_eq!((body.len(), coding), (4000, None), "the first request does not wait");
        file.wait();
        let (body, coding) = file.pick(&accept("gzip, br, zstd"));
        assert_eq!(coding, Some(Coding::Brotli));
        assert!(body.len() < 100);
        assert_eq!(file.pick(&accept("gzip")).1, Some(Coding::Gzip));
        assert_eq!(file.pick(&accept("identity")).1, None);
    }
}
