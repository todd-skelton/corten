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
//! `corten::compress::apply(router)` wraps the whole app. Event streams, images, fonts, bodies
//! under 1 KiB and responses that are already encoded or marked `no-transform` pass through.

use std::{
    cell::RefCell,
    io::Write,
    sync::{atomic::AtomicBool, atomic::Ordering, Arc, OnceLock},
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
    let headers = res.headers();
    let content_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
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
                .route("/png", get(|| async { ([(header::CONTENT_TYPE, "image/png")], vec![0u8; 4096]) })),
        )
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
