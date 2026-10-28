//! One seller-listings feed served by several instances through corten-cluster. Writes commit to
//! Postgres with an outbox row; the relay holding the lease publishes them to NATS JetStream; every
//! instance applies them to its in-memory feed. Clients resume on any instance with the same cursor.
//! The routes are in `app.rs`.
//! Env: INSTANCE (a), HOST and PORT (as `corten::serve` reads them), DATABASE_URL, NATS_URL,
//! SELLER (s1), ROWS (5000), LEASE_MS (2000).

mod app;
mod feed_monitor;
mod listing;

use std::{
    sync::{Arc, atomic::AtomicU64},
    time::Duration,
};

use corten::axum::http::HeaderValue;
use corten_cluster::{Cluster, Error, Options};
use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::app::{Instance, Listings, Market, Shared};

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let instance = env("INSTANCE", "a");
    let header = HeaderValue::from_str(&instance)?;
    let db = env("DATABASE_URL", "postgres://postgres:postgres@localhost/feeds");
    let nats_url = env("NATS_URL", "nats://localhost:4222");
    let seller = env("SELLER", "s1");
    let rows: usize = env("ROWS", "5000").parse()?;
    let lease = Duration::from_millis(env("LEASE_MS", "2000").parse()?);

    let pool = retry("postgres", || async { Ok(PgPoolOptions::new().max_connections(16).connect(&db).await?) }).await;
    corten_cluster::migrate(&pool).await?;
    seed(&pool, &seller, rows).await?;
    let nats = retry("nats", || async { Ok(async_nats::connect(&nats_url).await?) }).await;
    let options = Options { instance: instance.clone(), lease, ..Options::default() };
    let cluster = Cluster::start(pool.clone(), nats, Listings, options).await?;

    let market = Market { rows, writes: Arc::new(AtomicU64::new(1)) };
    corten::serve(app::app(Shared { cluster, pool, instance: Instance { name: instance.into(), header }, market })).await?;
    Ok(())
}

/// Creates the listings table and, once per database, seller `key` with `rows` sample listings.
async fn seed(pool: &PgPool, key: &str, rows: usize) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('listings-feed-seed'))").execute(&mut *tx).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS listings (seller text NOT NULL, id text NOT NULL, data jsonb NOT NULL, PRIMARY KEY (seller, id))")
        .execute(&mut *tx)
        .await?;
    if corten_cluster::create_feed(&mut tx, key).await? {
        let data = serde_json::to_value(listing::generate(rows, listing::now()))?;
        sqlx::query("INSERT INTO listings (seller, id, data) SELECT $1, e->>'id', e FROM jsonb_array_elements($2) e")
            .bind(key)
            .bind(data)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn retry<T, F, Fut>(what: &str, mut connect: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, Error>>,
{
    loop {
        match connect().await {
            Ok(v) => return v,
            Err(e) => {
                eprintln!("{what}: {e}; retrying");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}
