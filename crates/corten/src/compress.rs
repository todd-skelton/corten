//! Response compression (feature `compress`), tuned for what Corten serves.
//!
//! Pages, patches and JSON are generated per request, so they are compressed per request at the
//! fastest level that still pays: zstd 1 for clients that accept it (current Chrome, Edge,
//! Firefox and Safari), otherwise brotli 2, otherwise gzip 1. On the item page that costs about
//! 50 µs with zstd, against 400-500 µs for brotli and gzip at common default levels, for a body
//! within 10% of the smallest. The runtime and [`crate::assets`] files never change, so
//! they are compressed once, at brotli 11 and gzip 9, the first time a client asks for them (in
//! a background thread, so startup is not delayed; until then they go out as dynamic responses).
//! JSON the app holds as bytes and sends again ([`crate::json::Raw`]) is compressed once too, the
//! second time it is sent, at brotli 9 and gzip 9, by one background thread that takes them in
//! turn from a bounded queue: on an 80 KB document that takes 15 ms once,
//! where compressing every read costs 220 µs (zstd 1) to 2.1 ms (brotli 2), and gives 5,000 bytes
//! against 6,992 with zstd 1. brotli 11 would take 356 ms, too long for a value that changes while
//! the app runs.
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
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
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
    each_accepted(headers, |q, coding| {
        if let Some(rank) = offered.iter().position(|c| *c == coding) {
            if best.is_none_or(|(bq, brank, _)| q > bq || (q == bq && rank < brank)) {
                best = Some((q, rank, coding));
            }
        }
    });
    best.map(|(_, _, coding)| coding)
}

/// Calls `f` with each coding `Accept-Encoding` allows and its q-value (in thousandths).
/// Read as bytes, and a q-value parsed only where an item has one: every request reads this header.
fn each_accepted(headers: &HeaderMap, mut f: impl FnMut(u16, Coding)) {
    for value in headers.get_all(header::ACCEPT_ENCODING) {
        for item in value.as_bytes().split(|&b| b == b',') {
            let (name, params) = match item.iter().position(|&b| b == b';') {
                Some(at) => (&item[..at], Some(&item[at + 1..])),
                None => (item, None),
            };
            let is = |coding: &[u8]| name.trim_ascii().eq_ignore_ascii_case(coding);
            let coding = if is(b"zstd") {
                Coding::Zstd
            } else if is(b"br") {
                Coding::Brotli
            } else if is(b"gzip") || is(b"x-gzip") {
                Coding::Gzip
            } else {
                continue;
            };
            let q = params.map_or(1000, quality);
            if q > 0 {
                f(q, coding);
            }
        }
    }
}

/// The q-value among an item's parameters (`q=0.5`), in thousandths; 1000 when it has none.
fn quality(params: &[u8]) -> u16 {
    params
        .split(|&b| b == b';')
        .filter_map(|p| p.trim_ascii().strip_prefix(b"q=").or_else(|| p.trim_ascii().strip_prefix(b"Q=")))
        .find_map(|q| std::str::from_utf8(q.trim_ascii()).ok()?.parse::<f32>().ok())
        .map_or(1000, |q| (q.clamp(0.0, 1.0) * 1000.0) as u16)
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

/// How hard a one-shot compression tries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    /// Per request: the fastest level that still pays.
    Fast,
    /// Once per value the app holds and sends again ([`crate::json::Raw`]): brotli 9 and gzip 9.
    /// Measured on an 80 KB JSON document (release build): brotli 9 takes 15 ms once and gives
    /// 5,000 bytes, where brotli 11 takes 356 ms (4,351 bytes), too long for a value that changes
    /// while the app runs; zstd 1 per read takes 220 µs for 6,992 bytes and brotli 2 2.1 ms.
    Held,
    /// Once per file that never changes: the highest levels.
    Best,
}

