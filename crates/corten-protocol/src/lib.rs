//! The live-data protocol every Corten surface speaks.
//!
//! A feed is a keyed set of rows. A client loads a [`Snapshot`] (every row, plus the cursor it is
//! current at), then applies numbered [`Patch`]es in order. The server side is `corten-feed`;
//! the browser side is `subscribe()` in `corten.js`; [`Replica`] is the same client logic in Rust,
//! for native apps, agents and tests. Everything here is plain data: the transport is HTTP for
//! snapshots and server-sent events for patches, which every platform can read.
//!
//! ```text
//! GET {url}                 {"epoch":"e","seq":N,"rows":[...]}
//! GET {url}/live?since=e-N  event: patch  id: e-N  data: {"seq":N,"changes":[{"op":"upsert","id":"a","value":{...}},{"op":"remove","id":"b"}]}
//!                           event: sync   data: {"reason":"cursor-expired"|"replay-backpressure"}  (then the stream ends)
//! ```

use std::{collections::BTreeMap, fmt};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// A row a feed can hold: serializable, with a stable id.
pub trait Row: Serialize + Clone + Send + Sync + 'static {
    fn id(&self) -> &str;
}

/// One row change, borrowed, for writing a patch with [`patch_json`].
#[derive(Serialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Change<'a, T> {
    Upsert { id: &'a str, value: &'a T },
    Remove { id: &'a str },
}

#[derive(Serialize)]
struct PatchRef<'a, T> {
    seq: u64,
    changes: &'a [Change<'a, T>],
}

/// The JSON for patch `seq`. A writer that keeps its log elsewhere (an outbox row) stores this
/// string, and every follower and client receives it byte for byte.
pub fn patch_json<T: Serialize>(seq: u64, changes: &[Change<'_, T>]) -> String {
    serde_json::to_string(&PatchRef { seq, changes }).expect("feed rows serialize to JSON")
}

/// One row change as read from the wire (the owned form of [`Change`]).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op<T> {
    Upsert { id: String, value: T },
    Remove { id: String },
}

/// One write: every row it touched, with its final value or as a removal.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Patch<T> {
    pub seq: u64,
    pub changes: Vec<Op<T>>,
}

impl<T: DeserializeOwned> Patch<T> {
    /// Parses a patch as [`patch_json`] wrote it. It must be one line, so it fits one SSE `data:`.
    pub fn parse(json: &str) -> Result<Self, ApplyError> {
        if json.contains('\n') {
            return Err(ApplyError::Invalid("patch JSON must be one line".into()));
        }
        serde_json::from_str(json).map_err(|e| ApplyError::Invalid(e.to_string()))
    }
}

/// Every row of a feed and the cursor they are current at.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Snapshot<T> {
    pub epoch: String,
    pub seq: u64,
    pub rows: Vec<T>,
}

/// [`Snapshot`] for writing, without copying the rows.
#[derive(Serialize)]
pub struct SnapshotRef<'a, T> {
    pub epoch: &'a str,
    pub seq: u64,
    pub rows: &'a [T],
}

/// Where a client is in a feed: `epoch-seq` on the wire (`Last-Event-ID` or `?since=`). The epoch
/// names the log, so a cursor is only valid against the same log; it cannot contain `-`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub epoch: String,
    pub seq: u64,
}

impl Cursor {
    pub fn new(epoch: impl Into<String>, seq: u64) -> Self {
        Cursor { epoch: epoch.into(), seq }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let (epoch, seq) = value.split_once('-')?;
        Some(Cursor { epoch: epoch.to_string(), seq: seq.parse().ok()? })
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.epoch, self.seq)
    }
}

/// Why the server ended a live stream with `sync`: the client must load a new snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncReason {
    /// The cursor is from another log, older than the replay buffer, or too far ahead.
    CursorExpired,
    /// The client fell too far behind the live stream.
    ReplayBackpressure,
}

impl SyncReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SyncReason::CursorExpired => "cursor-expired",
            SyncReason::ReplayBackpressure => "replay-backpressure",
        }
    }
}

/// What applying a patch did.
#[derive(Debug, PartialEq, Eq)]
pub enum Applied {
    Applied,
    /// At or below the current `seq`: already applied.
    Duplicate,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// The patch is not the next one: patches `have + 1 .. got` are missing. Load a snapshot.
    Gap {
        have: u64,
        got: u64,
    },
    Invalid(String),
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplyError::Gap { have, got } => write!(f, "feed gap: have {have}, got {got}"),
            ApplyError::Invalid(e) => write!(f, "invalid patch: {e}"),
        }
    }
}

impl std::error::Error for ApplyError {}

