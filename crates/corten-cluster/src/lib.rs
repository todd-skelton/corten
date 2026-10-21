//! One [`corten_feed::Feed`] served by every instance of an app.
//!
//! ```text
//! write ──► Postgres: app rows + corten_feeds.seq + corten_outbox row, one transaction
//!                        │ (NOTIFY at commit)
//!                        ▼
//!           relay (one instance at a time, by lease) ──► NATS JetStream, deduplicated by message id
//!                                                            │
//!           every instance: Hub ◄────────────────────────────┘ applies patches to its Feed
//!                                └──► SSE clients, who resume on any instance with the same cursor
//! ```
//!
//! A patch exists if and only if its write committed (the outbox row is in the same transaction).
//! The relay publishes outbox rows in order and marks them published only after JetStream
//! acknowledged them; a relay that dies in between republishes, and JetStream drops the copy.
//! A gap or a consumer restart reloads the feed from Postgres. Measured with two instances, kills
//! on a schedule and 200 clients: see `examples/cluster`.
//!
//! The app provides its rows through [`Source`] and writes through [`begin`] / [`Write::commit`],
//! with an optional idempotency key so a retried request commits once.

mod hub;
mod relay;
mod store;

use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use corten_feed::{Feed, Row, Since};
use serde::de::DeserializeOwned;
use sqlx::{PgConnection, PgPool};

pub use store::{begin, create_feed, migrate, prune, seq, Begin, Write, Written};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Where a feed's rows live. Called inside the cluster's consistent read, so the rows match the
/// sequence number and replay log loaded with them.
pub trait Source: Send + Sync + 'static {
    type Row: Row + DeserializeOwned;

    /// Every row of feed `key`.
    fn rows(&self, conn: &mut PgConnection, key: &str) -> impl Future<Output = Result<Vec<Self::Row>, Error>> + Send;
}

/// Settings shared by every instance of an app (except `instance`).
#[derive(Clone, Debug)]
pub struct Options {
    /// Names this instance in logs and in the relay lease.
    pub instance: String,
    /// JetStream stream that carries the patches.
    pub stream: String,
    /// Feed `k` is published on `{subject_prefix}.k`.
    pub subject_prefix: String,
    /// The relay lease. The active relay renews it every quarter and steps down if it cannot
    /// renew within three quarters, so after an instance dies another takes over within about
    /// `lease`, even when its connections never close (a frozen VM, a network partition).
    pub lease: Duration,
    /// How long JetStream keeps patches. Followers only need the ones since they loaded.
    pub max_age: Duration,
    /// JetStream drops a republished patch within this window.
    pub duplicate_window: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            instance: "corten".into(),
            stream: "CORTEN_FEEDS".into(),
            subject_prefix: "corten.feeds".into(),
            lease: Duration::from_secs(2),
            max_age: Duration::from_secs(3600),
            duplicate_window: Duration::from_secs(120),
        }
    }
}

/// This instance's view of the cluster: the feeds it serves and whether it runs the relay.
pub struct Cluster<S: Source> {
    hub: Arc<hub::Hub<S>>,
    relay_active: Arc<AtomicBool>,
}

impl<S: Source> Clone for Cluster<S> {
    fn clone(&self) -> Self {
        Cluster { hub: self.hub.clone(), relay_active: self.relay_active.clone() }
    }
}

impl<S: Source> Cluster<S> {
    /// Creates the tables if needed, starts this instance's JetStream consumer (before any feed
    /// loads, so no patch falls between a load and the stream) and its relay candidate.
    pub async fn start(pool: PgPool, nats: async_nats::Client, source: S, options: Options) -> Result<Self, Error> {
        store::migrate(&pool).await?;
        let js = async_nats::jetstream::new(nats);
        let options = Arc::new(options);
        let hub = hub::Hub::new(pool.clone(), source, options.clone());
        hub::start(hub.clone(), js.clone()).await?;
        let relay_active = Arc::new(AtomicBool::new(false));
        tokio::spawn(relay::run(pool, js, options, relay_active.clone()));
        Ok(Cluster { hub, relay_active })
    }

    /// The feed for `key`, loaded on first use. `None` when no such feed exists.
    pub async fn feed(&self, key: &str) -> Result<Option<Feed<S::Row>>, Error> {
        self.hub.feed(key).await
    }

    /// `GET {url}`: [`Feed::snapshot`], 404 for an unknown feed, 503 when Postgres is unreachable.
    pub async fn snapshot(&self, key: &str) -> Response {
        match self.feed(key).await {
            Ok(Some(feed)) => feed.snapshot(),
            other => unavailable(key, other.err()),
        }
    }

    /// `GET {url}/live`: [`Feed::live`], with the same errors as [`Cluster::snapshot`].
    pub async fn live(&self, key: &str, since: Since) -> Response {
        self.live_with(key, since, ()).await
    }

    /// [`Cluster::live`] holding `guard` (a `corten_feed::Permit`) until the stream ends.
    pub async fn live_with(&self, key: &str, since: Since, guard: impl Send + 'static) -> Response {
        match self.feed(key).await {
            Ok(Some(feed)) => feed.live_with(since, guard),
            other => unavailable(key, other.err()),
        }
    }

    /// True while this instance holds the relay lease.
    pub fn relay_active(&self) -> bool {
        self.relay_active.load(Ordering::Relaxed)
    }

    /// Loaded feeds: key, sequence, live subscribers.
    pub fn stats(&self) -> Vec<(String, u64, usize)> {
        self.hub.stats()
    }
}

fn unavailable(key: &str, error: Option<Error>) -> Response {
    match error {
        None => (StatusCode::NOT_FOUND, "No such feed.").into_response(),
        Some(e) => {
            eprintln!("corten-cluster: feed {key}: {e}");
            (StatusCode::SERVICE_UNAVAILABLE, "Feed unavailable.").into_response()
        }
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests;
