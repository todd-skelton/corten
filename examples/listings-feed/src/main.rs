//! One seller-listings feed served by several instances through corten-cluster. Writes commit to
//! Postgres with an outbox row; the relay holding the lease publishes them to NATS JetStream; every
//! instance applies them to its in-memory feed. Clients resume on any instance with the same cursor.
//! Env: INSTANCE (a), PORT (8080), DATABASE_URL, NATS_URL, SELLER (s1), ROWS (5000), LEASE_MS (2000).

mod listing;

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use corten::feed::{Change, Since};
use corten_cluster::{Begin, Cluster, Error, Options, Source};
use serde::Deserialize;
use sqlx::{postgres::PgPoolOptions, types::Json as Jsonb, PgConnection, PgPool};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::listing::Listing;

/// The app's rows: `listings`, keyed by seller (the feed key).
struct Listings;

impl Source for Listings {
    type Row = Listing;

    async fn rows(&self, conn: &mut PgConnection, key: &str) -> Result<Vec<Listing>, Error> {
        let rows: Vec<Jsonb<Listing>> =
            sqlx::query_scalar("SELECT data FROM listings WHERE seller = $1 ORDER BY id").bind(key).fetch_all(conn).await?;
        Ok(rows.into_iter().map(|j| j.0).collect())
    }
}

#[derive(Clone)]
struct App {
    cluster: Cluster<Listings>,
    pool: PgPool,
    instance: Arc<str>,
    writes: Arc<AtomicU64>,
    rows: usize,
}

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let instance = env("INSTANCE", "a");
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

    let app = App { cluster, pool, instance: instance.as_str().into(), writes: Arc::new(AtomicU64::new(1)), rows };
    let router =
        Router::new()
            .route(
                "/feeds/{key}",
                get(|State(app): State<App>, Path(key): Path<String>| async move { app.cluster.snapshot(&key).await }),
            )
            .route(
                "/feeds/{key}/live",
                get(|State(app): State<App>, Path(key): Path<String>, since: Since| async move {
                    app.cluster.live(&key, since).await
                }),
            )
            .route("/feeds/{key}/market", post(market))
            .route("/feeds/{key}/seq", get(committed_seq))
            .route("/stats", get(stats))
            .route("/health", get(|| async { "ok" }))
            .route("/probe", get(|| async { Html(PROBE) }))
            .merge(corten::routes())
            .layer(SetResponseHeaderLayer::overriding(HeaderName::from_static("x-instance"), HeaderValue::from_str(&instance)?))
            .with_state(app);
    let port = env("PORT", "8080");
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    eprintln!("{instance}: listening on {port}");
    axum::serve(listener, router).await?;
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
        let data = serde_json::to_value(listing::generate(rows, corten_now()))?;
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

fn corten_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn unavailable(e: Error) -> Response {
    eprintln!("request failed: {e}");
    (StatusCode::SERVICE_UNAVAILABLE, "Unavailable.").into_response()
}

#[derive(Deserialize)]
struct MarketQuery {
    n: Option<usize>,
}

/// Benchmark writer: `n` listings get a new market low, committed as one patch. Send an
/// `Idempotency-Key` header and a retry of the same request commits at most once.
async fn market(State(app): State<App>, Path(key): Path<String>, Query(q): Query<MarketQuery>, headers: HeaderMap) -> Response {
    let idem = headers.get("idempotency-key").and_then(|v| v.to_str().ok());
    let roll = app.writes.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ corten_now();
    match write_market(&app, &key, q.n.unwrap_or(5).min(100), roll, idem).await {
        Ok(written) => Json(written).into_response(),
        Err(e) => unavailable(e),
    }
}

async fn write_market(app: &App, key: &str, n: usize, roll: u64, idem: Option<&str>) -> Result<serde_json::Value, Error> {
    let mut rng = listing::Rng::new(roll);
    let mut ids: Vec<String> = (0..n.max(1)).map(|_| listing::id(1 + rng.below(app.rows))).collect();
    ids.sort_unstable();
    ids.dedup();
    let mut write = match corten_cluster::begin(&app.pool, key, idem).await? {
        Begin::Done(w) => {
            return Ok(serde_json::json!({ "seq": w.seq, "committed_at": w.committed_at, "replayed": true, "at": 0 }))
        }
        Begin::New(write) => write,
    };
    // Lock the rows first, always in id order (no deadlocks between writers).
    let current: Vec<Jsonb<Listing>> =
        sqlx::query_scalar("SELECT data FROM listings WHERE seller = $1 AND id = ANY($2) ORDER BY id FOR UPDATE")
            .bind(key)
            .bind(&ids)
            .fetch_all(write.conn())
            .await?;
    let at = corten_now();
    let rows: Vec<Listing> = current
        .into_iter()
        .map(|j| {
            let mut row = j.0;
            row.market_cents = listing::drift(row.market_cents, rng.next());
            row.updated_at = at;
            row
        })
        .collect();
    sqlx::query("UPDATE listings l SET data = e FROM jsonb_array_elements($2) e WHERE l.seller = $1 AND l.id = e->>'id'")
        .bind(key)
        .bind(serde_json::to_value(&rows)?)
        .execute(write.conn())
        .await?;
    let changes: Vec<Change<'_, Listing>> = rows.iter().map(|r| Change::Upsert { id: &r.id, value: r }).collect();
    let w = write.commit(&changes).await?;
    Ok(serde_json::json!({ "seq": w.seq, "committed_at": w.committed_at, "replayed": false, "at": at }))
}

/// The sequence committed in Postgres (the benchmark's source of truth for lost patches).
async fn committed_seq(State(app): State<App>, Path(key): Path<String>) -> Response {
    match corten_cluster::seq(&app.pool, &key).await {
        Ok(Some(seq)) => Json(seq).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => unavailable(e),
    }
}

async fn stats(State(app): State<App>) -> Json<serde_json::Value> {
    let feeds: serde_json::Map<String, serde_json::Value> = app
        .cluster
        .stats()
        .into_iter()
        .map(|(k, seq, subs)| (k, serde_json::json!({ "seq": seq, "subscribers": subs })))
        .collect();
    Json(serde_json::json!({ "instance": &*app.instance, "relay_active": app.cluster.relay_active(), "feeds": feeds }))
}

/// A page that follows feed `s1` with corten.js `subscribe()`, for the browser failover check.
const PROBE: &str = r#"<!doctype html><meta charset="utf-8"><title>feed probe</title>
<p id="out">starting</p>
<script type="module">
import { subscribe } from "/corten/corten.js";
const s = (window.__probe = { snapshots: 0, patches: 0, gaps: 0, first: 0, last: 0, status: "" });
subscribe("/feeds/s1", {
  snapshot(rows, seq) { s.snapshots += 1; if (!s.first) s.first = seq; s.last = seq; },
  patch(changes, seq) { if (seq !== s.last + 1) s.gaps += 1; s.patches += 1; s.last = seq; },
  status(x) { s.status = x; document.getElementById("out").textContent = x; },
});
</script>"#;