/// One-shot compression at `level`. zstd is only ever used per request.
pub(crate) fn encode(coding: Coding, data: &[u8], level: Level) -> Vec<u8> {
    match coding {
        Coding::Zstd => ZSTD.with_borrow_mut(|slot| {
            let compressor = slot.get_or_insert_with(|| zstd::bulk::Compressor::new(ZSTD_LEVEL).expect("zstd context"));
            compressor.compress(data).expect("zstd compresses in memory")
        }),
        Coding::Brotli => {
            let quality = match level {
                Level::Fast => BROTLI_LEVEL,
                Level::Held => 9,
                Level::Best => 11,
            };
            let mut out = Vec::with_capacity(data.len() / 3);
            {
                let mut writer = brotli::CompressorWriter::new(&mut out, 4096, quality, 22);
                writer.write_all(data).expect("brotli writes to memory");
            }
            out
        }
        Coding::Gzip => {
            let level = if level == Level::Fast { GZIP_LEVEL } else { 9 };
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

/// What a request accepts, read before it moves on: each coding's q-value (in thousandths, `0`:
/// not accepted), in the server's order of preference, zstd, brotli, gzip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Accepts([u16; 3]);

impl Accepts {
    /// The coding for a body compressed as it is sent: the highest q-value, then the server's order.
    fn now(self) -> Option<Coding> {
        self.best(&[Coding::Zstd, Coding::Brotli, Coding::Gzip])
    }

    /// The coding for a body compressed before ([`Precompressed`]: brotli or gzip), when the
    /// client ranks it as high as any it accepts: one that prefers zstd gets zstd made now.
    fn stored(self) -> Option<Coding> {
        let top = self.0.iter().copied().max().unwrap_or(0);
        self.best(&[Coding::Brotli, Coding::Gzip]).filter(|c| self.0[*c as usize] == top)
    }

    fn best(self, offered: &[Coding]) -> Option<Coding> {
        let mut best: Option<(u16, Coding)> = None;
        for &coding in offered {
            let q = self.0[coding as usize];
            if q > best.map_or(0, |(bq, _)| bq) {
                best = Some((q, coding));
            }
        }
        best.map(|(_, coding)| coding)
    }
}

/// The codings a response to this request may use, in one pass over `Accept-Encoding`.
pub(crate) fn negotiate(headers: &HeaderMap) -> Accepts {
    let mut q = [0u16; 3];
    each_accepted(headers, |value, coding| {
        let slot = &mut q[coding as usize];
        *slot = (*slot).max(value);
    });
    Accepts(q)
}

/// `res` compressed for what the request accepts when it is worth it: a body compressed before
/// ([`Stored`]) is sent in the form made for it, anything else is compressed now.
pub(crate) async fn finish(res: Response, accepts: Accepts) -> Response {
    let coding = accepts.now();
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
    // Exact size: a buffered page or JSON body, never a stream, so collecting it does not wait
    // (one held `Bytes` comes back as it is, with no copy).
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return Response::from_parts(parts, Body::empty());
    };
    // A held value's stored form, only while the body is still its bytes: a handler or a
    // middleware that replaced the body keeps the extension, not the bytes it stood for.
    let stored = parts.extensions.get::<Stored>().and_then(|Stored(held)| {
        let identity = held.identity();
        let same = identity.as_ptr() == bytes.as_ptr() && identity.len() == bytes.len();
        Precompressed::stored_held(held, accepts.stored().filter(|_| same)?)
    });
    let (encoded, coding) = match stored {
        Some(made) => made,
        None => (Bytes::from(encode(coding, &bytes, Level::Fast)), coding),
    };
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

/// A body compressed once with brotli and gzip and sent many times: a file that never changes, at
/// [`Level::Best`], made on its first request; or JSON the app holds ([`crate::json::Raw`]), at
/// [`Level::Held`], made on its second read, so a value made for one request is never compressed
/// twice. Until they are made, the body is compressed per request as any other.
pub(crate) struct Precompressed {
    identity: Bytes,
    content_type: &'static str,
    encoded: Arc<OnceLock<[Bytes; 2]>>,
    started: AtomicBool,
    level: Level,
    reads: AtomicU32,
}

/// On a response whose body is a [`Precompressed`]'s identity bytes: [`finish`] sends the form
/// made for it instead of compressing the body again.
#[derive(Clone)]
pub(crate) struct Stored(pub Arc<Precompressed>);

impl Precompressed {
    pub fn new(identity: Bytes, content_type: &'static str) -> Self {
        Self::at(identity, content_type, Level::Best)
    }

    /// JSON the app holds and sends again.
    #[cfg(feature = "json")]
    pub fn held(identity: Bytes) -> Self {
        Self::at(identity, crate::api::JSON, Level::Held)
    }

    fn at(identity: Bytes, content_type: &'static str, level: Level) -> Self {
        let (encoded, started, reads) = (Arc::default(), AtomicBool::new(false), AtomicU32::new(0));
        Precompressed { identity, content_type, encoded, started, level, reads }
    }

    /// The bytes as they are.
    pub fn identity(&self) -> &Bytes {
        &self.identity
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
        match self.stored(coding) {
            Some((bytes, coding)) => (bytes, Some(coding)),
            None => (self.identity.clone(), None),
        }
    }

    /// The form made for `coding` (brotli, or gzip for anything else), or `None` until it is
    /// made; asking starts making them (a file's first time).
    pub fn stored(&self, coding: Coding) -> Option<(Bytes, Coding)> {
        self.stored_or_start(coding, || {
            let (identity, slot, level) = (self.identity.clone(), self.encoded.clone(), self.level);
            Box::new(move || make(&identity, &slot, level))
        })
    }

    /// [`Precompressed::stored`] for a held value (a second read starts making them). The job
    /// waiting its turn holds the value weakly: one the app has replaced and dropped by then is
    /// skipped, and its bytes are freed when the app drops it, not when its turn comes.
    pub fn stored_held(this: &Arc<Self>, coding: Coding) -> Option<(Bytes, Coding)> {
        this.stored_or_start(coding, || {
            let held = Arc::downgrade(this);
            Box::new(move || {
                if let Some(held) = held.upgrade() {
                    make(&held.identity, &held.encoded, held.level)
                }
            })
        })
    }

    fn stored_or_start(&self, coding: Coding, job: impl FnOnce() -> Job) -> Option<(Bytes, Coding)> {
        if let Some([br, gzip]) = self.encoded.get() {
            return Some(match coding {
                Coding::Brotli => (br.clone(), coding),
                _ => (gzip.clone(), Coding::Gzip),
            });
        }
        let wait = if self.level == Level::Held { 1 } else { 0 };
        if self.reads.fetch_add(1, Ordering::Relaxed) >= wait && !self.started.swap(true, Ordering::AcqRel) {
            let queued = match self.level {
                // Files: as many as the app has, each once.
                Level::Best => std::thread::Builder::new().spawn(job()).is_ok(),
                // Held values: as many as the app's data, so one worker, a bounded queue.
                _ => held_worker().is_some_and(|tx| tx.try_send(job()).is_ok()),
            };
            if !queued {
                // Full, or no thread to be had: a later read asks again.
                self.started.store(false, Ordering::Release);
            }
        }
        None
    }

    /// Whether making the stored forms has started (tests).
    #[cfg(test)]
    pub fn started(&self) -> bool {
        self.started.load(Ordering::Acquire)
    }

    /// Waits until the compressed forms exist (tests).
    #[cfg(test)]
    pub fn wait(&self) {
        while self.encoded.get().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

type Job = Box<dyn FnOnce() + Send>;

/// Makes the stored forms of `identity` at `level` into `slot`.
fn make(identity: &Bytes, slot: &OnceLock<[Bytes; 2]>, level: Level) {
    let br = Bytes::from(encode(Coding::Brotli, identity, level));
    let gzip = Bytes::from(encode(Coding::Gzip, identity, level));
    let _ = slot.set([br, gzip]);
}

/// Held values waiting to be compressed; past this, a read leaves its value to a later one.
const HELD_QUEUE: usize = 64;

/// The one thread that compresses held values ([`Level::Held`]), in the order they were asked
/// for: a burst of documents read twice at once is compressed one at a time, not on a thread
/// each. `None` when the thread could not be started.
fn held_worker() -> Option<&'static std::sync::mpsc::SyncSender<Job>> {
    static WORKER: OnceLock<Option<std::sync::mpsc::SyncSender<Job>>> = OnceLock::new();
    WORKER
        .get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::sync_channel::<Job>(HELD_QUEUE);
            let worker = std::thread::Builder::new().name("corten-compress".into()).spawn(move || {
                for job in rx {
                    // A job that panics loses its own forms (that value is compressed per
                    // request), not the worker every later value needs.
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                }
            });
            worker.ok().map(|_| tx)
        })
        .as_ref()
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

    /// A held JSON document (`Raw`) read again is sent in the form made for it once, and only in a
    /// coding the request accepts; one read once is never compressed off the request.
    #[cfg(feature = "json")]
    #[tokio::test]
    async fn a_held_document_is_compressed_once_and_sent_as_made() {
        let text: String =
            format!("[{}]", (0..300).map(|i| format!(r#"{{"id":{i},"name":"row {i}"}}"#)).collect::<Vec<_>>().join(","));
        let raw = crate::json::Raw::new(text.clone()).unwrap();
        let held = raw.clone();
        let app = apply(Router::new().route("/doc", get(move || std::future::ready(crate::Json(held.clone())))));
        let read = |encoding: &'static str| {
            let app = app.clone();
            async move {
                let req = Request::get("/doc").header("accept-encoding", encoding).body(Body::empty()).unwrap();
                let (parts, body) = app.oneshot(req).await.unwrap().into_parts();
                (parts.headers, body.collect().await.unwrap().to_bytes().to_vec())
            }
        };
        let chrome = "gzip, deflate, br, zstd";
        // The first read is compressed now, as any answer, and starts nothing.
        let (headers, _) = read(chrome).await;
        assert_eq!(headers[header::CONTENT_ENCODING], "zstd");
        assert!(!raw.stored_started());
        // The second starts making the stored forms; until they exist, it is compressed now too.
        let (headers, _) = read(chrome).await;
        assert_eq!(headers[header::CONTENT_ENCODING], "zstd");
        raw.wait_stored();
        // Then a client that takes brotli gets the stored brotli, decoding to the document.
        let (headers, body) = read(chrome).await;
        assert_eq!(headers[header::CONTENT_ENCODING], "br");
        assert_eq!(headers[header::VARY], "accept-encoding");
        // The identity length is gone; a length the stack sets is the stored form's.
        if let Some(length) = headers.get(header::CONTENT_LENGTH) {
            assert_eq!(length.to_str().unwrap(), body.len().to_string());
        }
        let mut decoder = brotli::DecompressorWriter::new(Vec::new(), 4096);
        decoder.write_all(&body).unwrap();
        assert_eq!(decoder.into_inner().unwrap(), text.as_bytes());
        // gzip for one that takes only gzip; one that takes only zstd gets it made now.
        let (headers, body) = read("gzip").await;
        assert_eq!(headers[header::CONTENT_ENCODING], "gzip");
        let mut out = String::new();
        std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(&body[..]), &mut out).unwrap();
        assert_eq!(out, text);
        let (headers, _) = read("zstd").await;
        assert_eq!(headers[header::CONTENT_ENCODING], "zstd");
        let (headers, body) = read("identity").await;
        assert!(!headers.contains_key(header::CONTENT_ENCODING));
        assert_eq!(body, text.as_bytes());
        // A client that ranks zstd above the stored codings gets zstd, made now.
        let (headers, _) = read("zstd, gzip;q=0.1").await;
        assert_eq!(headers[header::CONTENT_ENCODING], "zstd");
        assert_eq!(read("zstd;q=0.5, br;q=0.5").await.0[header::CONTENT_ENCODING], "br");
    }

    /// A handler or a middleware that replaces a held value's body keeps the extension, not the
    /// bytes: what goes out is the new body, compressed now, never the stored form of the old one.
    #[cfg(feature = "json")]
    #[tokio::test]
    async fn a_replaced_body_is_never_sent_as_the_stored_form_of_the_one_it_replaced() {
        let row = |i: usize| format!(r#"{{"id":{i},"name":"row {i}"}}"#);
        let held = format!("[{}]", (0..300).map(row).collect::<Vec<_>>().join(","));
        let other = format!("[{}]", (300..600).map(row).collect::<Vec<_>>().join(","));
        let raw = crate::json::Raw::new(held).unwrap();
        let (doc, replaced, replacement) = (raw.clone(), raw.clone(), other.clone());
        let app = apply(Router::new().route("/doc", get(move || std::future::ready(crate::Json(doc.clone())))).route(
            "/replaced",
            get(move || {
                let mut res = crate::JsonResponse::from(crate::Json(replaced.clone()));
                *res.body_mut() = Body::from(replacement.clone());
                std::future::ready(res)
            }),
        ));
        let read = |uri: &'static str| {
            let app = app.clone();
            async move {
                let req = Request::get(uri).header("accept-encoding", "br").body(Body::empty()).unwrap();
                let (parts, body) = app.oneshot(req).await.unwrap().into_parts();
                (parts.headers, body.collect().await.unwrap().to_bytes().to_vec())
            }
        };
        // The held value's stored forms exist...
        read("/doc").await;
        read("/doc").await;
        raw.wait_stored();
        assert_eq!(read("/doc").await.0[header::CONTENT_ENCODING], "br");
        // ...and a response that carries it with another body sends that body.
        let (headers, body) = read("/replaced").await;
        assert_eq!(headers[header::CONTENT_ENCODING], "br");
        let mut decoder = brotli::DecompressorWriter::new(Vec::new(), 4096);
        decoder.write_all(&body).unwrap();
        assert_eq!(decoder.into_inner().unwrap(), other.as_bytes());
    }

    #[test]
    fn one_pass_finds_the_coding_for_now_and_for_a_stored_form() {
        let at = |value: &str| {
            let a = negotiate(&accept(value));
            (a.now(), a.stored())
        };
        assert_eq!(at("gzip, deflate, br, zstd"), (Some(Coding::Zstd), Some(Coding::Brotli)));
        assert_eq!(at("zstd"), (Some(Coding::Zstd), None));
        assert_eq!(at("zstd, gzip"), (Some(Coding::Zstd), Some(Coding::Gzip)));
        assert_eq!(at("br;q=0.5, gzip"), (Some(Coding::Gzip), Some(Coding::Gzip)));
        assert_eq!(at("identity"), (None, None));
        assert_eq!(at("br;q=0, gzip;level=1;q=0.9 , zstd ; q=0.1"), (Some(Coding::Gzip), Some(Coding::Gzip)));
        assert_eq!(at("ZSTD;Q=0.2, Br;q=0.2"), (Some(Coding::Zstd), Some(Coding::Brotli)));
        assert_eq!(at("x-gzip;q=bad, br;q=0"), (Some(Coding::Gzip), Some(Coding::Gzip)));
        assert_eq!(negotiate(&HeaderMap::new()), Accepts::default());
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
