//! Postgres: per-feed sequence numbers, the outbox, idempotency keys and the relay lease. A write
//! changes the app's rows, takes the feed's next sequence number and stores the patch in the
//! outbox in one transaction, so a patch exists if and only if its change committed.

use std::time::Duration;

use corten::feed::{Change, Row};
use serde::Serialize;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};

use crate::{now_ms, Error, Source};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS corten_feeds (
    key text PRIMARY KEY,
    generation text NOT NULL,
    seq bigint NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS corten_outbox (
    id bigserial PRIMARY KEY,
    feed text NOT NULL,
    generation text NOT NULL,
    seq bigint NOT NULL,
    body text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    published_at timestamptz,
    UNIQUE (feed, seq)
);
CREATE INDEX IF NOT EXISTS corten_outbox_pending ON corten_outbox (id) WHERE published_at IS NULL;
CREATE TABLE IF NOT EXISTS corten_idempotency (
    key text PRIMARY KEY,
    feed text NOT NULL,
    seq bigint,
    committed_at bigint,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE IF NOT EXISTS corten_relay_lease (
    id int PRIMARY KEY CHECK (id = 1),
    holder text,
    holder_pid int,
    expires_at timestamptz NOT NULL DEFAULT '-infinity'
);
INSERT INTO corten_relay_lease (id) VALUES (1) ON CONFLICT DO NOTHING;
-- Wakes the relay. Notifications are delivered at commit, once per transaction.
CREATE OR REPLACE FUNCTION corten_outbox_notify() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('corten_outbox', '');
    RETURN NULL;
END $$;
CREATE OR REPLACE TRIGGER corten_outbox_notify AFTER INSERT ON corten_outbox
    FOR EACH STATEMENT EXECUTE FUNCTION corten_outbox_notify();
";

/// Creates the cluster's tables. Safe to run from every instance at once.
pub async fn migrate(pool: &PgPool) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('corten-cluster-migrate'))").execute(&mut *tx).await?;
    sqlx::raw_sql(SCHEMA).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Creates feed `key` if it does not exist, inside the caller's transaction (so it can insert the
/// feed's first rows in the same commit). Returns true when it was created.
pub async fn create_feed(conn: &mut PgConnection, key: &str) -> Result<bool, Error> {
    // The generation becomes the epoch in client cursors, which cannot contain '-'.
    let generation = format!("{:x}", now_ms());
    let created = sqlx::query("INSERT INTO corten_feeds (key, generation) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(key)
        .bind(generation)
        .execute(conn)
        .await?
        .rows_affected();
    Ok(created == 1)
}

/// A feed as of one consistent read: generation, sequence, rows, and the patches that led up to
/// them (oldest first, at most `recent`) for the replay log.
pub(crate) struct Loaded<T> {
    pub generation: String,
    pub seq: u64,
    pub rows: Vec<T>,
    pub recent: Vec<String>,
}

pub(crate) async fn load<S: Source>(
    pool: &PgPool,
    source: &S,
    key: &str,
    recent: usize,
) -> Result<Option<Loaded<S::Row>>, Error> {
    let mut tx = pool.begin().await?;
    // One snapshot for every read: the sequence, the rows and the recent patches agree.
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").execute(&mut *tx).await?;
    let feed: Option<(String, i64)> =
        sqlx::query_as("SELECT generation, seq FROM corten_feeds WHERE key = $1").bind(key).fetch_optional(&mut *tx).await?;
    let Some((generation, seq)) = feed else { return Ok(None) };
    let rows = source.rows(&mut tx, key).await?;
    let recent: Vec<String> =
        sqlx::query_scalar("SELECT body FROM corten_outbox WHERE feed = $1 AND generation = $2 AND seq > $3 ORDER BY seq")
            .bind(key)
            .bind(&generation)
            .bind(seq - recent as i64)
            .fetch_all(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok(Some(Loaded { generation, seq: seq as u64, rows, recent }))
}

/// The feed's last committed sequence number, `None` for an unknown feed.
pub async fn seq(pool: &PgPool, key: &str) -> Result<Option<u64>, Error> {
    let seq: Option<i64> =
        sqlx::query_scalar("SELECT seq FROM corten_feeds WHERE key = $1").bind(key).fetch_optional(pool).await?;
    Ok(seq.map(|s| s as u64))
}

/// What a committed write produced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Written {
    /// The patch's sequence number; `None` when the write changed no rows.
    pub seq: Option<u64>,
    /// When the commit returned (or, for a replay, when the original did), Unix ms.
    pub committed_at: u64,
    /// True when the idempotency key had already committed: nothing was written this time.
    pub replayed: bool,
}

/// The result of [`begin`].
pub enum Begin {
    /// Make the change with [`Write::conn`], then [`Write::commit`].
    New(Write),
    /// This idempotency key already committed; here is what it wrote. Answer as if it just did.
    Done(Written),
}

/// Starts a write to feed `key`. With an idempotency key (from the client, one per user intent),
/// a retry of a write that already committed gets [`Begin::Done`] instead of writing twice, and
/// a retry that races the original waits for it.
pub async fn begin(pool: &PgPool, key: &str, idempotency_key: Option<&str>) -> Result<Begin, Error> {
    let mut tx = pool.begin().await?;
    if let Some(idem) = idempotency_key {
        // A concurrent insert of the same key blocks here until the other transaction ends.
        let inserted = sqlx::query("INSERT INTO corten_idempotency (key, feed) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(idem)
            .bind(key)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if inserted == 0 {
            let (seq, committed_at): (Option<i64>, Option<i64>) =
                sqlx::query_as("SELECT seq, committed_at FROM corten_idempotency WHERE key = $1")
                    .bind(idem)
                    .fetch_one(&mut *tx)
                    .await?;
            tx.rollback().await?;
            let written = Written { seq: seq.map(|s| s as u64), committed_at: committed_at.unwrap_or(0) as u64, replayed: true };
            return Ok(Begin::Done(written));
        }
    }
    Ok(Begin::New(Write { tx, feed: key.to_string(), idempotency_key: idempotency_key.map(str::to_string) }))
}

/// An open write transaction for one feed.
pub struct Write {
    tx: Transaction<'static, Postgres>,
    feed: String,
    idempotency_key: Option<String>,
}

impl Write {
    /// The transaction, for the app's own statements. Lock rows in a fixed order (by id) so
    /// concurrent writers cannot deadlock.
    pub fn conn(&mut self) -> &mut PgConnection {
        &mut self.tx
    }

    /// Appends `changes` as the feed's next patch (when there are any) and commits. Taking the
    /// sequence number is the last statement: it locks the feed row until commit, so a feed's
    /// patches commit in sequence order, and the lock is held only for that statement and the
    /// commit.
    pub async fn commit<T: Row>(mut self, changes: &[Change<'_, T>]) -> Result<Written, Error> {
        let seq = if changes.is_empty() { None } else { Some(append(&mut self.tx, &self.feed, changes).await?) };
        let committed_at = now_ms();
        if let Some(idem) = &self.idempotency_key {
            sqlx::query("UPDATE corten_idempotency SET seq = $2, committed_at = $3 WHERE key = $1")
                .bind(idem)
                .bind(seq.map(|s| s as i64))
                .bind(committed_at as i64)
                .execute(&mut *self.tx)
                .await?;
        }
        self.tx.commit().await?;
        Ok(Written { seq, committed_at: now_ms(), replayed: false })
    }
}

/// Takes the feed's next sequence number and stores the patch in the outbox. The body is
/// `corten::feed::patch_json`'s exact shape with the sequence filled in by Postgres.
async fn append<T: Row>(conn: &mut PgConnection, feed: &str, changes: &[Change<'_, T>]) -> Result<u64, Error> {
    let changes = serde_json::to_string(changes)?;
    let seq: Option<i64> = sqlx::query_scalar(
        "WITH next AS (UPDATE corten_feeds SET seq = seq + 1 WHERE key = $1 RETURNING seq, generation),
              saved AS (
                INSERT INTO corten_outbox (feed, generation, seq, body)
                SELECT $1, generation, seq, '{\"seq\":' || seq || ',\"changes\":' || $2 || '}' FROM next
                RETURNING seq)
         SELECT seq FROM saved",
    )
    .bind(feed)
    .bind(changes)
    .fetch_optional(conn)
    .await?;
    seq.map(|s| s as u64).ok_or_else(|| format!("no feed named {feed}; create it with create_feed").into())
}

/// Unpublished outbox rows, oldest first.
pub(crate) struct Pending {
    pub id: i64,
    pub feed: String,
    pub generation: String,
    pub seq: i64,
    pub body: String,
}

pub(crate) async fn pending(pool: &PgPool, limit: i64) -> Result<Vec<Pending>, Error> {
    let rows: Vec<(i64, String, String, i64, String)> = sqlx::query_as(
        "SELECT id, feed, generation, seq, body FROM corten_outbox WHERE published_at IS NULL ORDER BY id LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id, feed, generation, seq, body)| Pending { id, feed, generation, seq, body }).collect())
}

pub(crate) async fn mark_published(pool: &PgPool, ids: &[i64]) -> Result<(), Error> {
    sqlx::query("UPDATE corten_outbox SET published_at = clock_timestamp() WHERE id = ANY($1)").bind(ids).execute(pool).await?;
    Ok(())
}

/// Takes or renews the relay lease on `conn` (a dedicated connection). The lease is free when it
/// expired or when the connection that holds it is gone, so a crashed relay is replaced as soon
/// as Postgres notices, and a frozen one (connection open, process stalled) after `lease`.
pub(crate) async fn lease(conn: &mut PgConnection, holder: &str, lease: Duration) -> Result<bool, Error> {
    let got = sqlx::query(
        "UPDATE corten_relay_lease
            SET holder = $1, holder_pid = pg_backend_pid(), expires_at = clock_timestamp() + make_interval(secs => $2)
          WHERE id = 1 AND (
                holder_pid = pg_backend_pid()
             OR expires_at < clock_timestamp()
             OR NOT EXISTS (SELECT 1 FROM pg_stat_activity WHERE pid = corten_relay_lease.holder_pid))",
    )
    .bind(holder)
    .bind(lease.as_secs_f64())
    .execute(conn)
    .await?
    .rows_affected();
    Ok(got == 1)
}

/// Releases the lease if `conn` holds it, so a clean shutdown hands over at once.
pub(crate) async fn release(conn: &mut PgConnection) -> Result<(), Error> {
    sqlx::query("UPDATE corten_relay_lease SET expires_at = '-infinity', holder_pid = NULL WHERE id = 1 AND holder_pid = pg_backend_pid()")
        .execute(conn)
        .await?;
    Ok(())
}

/// Deletes published outbox rows and idempotency keys older than `age`. Keeps every feed's last
/// `keep` patches, which a freshly started instance loads into its replay log.
pub async fn prune(pool: &PgPool, age: Duration, keep: u64) -> Result<u64, Error> {
    let secs = age.as_secs_f64();
    let outbox = sqlx::query(
        "DELETE FROM corten_outbox o USING corten_feeds f
          WHERE o.feed = f.key AND o.published_at IS NOT NULL
            AND o.created_at < clock_timestamp() - make_interval(secs => $1)
            AND (o.generation <> f.generation OR o.seq <= f.seq - $2)",
    )
    .bind(secs)
    .bind(keep as i64)
    .execute(pool)
    .await?
    .rows_affected();
    let keys = sqlx::query("DELETE FROM corten_idempotency WHERE created_at < clock_timestamp() - make_interval(secs => $1)")
        .bind(secs)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(outbox + keys)
}
