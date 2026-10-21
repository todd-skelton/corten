//! Outbox relay: publishes committed patches to JetStream in outbox order. Every instance runs a
//! candidate; the one holding the lease (store::lease) publishes. A patch is marked published only
//! after JetStream acknowledged it. A relay that dies or loses its lease in between leaves it to
//! the next one, which publishes it again, and JetStream drops the copy by its message id. Two
//! relays briefly overlapping is therefore harmless: each publishes the pending rows in order, and
//! the copies are dropped.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use async_nats::{jetstream, HeaderMap};
use sqlx::{postgres::PgListener, PgConnection, PgPool};

use crate::{store, Error, Options};

/// Outbox rows published per round trip.
const BATCH: i64 = 256;

pub(crate) fn subject(options: &Options, feed: &str) -> String {
    format!("{}.{feed}", options.subject_prefix)
}

/// Runs forever: wait for the lease, publish while holding it, then start over.
pub(crate) async fn run(pool: PgPool, js: jetstream::Context, options: Arc<Options>, active: Arc<AtomicBool>) {
    loop {
        if let Err(e) = lead(&pool, &js, &options, &active).await {
            eprintln!("relay {}: {e}; standing by", options.instance);
        }
        active.store(false, Ordering::Relaxed);
        tokio::time::sleep(options.lease / 4).await;
    }
}

/// The lease as held: renewed at `renewed`, which must stay within `stale` to keep publishing.
struct Held {
    conn: PgConnection,
    renewed: Instant,
    stale: Duration,
}

impl Held {
    /// Renews the lease; an error means another instance may now hold it.
    async fn renew(&mut self, options: &Options) -> Result<(), Error> {
        let started = Instant::now();
        let renewed = tokio::time::timeout(self.stale, store::lease(&mut self.conn, &options.instance, options.lease)).await;
        match renewed {
            Ok(Ok(true)) => {
                self.renewed = started;
                Ok(())
            }
            Ok(Ok(false)) => Err("lost the relay lease".into()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("renewing the relay lease timed out".into()),
        }
    }

    fn check(&self) -> Result<(), Error> {
        if self.renewed.elapsed() > self.stale {
            return Err("relay lease is stale".into());
        }
        Ok(())
    }
}

async fn lead(pool: &PgPool, js: &jetstream::Context, options: &Options, active: &AtomicBool) -> Result<(), Error> {
    // A dedicated connection outside the pool: the lease names its backend pid.
    let mut conn = pool.acquire().await?.detach();
    while !store::lease(&mut conn, &options.instance, options.lease).await? {
        tokio::time::sleep(options.lease / 4).await;
    }
    let mut held = Held { conn, renewed: Instant::now(), stale: options.lease * 3 / 4 };
    active.store(true, Ordering::Relaxed);
    eprintln!("relay {}: active", options.instance);
    let result = publish(pool, js, options, &mut held).await;
    active.store(false, Ordering::Relaxed);
    let _ = store::release(&mut held.conn).await;
    result
}

async fn publish(pool: &PgPool, js: &jetstream::Context, options: &Options, held: &mut Held) -> Result<(), Error> {
    let mut wake = PgListener::connect_with(pool).await?;
    wake.listen("corten_outbox").await?;
    loop {
        drain(pool, js, options, held).await?;
        tokio::select! {
            n = wake.recv() => { n?; }
            // Renew on time even when idle; also a safety net for a lost notification.
            _ = tokio::time::sleep(options.lease / 4) => {}
        }
        if held.renewed.elapsed() >= options.lease / 4 {
            held.renew(options).await?;
        }
    }
}

/// Publishes every pending outbox row, in order, pipelining up to `BATCH` acknowledgements.
async fn drain(pool: &PgPool, js: &jetstream::Context, options: &Options, held: &mut Held) -> Result<(), Error> {
    loop {
        if held.renewed.elapsed() >= options.lease / 4 {
            held.renew(options).await?;
        }
        let rows = store::pending(pool, BATCH).await?;
        if rows.is_empty() {
            return Ok(());
        }
        held.check()?;
        let mut acks = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut headers = HeaderMap::new();
            headers.insert("Nats-Msg-Id", format!("{}:{}:{}", row.feed, row.generation, row.seq).as_str());
            headers.insert("Feed-Generation", row.generation.as_str());
            acks.push(js.publish_with_headers(subject(options, &row.feed), headers, row.body.clone().into()).await?);
        }
        for ack in acks {
            ack.await?;
        }
        let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        store::mark_published(pool, &ids).await?;
        if (rows.len() as i64) < BATCH {
            return Ok(());
        }
    }
}