/// The ordering rule, shared by every side: patch `got` applies on top of `have` only when it is
/// the next one. `Ok(Applied)` means apply it, `Ok(Duplicate)` means skip it.
pub fn check_seq(have: u64, got: u64) -> Result<Applied, ApplyError> {
    if got <= have {
        Ok(Applied::Duplicate)
    } else if got == have + 1 {
        Ok(Applied::Applied)
    } else {
        Err(ApplyError::Gap { have, got })
    }
}

/// Applies a patch's changes to rows keyed by id.
pub fn apply_ops<T>(rows: &mut BTreeMap<String, T>, ops: Vec<Op<T>>) {
    for op in ops {
        match op {
            Op::Upsert { id, value } => {
                rows.insert(id, value);
            }
            Op::Remove { id } => {
                rows.remove(&id);
            }
        }
    }
}

/// A client's copy of a feed: a snapshot plus every later patch, in order. The Rust twin of
/// `subscribe()` in `corten.js`; a native app or agent drives it from its own HTTP/SSE client
/// and loads a new snapshot whenever [`Replica::apply_json`] reports a gap or the server sends
/// `sync`.
#[derive(Clone, Debug)]
pub struct Replica<T> {
    epoch: String,
    seq: u64,
    rows: BTreeMap<String, T>,
}

impl<T: Row + DeserializeOwned> Replica<T> {
    pub fn new(snapshot: Snapshot<T>) -> Self {
        let rows = snapshot.rows.into_iter().map(|r| (r.id().to_string(), r)).collect();
        Replica { epoch: snapshot.epoch, seq: snapshot.seq, rows }
    }

    /// Applies the next patch. A duplicate is ignored; a gap leaves the rows untouched.
    pub fn apply(&mut self, patch: Patch<T>) -> Result<Applied, ApplyError> {
        let applied = check_seq(self.seq, patch.seq)?;
        if applied == Applied::Applied {
            apply_ops(&mut self.rows, patch.changes);
            self.seq = patch.seq;
        }
        Ok(applied)
    }

    /// [`Replica::apply`] for the `data:` of a `patch` event.
    pub fn apply_json(&mut self, json: &str) -> Result<Applied, ApplyError> {
        self.apply(Patch::parse(json)?)
    }

    /// The cursor to resume the live stream from (`?since=` or `Last-Event-ID`).
    pub fn cursor(&self) -> Cursor {
        Cursor::new(self.epoch.clone(), self.seq)
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn rows(&self) -> &BTreeMap<String, T> {
        &self.rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct Item {
        id: String,
        n: i32,
    }

    impl Row for Item {
        fn id(&self) -> &str {
            &self.id
        }
    }

    fn item(id: &str, n: i32) -> Item {
        Item { id: id.into(), n }
    }

    #[test]
    fn patch_json_round_trips_through_patch_parse() {
        let a = item("a", 1);
        let json = patch_json(7, &[Change::Upsert { id: "a", value: &a }, Change::Remove { id: "b" }]);
        assert_eq!(json, r#"{"seq":7,"changes":[{"op":"upsert","id":"a","value":{"id":"a","n":1}},{"op":"remove","id":"b"}]}"#);
        let patch: Patch<Item> = Patch::parse(&json).unwrap();
        assert_eq!(patch.changes, vec![Op::Upsert { id: "a".into(), value: a }, Op::Remove { id: "b".into() }]);
        assert!(Patch::<Item>::parse("{\"seq\":1,\n\"changes\":[]}").is_err(), "one line only");
    }

    #[test]
    fn cursor_parses_and_prints_epoch_dash_seq() {
        assert_eq!(Cursor::parse("g7-12"), Some(Cursor::new("g7", 12)));
        assert_eq!(Cursor::new("g7", 12).to_string(), "g7-12");
        assert_eq!(Cursor::parse("g7"), None);
        assert_eq!(Cursor::parse("g7-x"), None);
    }

    #[test]
    fn replica_applies_in_order_ignores_duplicates_and_stops_at_gaps() {
        let snapshot: Snapshot<Item> =
            serde_json::from_str(r#"{"epoch":"g1","seq":10,"rows":[{"id":"a","n":1},{"id":"b","n":2}]}"#).unwrap();
        let mut replica = Replica::new(snapshot);
        let up = |seq, n| patch_json(seq, &[Change::Upsert { id: "a", value: &item("a", n) }]);
        assert_eq!(replica.apply_json(&up(11, 5)), Ok(Applied::Applied));
        assert_eq!(replica.apply_json(&up(11, 99)), Ok(Applied::Duplicate));
        assert_eq!(replica.apply_json(&patch_json::<Item>(12, &[Change::Remove { id: "b" }])), Ok(Applied::Applied));
        assert_eq!(replica.apply_json(&up(14, 99)), Err(ApplyError::Gap { have: 12, got: 14 }));
        assert_eq!(replica.rows().values().cloned().collect::<Vec<_>>(), vec![item("a", 5)]);
        assert_eq!(replica.cursor().to_string(), "g1-12");
    }
}
