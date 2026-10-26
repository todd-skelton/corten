//! The server side of a live feed: a keyed set of rows with a snapshot and an ordered patch stream.
//!
//! Pages stay server-rendered. When rows on a page change live (a seller grid), the server keeps
//! the rows in a [`Feed`], writes go through [`Feed::write`], and every write becomes one
//! numbered patch. Two routes per feed:
//!
//! | Route | Response |
//! |---|---|
//! | `GET {url}` | [`Feed::snapshot`]: `{"epoch":"e","seq":N,"rows":[...]}` (compressible JSON) |
//! | `GET {url}/live?since=e-N` | [`Feed::live`]: server-sent events after `N` |
//!
//! Events on the live stream (types in `corten-protocol`):
//!
//! ```text
//! event: patch   id: e-N   data: {"seq":N,"changes":[{"op":"upsert","id":"a","value":{...}},{"op":"remove","id":"b"}]}
//! event: sync    data: {"reason":"cursor-expired"|"replay-backpressure"}   (then the stream ends)
//! ```
//!
//! `sync` tells the client to fetch a new snapshot: its cursor is from another server process
//! (`epoch`), older than the replay buffer, or the client fell too far behind. The browser
//! client is `subscribe()` in `corten.js` and the Rust client is `corten_protocol::Replica`;
//! both also resync when they see a gap in `seq`. The wire types live in `corten-protocol`,
//! which has no server dependencies, so native apps and agents share them.
//!
//! A feed either owns its rows ([`Feed::new`] + [`Feed::write`], one process) or follows a log
//! written elsewhere ([`Feed::follow`] + [`Feed::apply`]): every server instance applies the same
//! numbered patches, and the epoch is a generation stored with the data instead of a process id,
//! so a cursor from one instance resumes on another. A follower that loads its rows after other
//! instances served the feed calls [`Feed::backfill`] with the log's recent patches, so clients
//! that move to it (their instance died) resume instead of loading a new snapshot.
//!
//! Authorization is the route's job: check that the user may read the feed before calling
//! [`Feed::snapshot`] or [`Feed::live`]. A cursor only positions a client inside a feed it was
//! already allowed to read, so cursors need no signature. Cap open streams per user with a
//! [`Limiter`] and [`Feed::live_with`]. A page that follows several feeds opens one stream per
//! feed; over HTTP/2 those share one connection.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    convert::Infallible,
    sync::{Arc, LazyLock, Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::{Body, Bytes},
    extract::FromRequestParts,
    http::{header, request::Parts, HeaderValue},
    response::{IntoResponse, Response},
};
use corten_protocol::{apply_ops, check_seq, Patch, SnapshotRef};
pub use corten_protocol::{patch_json, Applied, ApplyError, Change, Cursor, Row, SyncReason};
use futures_util::{stream, StreamExt};
use serde::de::DeserializeOwned;
use tokio::sync::{
    broadcast::{
        self,
        error::{RecvError, TryRecvError},
    },
    watch,
};

/// Patches kept for clients that reconnect or subscribe right after a snapshot...
pub const REPLAY: usize = 1_024;
/// ...and at most this many bytes of them, so memory stays bounded at any write rate.
pub const REPLAY_BYTES: usize = 8 << 20;
/// Patches a live client may fall behind before it is told to resync. The broadcast ring holds
/// this many batches, so it also bounds per-feed memory.
pub const BACKLOG: usize = 1_024;
/// A comment line goes out after this long without a patch, so proxies keep the stream open.
const KEEP_ALIVE: Duration = Duration::from_secs(15);
/// Most patches sent in one chunk to a client that is behind.
const COALESCE: usize = 64;
/// First bytes of every live stream: `EventSource` reconnects after 1 s instead of its default.
const OPEN: &[u8] = b"retry: 1000\n\n";
/// How far a cursor may be ahead of this feed. Followers apply the same log a few milliseconds
/// apart, so a client whose snapshot came from one instance can open its stream on another that
/// has not caught up yet; the stream waits for it instead of forcing a resync.
pub const AHEAD: u64 = 1_024;

/// Set by [`drain`]; every live stream in the process watches it.
static DRAINING: LazyLock<watch::Sender<bool>> = LazyLock::new(|| watch::channel(false).0);

/// Ends every live stream in this process and refuses new ones (they end at once). Call it when
/// the server starts shutting down: open streams would otherwise keep their connections, and a
/// graceful shutdown, alive forever. Clients reconnect with their cursor and resume on another
/// instance (or this one, after a restart) without a snapshot.
pub fn drain() {
    tracing::info!(target: "corten::feed", "ending live streams for shutdown");
    DRAINING.send_replace(true);
}

/// A keyed set of rows with numbered patches. Cheap to clone; clones share the rows.
pub struct Feed<T: Row> {
    inner: Arc<Inner<T>>,
}

impl<T: Row> Clone for Feed<T> {
    fn clone(&self) -> Self {
        Feed { inner: self.inner.clone() }
    }
}

struct Inner<T> {
    /// Identifies this process's sequence, so a cursor from before a restart is rejected.
    epoch: String,
    state: Mutex<State<T>>,
    tx: broadcast::Sender<Arc<Batch>>,
}

struct State<T> {
    seq: u64,
    rows: BTreeMap<String, T>,
    log: VecDeque<Arc<Batch>>,
    log_bytes: usize,
    snapshot: Cached,
}

/// The last snapshot's body, kept until the rows or `seq` change. `version` counts the changes,
/// so a body serialized while a write happened is not kept.
#[derive(Default)]
struct Cached {
    version: u64,
    body: Option<Bytes>,
}

impl Cached {
    fn stale(&mut self) {
        self.version += 1;
        self.body = None;
    }
}

/// One write, encoded once as a complete SSE frame. Every subscriber sends the same bytes.
/// `ends` marks a reset: the frame is a `sync` event and the stream closes after it.
struct Batch {
    seq: u64,
    frame: Bytes,
    ends: bool,
}

impl<T: Row> Feed<T> {
    pub fn new(rows: impl IntoIterator<Item = T>) -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        Self::follow(format!("{:x}", nanos as u64), 0, rows)
    }

    /// A feed that mirrors a log kept elsewhere: `rows` as of `seq`, and `generation` naming the
    /// log (it must not contain `-`). Every instance that follows the same log with the same
    /// generation accepts the same cursors. Changes arrive through [`Feed::apply`].
    pub fn follow(generation: impl Into<String>, seq: u64, rows: impl IntoIterator<Item = T>) -> Self {
        let epoch = generation.into();
        assert!(!epoch.contains('-'), "a feed generation cannot contain '-'");
        let rows = rows.into_iter().map(|r| (r.id().to_string(), r)).collect();
        let (tx, _) = broadcast::channel(BACKLOG);
        let state = State { seq, rows, log: VecDeque::new(), log_bytes: 0, snapshot: Cached::default() };
        Feed { inner: Arc::new(Inner { epoch, state: Mutex::new(state), tx }) }
    }

    /// The generation (the `epoch` in cursors).
    pub fn generation(&self) -> &str {
        &self.inner.epoch
    }

    fn lock(&self) -> MutexGuard<'_, State<T>> {
        // A panic inside `write` leaves the rows as the closure left them; keep serving them.
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Reads the current rows (for server-rendering the first page, for example).
    pub fn read<R>(&self, f: impl FnOnce(&BTreeMap<String, T>) -> R) -> R {
        f(&self.lock().rows)
    }

    /// The sequence number of the last write.
    pub fn seq(&self) -> u64 {
        self.lock().seq
    }

    /// Open live streams on this feed.
    pub fn subscribers(&self) -> usize {
        self.inner.tx.receiver_count()
    }

    /// Changes rows and publishes the result as one patch. Every row the closure inserted,
    /// removed or borrowed mutably is sent with its final value (or as a removal). Returns the
    /// closure's result. Writes are serialized, so read-modify-write is safe.
    pub fn write<R>(&self, f: impl FnOnce(&mut Tx<'_, T>) -> R) -> R {
        let mut state = self.lock();
        let state = &mut *state;
        let mut tx = Tx { rows: &mut state.rows, snapshot: &mut state.snapshot, touched: Vec::new(), seen: HashSet::new() };
        let out = f(&mut tx);
        let Tx { rows, touched, .. } = tx;
        if touched.is_empty() {
            return out;
        }
        let changes: Vec<Change<'_, T>> = touched
            .iter()
            .map(|id| match rows.get(id) {
                Some(value) => Change::Upsert { id, value },
                None => Change::Remove { id },
            })
            .collect();
        let seq = state.seq + 1;
        let json = patch_json(seq, &changes);
        self.publish(state, seq, &json);
        out
    }

    /// Applies one patch from the log this feed follows, as written by [`patch_json`]. The JSON is
    /// sent to clients byte for byte, so every instance streams identical events. A patch at or
    /// below the current `seq` is a duplicate and is ignored; one further ahead is a gap, and the
    /// caller should load fresh rows and call [`Feed::reset`].
    pub fn apply(&self, json: &str) -> Result<Applied, ApplyError>
    where
        T: DeserializeOwned,
    {
        let patch: Patch<T> = Patch::parse(json)?;
        let mut state = self.lock();
        if check_seq(state.seq, patch.seq)? == Applied::Duplicate {
            return Ok(Applied::Duplicate);
        }
        // Before the rows change, as in a write.
        state.snapshot.stale();
        apply_ops(&mut state.rows, patch.changes);
        self.publish(&mut state, patch.seq, json);
        Ok(Applied::Applied)
    }

    /// Replaces every row (after a gap, for example). Live clients get `sync` and load a snapshot.
    pub fn reset(&self, seq: u64, rows: impl IntoIterator<Item = T>) {
        let mut state = self.lock();
        state.rows = rows.into_iter().map(|r| (r.id().to_string(), r)).collect();
        state.seq = seq;
        state.snapshot.stale();
        state.log.clear();
        state.log_bytes = 0;
        let _ = self.inner.tx.send(Arc::new(Batch { seq, frame: sync_frame(SyncReason::CursorExpired), ends: true }));
    }

    /// Adds patches the rows already include (as [`patch_json`] wrote them, oldest first) to the
    /// front of the replay log, so a cursor from before this feed was loaded still resumes. They
    /// must be consecutive and end right before the oldest patch in the log (at `seq` when the log
    /// is empty). Returns how many were kept; the log's limits still apply, newest first.
    pub fn backfill(&self, patches: impl IntoIterator<Item = String>) -> Result<usize, ApplyError> {
        let mut parsed = Vec::new();
        for json in patches {
            let patch: Patch<serde::de::IgnoredAny> = Patch::parse(&json)?;
            parsed.push((patch.seq, json));
        }
        let mut state = self.lock();
        let end = state.log.front().map_or(state.seq, |b| b.seq - 1);
        let start = (end + 1).checked_sub(parsed.len() as u64).filter(|s| *s > 0);
        let consecutive = parsed.iter().zip(start.into_iter().flat_map(|s| s..)).all(|((seq, _), want)| *seq == want);
        if parsed.is_empty() {
            return Ok(0);
        }
        if start.is_none() || !consecutive {
            let (first, last) = (parsed[0].0, parsed[parsed.len() - 1].0);
            return Err(ApplyError::Invalid(format!("backfill {first}..={last} does not end at {end}")));
        }
        let mut kept = 0;
        for (seq, json) in parsed.into_iter().rev() {
            let frame = self.frame(seq, &json);
            if state.log.len() >= REPLAY || state.log_bytes + frame.len() > REPLAY_BYTES {
                break;
            }
            state.log_bytes += frame.len();
            state.log.push_front(Arc::new(Batch { seq, frame, ends: false }));
            kept += 1;
        }
        Ok(kept)
    }

    fn frame(&self, seq: u64, json: &str) -> Bytes {
        // Compact JSON has no raw newlines, so it fits one `data:` line.
        Bytes::from(format!("event: patch\nid: {}-{seq}\ndata: {json}\n\n", self.inner.epoch))
    }

    /// Records patch `seq` (already applied to the rows) and sends it to live clients.
    fn publish(&self, state: &mut State<T>, seq: u64, json: &str) {
        let batch = Arc::new(Batch { seq, frame: self.frame(seq, json), ends: false });
        state.seq = seq;
        state.snapshot.stale();
        state.log_bytes += batch.frame.len();
        state.log.push_back(batch.clone());
        while state.log.len() > REPLAY || (state.log_bytes > REPLAY_BYTES && state.log.len() > 1) {
            if let Some(old) = state.log.pop_front() {
                state.log_bytes -= old.frame.len();
            }
        }
        // No subscribers is fine; the batch stays in the replay log.
        let _ = self.inner.tx.send(batch);
    }

    /// `GET {url}`: every row with the epoch and sequence number they are current at. The body is
    /// kept until the next change, so the snapshots between two writes share one serialization.
    pub fn snapshot(&self) -> FeedResponse {
        // Clone under the lock and serialize outside it, so large snapshots do not block writes.
        let (version, seq, rows) = {
            let state = self.lock();
            if let Some(body) = &state.snapshot.body {
                return FeedResponse(snapshot_response(body.clone()));
            }
            (state.snapshot.version, state.seq, state.rows.values().cloned().collect::<Vec<_>>())
        };
        let body = Bytes::from(
            serde_json::to_string(&SnapshotRef { epoch: &self.inner.epoch, seq, rows: &rows })
                .expect("feed rows serialize to JSON"),
        );
        drop(rows);
        self.keep_snapshot(version, &body);
        FeedResponse(snapshot_response(body))
    }

    /// Keeps a snapshot body written from the rows at `version`, unless they changed since.
    fn keep_snapshot(&self, version: u64, body: &Bytes) {
        let mut state = self.lock();
        if state.snapshot.version == version {
            state.snapshot.body = Some(body.clone());
        }
    }

    /// `GET {url}/live`: patches after the client's cursor, then new patches as they happen.
    pub fn live(&self, since: Since) -> FeedResponse {
        self.live_with(since, ())
    }

    /// [`Feed::live`] that keeps `guard` (a [`Permit`], say) alive until the stream ends.
    pub fn live_with(&self, since: Since, guard: impl Send + 'static) -> FeedResponse {
        let state = self.lock();
        let rx = self.inner.tx.subscribe();
        let last = state.seq;
        let start = match since.0 {
            Some(Cursor { epoch, seq }) if epoch == self.inner.epoch && seq > last => {
                if seq - last > AHEAD {
                    Err(SyncReason::CursorExpired)
                } else {
                    Ok((Vec::new(), seq))
                }
            }
            Some(Cursor { epoch, seq }) if epoch == self.inner.epoch => {
                let oldest = state.log.front().map_or(last + 1, |b| b.seq);
                if seq + 1 < oldest {
                    Err(SyncReason::CursorExpired)
                } else {
                    Ok((state.log.iter().filter(|b| b.seq > seq).cloned().collect::<Vec<_>>(), last))
                }
            }
            _ => Err(SyncReason::CursorExpired),
        };
        drop(state);
        let (first, last, done) = match start {
            // `retry` first: it also tells the client the stream is open when there is no replay.
            Ok((replay, from)) => {
                let mut chunk = OPEN.to_vec();
                for b in &replay {
                    chunk.extend_from_slice(&b.frame);
                }
                (chunk.into(), from, false)
            }
            Err(reason) => (sync_frame(reason), last, true),
        };
        let rest = stream::unfold(StreamState { rx, last, done, drain: DRAINING.subscribe() }, next_chunk);
        let chunks = stream::iter([first]).chain(rest).map(move |chunk| {
            let _held = &guard;
            Ok::<Bytes, Infallible>(chunk)
        });
        FeedResponse(
            (
                [
                    (header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream")),
                    (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
                ],
                Body::from_stream(chunks),
            )
                .into_response(),
        )
    }
}

/// A feed's snapshot ([`Feed::snapshot`]) or live stream ([`Feed::live`]): a response a route
/// names by its type, so a handler that returns one says it serves a feed.
pub struct FeedResponse(Response);

impl FeedResponse {
    /// The response's body.
    pub fn into_body(self) -> Body {
        self.0.into_body()
    }
}

impl IntoResponse for FeedResponse {
    fn into_response(self) -> Response {
        self.0
    }
}

/// Caps how many live streams one key (a user, an account) holds open on this instance.
/// Cheap to clone; clones share the counts.
#[derive(Clone)]
pub struct Limiter {
    max: usize,
    open: Arc<Mutex<HashMap<String, usize>>>,
}

impl Limiter {
    pub fn new(max_per_key: usize) -> Self {
        Limiter { max: max_per_key, open: Arc::default() }
    }

    /// A permit for one more stream, or `None` when `key` already has the maximum (answer 429).
    pub fn acquire(&self, key: &str) -> Option<Permit> {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        let count = open.entry(key.to_string()).or_insert(0);
        if *count >= self.max {
            tracing::debug!(target: "corten::feed", key, max = self.max, "live stream refused: limit reached");
            return None;
        }
        *count += 1;
        Some(Permit { key: key.to_string(), open: self.open.clone() })
    }

    /// Streams `key` has open.
    pub fn open(&self, key: &str) -> usize {
        self.open.lock().unwrap_or_else(|e| e.into_inner()).get(key).copied().unwrap_or(0)
    }
}

/// One open stream for a key; released when dropped (the stream ended or the client left).
pub struct Permit {
    key: String,
    open: Arc<Mutex<HashMap<String, usize>>>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(count) = open.get_mut(&self.key) {
            *count -= 1;
            if *count == 0 {
                open.remove(&self.key);
            }
        }
    }
}

/// One live stream's position: its receiver, the last seq sent, whether it has ended, and the
/// process-wide drain signal.
struct StreamState {
    rx: broadcast::Receiver<Arc<Batch>>,
    last: u64,
    done: bool,
    drain: watch::Receiver<bool>,
}

/// The next bytes for one live client: the next patch, plus any patches already waiting for it,
/// so a client that fell behind catches up in one write. `None` ends the stream.
async fn next_chunk(mut s: StreamState) -> Option<(Bytes, StreamState)> {
    if s.done || *s.drain.borrow() {
        return None;
    }
    let last = s.last;
    let first = loop {
        let next = tokio::select! {
            next = tokio::time::timeout(KEEP_ALIVE, s.rx.recv()) => next,
            _ = s.drain.wait_for(|draining| *draining) => return None,
        };
        match next {
            Err(_) => return Some((Bytes::from_static(b":\n\n"), s)),
            Ok(Ok(b)) if b.ends => {
                s.done = true;
                return Some((b.frame.clone(), s));
            }
            Ok(Ok(b)) if b.seq <= last => continue,
            Ok(Ok(b)) => break b,
            Ok(Err(RecvError::Lagged(_))) => {
                s.done = true;
                return Some((sync_frame(SyncReason::ReplayBackpressure), s));
            }
            Ok(Err(RecvError::Closed)) => return None,
        }
    };
    let mut last = first.seq;
    let mut more = Vec::new();
    let mut lagged = false;
    while more.len() < COALESCE {
        match s.rx.try_recv() {
            Ok(b) if b.ends => {
                more.push(b);
                lagged = true; // ends the stream after this chunk; the reset frame is already in it
                break;
            }
            Ok(b) if b.seq <= last => {}
            Ok(b) => {
                last = b.seq;
                more.push(b);
            }
            Err(TryRecvError::Lagged(_)) => {
                lagged = true;
                break;
            }
            Err(_) => break,
        }
    }
    if more.is_empty() && !lagged {
        s.last = last;
        return Some((first.frame.clone(), s));
    }
    let mut chunk = first.frame.to_vec();
    for b in &more {
        chunk.extend_from_slice(&b.frame);
    }
    if lagged && !more.last().is_some_and(|b| b.ends) {
        chunk.extend_from_slice(&sync_frame(SyncReason::ReplayBackpressure));
    }
    s.last = last;
    s.done = lagged;
    Some((chunk.into(), s))
}

/// Also the feed's one observable event: a client sent back for a fresh snapshot. Logged with
/// `tracing` (target `corten::feed`, field `reason`), so an app's subscriber can count it; a rising
/// rate means a replay log too small for the write rate or clients that cannot keep up.
fn sync_frame(reason: SyncReason) -> Bytes {
    tracing::debug!(target: "corten::feed", reason = reason.as_str(), "client resyncs from a snapshot");
    Bytes::from(format!("event: sync\ndata: {{\"reason\":\"{}\"}}\n\n", reason.as_str()))
}

fn snapshot_response(body: Bytes) -> Response {
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("application/json")),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        body,
    )
        .into_response()
}

/// Rows inside [`Feed::write`]. Reads are free; anything that may change a row marks it.
pub struct Tx<'a, T> {
    rows: &'a mut BTreeMap<String, T>,
    snapshot: &'a mut Cached,
    touched: Vec<String>,
    seen: HashSet<String>,
}

impl<T: Row> Tx<'_, T> {
    fn touch(&mut self, id: &str) {
        if self.seen.insert(id.to_string()) {
            self.touched.push(id.to_string());
            // Before the row changes: a closure that panics still leaves no stale snapshot.
            self.snapshot.stale();
        }
    }

    pub fn get(&self, id: &str) -> Option<&T> {
        self.rows.get(id)
    }

    /// A row to change. It is published even if the closure leaves it as it was.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut T> {
        if !self.rows.contains_key(id) {
            return None;
        }
        self.touch(id);
        self.rows.get_mut(id)
    }

    /// Inserts or replaces the row with the same id.
    pub fn insert(&mut self, row: T) {
        let id = row.id().to_string();
        self.touch(&id);
        self.rows.insert(id, row);
    }

    pub fn remove(&mut self, id: &str) -> Option<T> {
        let row = self.rows.remove(id)?;
        self.touch(id);
        Some(row)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// The client's cursor: the `Last-Event-ID` header (an `EventSource` reconnecting) or the
/// `since` query parameter (a new subscription after a snapshot), as `epoch-seq`.
pub struct Since(pub Option<Cursor>);

impl<S: Send + Sync> FromRequestParts<S> for Since {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let header = parts.headers.get("last-event-id").and_then(|v| v.to_str().ok()).and_then(Cursor::parse);
        let query = || parts.uri.query()?.split('&').find_map(|pair| pair.strip_prefix("since=")).and_then(Cursor::parse);
        Ok(Since(header.or_else(query)))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::body::Body;
    use http_body_util::BodyExt;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
    struct Item {
        id: String,
        n: i32,
    }

    impl Row for Item {
        fn id(&self) -> &str {
            &self.id
        }
    }

    fn feed() -> Feed<Item> {
        Feed::new((1..=3).map(|n| Item { id: format!("i{n}"), n }))
    }

    async fn body_text(res: impl IntoResponse) -> String {
        let res = res.into_response();
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }

    /// Reads SSE text until `events` events arrived or the stream went quiet.
    async fn read_events(body: &mut Body, events: usize) -> String {
        let mut text = String::new();
        while text.matches("\n\n").count() < events {
            match tokio::time::timeout(Duration::from_millis(500), body.frame()).await {
                Ok(Some(Ok(frame))) => {
                    if let Some(data) = frame.data_ref() {
                        text.push_str(std::str::from_utf8(data).unwrap());
                    }
                }
                _ => break,
            }
        }
        text
    }

    fn cursor(feed: &Feed<Item>, seq: u64) -> Since {
        Since(Some(Cursor::new(feed.inner.epoch.clone(), seq)))
    }

    #[tokio::test]
    async fn snapshot_has_epoch_seq_and_rows() {
        let feed = feed();
        feed.write(|tx| tx.get_mut("i2").unwrap().n = 20);
        let json: serde_json::Value = serde_json::from_str(&body_text(feed.snapshot()).await).unwrap();
        assert_eq!(json["seq"], 1);
        assert_eq!(json["epoch"], feed.inner.epoch.as_str());
        assert_eq!(json["rows"][1], serde_json::json!({ "id": "i2", "n": 20 }));
    }

    fn kept(feed: &Feed<Item>) -> bool {
        feed.lock().snapshot.body.is_some()
    }

    /// The rows a snapshot sends, and its seq.
    async fn snapshot_of(feed: &Feed<Item>) -> (u64, Vec<Item>) {
        let snap: corten_protocol::Snapshot<Item> = serde_json::from_str(&body_text(feed.snapshot()).await).unwrap();
        (snap.seq, snap.rows)
    }

    #[tokio::test]
    async fn a_snapshot_is_kept_until_the_rows_or_seq_change() {
        let feed = feed();
        let first = body_text(feed.snapshot()).await;
        assert!(kept(&feed));
        assert_eq!(body_text(feed.snapshot()).await, first);
        // A write that touches nothing publishes nothing and keeps it.
        feed.write(|tx| assert_eq!(tx.get("i1").unwrap().n, 1));
        assert!(kept(&feed));
        feed.write(|tx| tx.get_mut("i2").unwrap().n = 20);
        assert!(!kept(&feed));
        assert_eq!(snapshot_of(&feed).await.1[1].n, 20);
        feed.write(|tx| tx.insert(Item { id: "i4".into(), n: 4 }));
        assert_eq!(snapshot_of(&feed).await.1.len(), 4);
        feed.write(|tx| tx.remove("i1"));
        let (seq, rows) = snapshot_of(&feed).await;
        assert_eq!((seq, rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>()), (3, vec!["i2", "i3", "i4"]));
        // A write that panics after changing a row leaves the row changed, as the feed serves it.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            feed.write(|tx| {
                tx.get_mut("i2").unwrap().n = 99;
                panic!("mid-write");
            })
        }));
        assert!(panicked.is_err());
        assert_eq!(snapshot_of(&feed).await.1[0].n, 99);
    }

    #[tokio::test]
    async fn a_follower_s_snapshot_follows_apply_and_reset() {
        let (feed, _) = followers();
        assert_eq!(snapshot_of(&feed).await.0, 10);
        feed.apply(&patch(11, "i1", 100)).unwrap();
        assert_eq!(
            snapshot_of(&feed).await,
            (11, vec![Item { id: "i1".into(), n: 100 }, Item { id: "i2".into(), n: 2 }, Item { id: "i3".into(), n: 3 },])
        );
        // A duplicate changes nothing and keeps it.
        feed.apply(&patch(11, "i1", 7)).unwrap();
        assert!(kept(&feed));
        // A reset to the same seq with other rows is a change all the same.
        feed.reset(11, [Item { id: "z".into(), n: 0 }]);
        assert!(!kept(&feed));
        assert_eq!(snapshot_of(&feed).await, (11, vec![Item { id: "z".into(), n: 0 }]));
    }

    #[test]
    fn a_snapshot_written_while_the_rows_changed_is_not_kept() {
        let feed = feed();
        let version = feed.lock().snapshot.version;
        // A write lands between reading the rows and keeping the body.
        feed.write(|tx| tx.get_mut("i1").unwrap().n = 10);
        feed.keep_snapshot(version, &Bytes::from_static(b"old rows"));
        assert!(!kept(&feed));
        let version = feed.lock().snapshot.version;
        feed.keep_snapshot(version, &Bytes::from_static(b"current rows"));
        assert!(kept(&feed));
    }

    #[test]
    fn write_publishes_final_values_once_per_row_and_skips_empty_writes() {
        let feed = feed();
        let mut rx = feed.inner.tx.subscribe();
        feed.write(|tx| {
            tx.get_mut("i1").unwrap().n = 10;
            tx.get_mut("i1").unwrap().n = 11;
            tx.remove("i3");
            tx.insert(Item { id: "i4".into(), n: 4 });
        });
        feed.write(|tx| assert!(tx.get("i1").is_some()));
        let batch = rx.try_recv().unwrap();
        let epoch = &feed.inner.epoch;
        assert_eq!(
            std::str::from_utf8(&batch.frame).unwrap(),
            format!(
                "event: patch\nid: {epoch}-1\ndata: {}\n\n",
                r#"{"seq":1,"changes":[{"op":"upsert","id":"i1","value":{"id":"i1","n":11}},{"op":"remove","id":"i3"},"#
                    .to_string()
                    + r#"{"op":"upsert","id":"i4","value":{"id":"i4","n":4}}]}"#
            )
        );
        assert!(rx.try_recv().is_err(), "a write that touches nothing publishes nothing");
        assert_eq!(feed.seq(), 1);
    }

    #[tokio::test]
    async fn live_replays_after_the_cursor_then_streams_new_writes() {
        let feed = feed();
        for n in 1..=3 {
            feed.write(|tx| tx.get_mut("i1").unwrap().n = n * 100);
        }
        let mut body = feed.live(cursor(&feed, 1)).into_body();
        feed.write(|tx| tx.get_mut("i2").unwrap().n = 7);
        let text = read_events(&mut body, 4).await;
        assert!(text.starts_with("retry: 1000\n\n"));
        let ids: Vec<&str> = text.lines().filter_map(|l| l.strip_prefix("id: ")).collect();
        let epoch = &feed.inner.epoch;
        assert_eq!(ids, vec![format!("{epoch}-2"), format!("{epoch}-3"), format!("{epoch}-4")]);
        assert!(text.contains(r#"data: {"seq":4,"changes":[{"op":"upsert","id":"i2","value":{"id":"i2","n":7}}]}"#));
    }

    #[tokio::test]
    async fn unknown_or_expired_cursors_get_sync() {
        let feed = feed();
        feed.write(|tx| tx.get_mut("i1").unwrap().n = 1);
        for since in [Since(None), Since(Some(Cursor::new("other", 1))), cursor(&feed, 2 + AHEAD)] {
            let text = body_text(feed.live(since)).await;
            assert_eq!(text, "event: sync\ndata: {\"reason\":\"cursor-expired\"}\n\n", "stream ends after sync");
        }
        for n in 0..REPLAY as i32 + 1 {
            feed.write(|tx| tx.get_mut("i2").unwrap().n = n);
        }
        assert!(body_text(feed.live(cursor(&feed, 0))).await.contains("cursor-expired"), "older than the replay log");
    }

    #[tokio::test]
    async fn a_cursor_slightly_ahead_waits_for_the_feed_to_catch_up() {
        let feed = feed();
        let mut body = feed.live(cursor(&feed, 2)).into_body();
        assert_eq!(read_events(&mut body, 1).await, "retry: 1000\n\n");
        for n in 1..=3 {
            feed.write(|tx| tx.get_mut("i1").unwrap().n = n);
        }
        let text = read_events(&mut body, 1).await;
        assert!(text.starts_with(&format!("event: patch\nid: {}-3\n", feed.inner.epoch)), "{text}");
    }

    #[tokio::test]
    async fn a_client_that_falls_behind_gets_sync() {
        let feed = feed();
        let mut body = feed.live(cursor(&feed, 0)).into_body();
        for n in 0..BACKLOG as i32 + 10 {
            feed.write(|tx| tx.get_mut("i1").unwrap().n = n);
        }
        let text = read_events(&mut body, 2).await;
        let text = text.strip_prefix("retry: 1000\n\n").expect("the retry line comes first");
        assert!(text.starts_with("event: sync\ndata: {\"reason\":\"replay-backpressure\"}"), "{}", &text[..80.min(text.len())]);
    }

    #[tokio::test]
    async fn a_client_that_is_behind_gets_waiting_patches_in_one_chunk() {
        let feed = feed();
        let mut body = feed.live(cursor(&feed, 0)).into_body();
        let open = body.frame().await.unwrap().unwrap();
        assert_eq!(open.data_ref().unwrap().as_ref(), OPEN);
        for n in 1..=5 {
            feed.write(|tx| tx.get_mut("i1").unwrap().n = n);
        }
        let chunk = body.frame().await.unwrap().unwrap();
        let text = std::str::from_utf8(chunk.data_ref().unwrap()).unwrap();
        assert_eq!(text.matches("event: patch\n").count(), 5, "{text}");
        assert!(text.ends_with(
            "data: {\"seq\":5,\"changes\":[{\"op\":\"upsert\",\"id\":\"i1\",\"value\":{\"id\":\"i1\",\"n\":5}}]}\n\n"
        ));
    }

    #[tokio::test]
    async fn since_prefers_last_event_id() {
        let req = axum::http::Request::builder().uri("/feed/live?since=aa-3").header("last-event-id", "bb-7").body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let since = Since::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(since.0, Some(Cursor::new("bb", 7)));
        let (mut parts, _) = axum::http::Request::builder().uri("/x?a=1&since=aa-3").body(()).unwrap().into_parts();
        assert_eq!(Since::from_request_parts(&mut parts, &()).await.unwrap().0, Some(Cursor::new("aa", 3)));
    }

    fn followers() -> (Feed<Item>, Feed<Item>) {
        let rows = || (1..=3).map(|n| Item { id: format!("i{n}"), n });
        (Feed::follow("g7", 10, rows()), Feed::follow("g7", 10, rows()))
    }

    fn patch(seq: u64, id: &str, n: i32) -> String {
        let item = Item { id: id.into(), n };
        patch_json(seq, &[Change::Upsert { id, value: &item }])
    }

    #[tokio::test]
    async fn followers_stream_identical_bytes_and_accept_each_others_cursors() {
        let (a, b) = followers();
        for seq in 11..=13 {
            let json = patch(seq, "i1", seq as i32);
            assert_eq!(a.apply(&json), Ok(Applied::Applied));
            assert_eq!(b.apply(&json), Ok(Applied::Applied));
        }
        // A client that saw patch 12 on `a` resumes on `b` with the same cursor.
        let mut on_a = a.live(Since(Some(Cursor::new("g7", 11)))).into_body();
        let from_a = read_events(&mut on_a, 3).await;
        let mut on_b = b.live(Since(Some(Cursor::new("g7", 12)))).into_body();
        let from_b = read_events(&mut on_b, 2).await;
        assert!(from_b.starts_with("retry: 1000\n\nevent: patch\nid: g7-13\n"), "{from_b}");
        assert!(from_a.ends_with(&from_b["retry: 1000\n\n".len()..]), "same frame bytes on both instances");
        let json: serde_json::Value = serde_json::from_str(&body_text(b.snapshot()).await).unwrap();
        assert_eq!((json["epoch"].as_str(), json["seq"].as_u64()), (Some("g7"), Some(13)));
        assert_eq!(json["rows"][0]["n"], 13);
    }

    #[test]
    fn apply_ignores_duplicates_and_reports_gaps() {
        let (a, _) = followers();
        assert_eq!(a.apply(&patch(11, "i2", 1)), Ok(Applied::Applied));
        assert_eq!(a.apply(&patch(11, "i2", 99)), Ok(Applied::Duplicate));
        assert_eq!(a.apply(&patch(9, "i2", 99)), Ok(Applied::Duplicate));
        assert_eq!(a.apply(&patch(14, "i2", 99)), Err(ApplyError::Gap { have: 11, got: 14 }));
        assert!(matches!(a.apply("{\"seq\":12}"), Err(ApplyError::Invalid(_))));
        assert_eq!(a.read(|rows| rows["i2"].n), 1);
        assert_eq!(a.seq(), 11);
    }

    #[tokio::test]
    async fn reset_sends_sync_and_ends_live_streams() {
        let (a, _) = followers();
        let mut body = a.live(Since(Some(Cursor::new("g7", 10)))).into_body();
        assert_eq!(read_events(&mut body, 1).await, "retry: 1000\n\n");
        a.reset(20, [Item { id: "z".into(), n: 0 }]);
        assert_eq!(read_events(&mut body, 1).await, "event: sync\ndata: {\"reason\":\"cursor-expired\"}\n\n");
        assert!(body.frame().await.is_none(), "stream ends after sync");
        assert_eq!((a.seq(), a.read(|r| r.len())), (20, 1));
    }

    #[tokio::test]
    async fn a_late_follower_backfills_recent_patches_and_resumes_older_cursors() {
        let (a, _) = followers();
        let patches: Vec<String> = (11..=14).map(|seq| patch(seq, "i1", seq as i32)).collect();
        for json in &patches {
            a.apply(json).unwrap();
        }
        // `late` loads its rows at 14, after `a` served patches 11..=14.
        let late = Feed::follow("g7", 14, a.read(|rows| rows.values().cloned().collect::<Vec<_>>()));
        let cursor = || Since(Some(Cursor::new("g7", 12)));
        assert!(body_text(late.live(cursor())).await.contains("cursor-expired"), "nothing to replay yet");
        assert!(late.backfill(patches[..3].to_vec()).is_err(), "must end at the feed's seq");
        assert!(late.backfill([patches[1].clone(), patches[3].clone()]).is_err(), "must be consecutive");
        assert_eq!(late.backfill(patches[2..].to_vec()), Ok(2));
        assert_eq!(late.backfill(patches[..2].to_vec()), Ok(2), "older ones go in front");
        let mut from_a = a.live(cursor()).into_body();
        let mut from_late = late.live(cursor()).into_body();
        let (from_a, from_late) = (read_events(&mut from_a, 3).await, read_events(&mut from_late, 3).await);
        assert!(from_late.starts_with("retry: 1000\n\nevent: patch\nid: g7-13\n"), "{from_late}");
        assert_eq!(from_late, from_a, "the late follower replays the same bytes");
    }

    #[tokio::test]
    async fn a_limiter_caps_streams_per_key_until_they_end() {
        let feed = feed();
        let limiter = Limiter::new(2);
        let first = feed.live_with(Since(None), limiter.acquire("u1").unwrap());
        let second = limiter.acquire("u1").unwrap();
        assert!(limiter.acquire("u1").is_none(), "third stream for u1 refused");
        assert!(limiter.acquire("u2").is_some(), "other keys are independent");
        assert_eq!(limiter.open("u1"), 2);
        drop(second);
        assert_eq!(limiter.open("u1"), 1);
        drop(first);
        assert_eq!(limiter.open("u1"), 0, "dropping the response body releases its permit");
    }
}
