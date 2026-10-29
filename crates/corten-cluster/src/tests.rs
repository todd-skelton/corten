//! Integration tests against real Postgres and NATS (JetStream). They run when
//! `CORTEN_TEST_DATABASE_URL` and `CORTEN_TEST_NATS_URL` are set and are skipped otherwise; see
//! CONTRIBUTING.md for the containers. Each test uses its own feed keys, so they can share a database,
//! but the relay lease is one row, so run them with `--test-threads=1`.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;

use corten::feed::Change;

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Item {
    id: String,
    n: i64,
}

impl Row for Item {
    fn id(&self) -> &str {
        &self.id
    }
}

/// Rows live in `test_items` (feed key, id, n).
struct Items;

impl Source for Items {
    type Row = Item;

    async fn rows(&self, conn: &mut PgConnection, key: &str) -> Result<Vec<Item>, Error> {
        let rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT id, n FROM test_items WHERE feed = $1 ORDER BY id").bind(key).fetch_all(conn).await?;
        Ok(rows.into_iter().map(|(id, n)| Item { id, n }).collect())
    }
}

fn urls() -> Option<(String, String)> {
    Some((std::env::var("CORTEN_TEST_DATABASE_URL").ok()?, std::env::var("CORTEN_TEST_NATS_URL").ok()?))
}

async fn pool(url: &str) -> PgPool {
    let pool = PgPoolOptions::new().max_connections(8).connect(url).await.unwrap();
    migrate(&pool).await.unwrap();
    sqlx::query("CREATE TABLE IF NOT EXISTS test_items (feed text, id text, n bigint, PRIMARY KEY (feed, id))")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

fn key(name: &str) -> String {
    format!("{name}{:x}", now_ms())
}

async fn new_feed(pool: &PgPool, key: &str) {
    let mut tx = pool.begin().await.unwrap();
    assert!(create_feed(&mut tx, key).await.unwrap());
    sqlx::query("INSERT INTO test_items VALUES ($1, 'a', 0)").bind(key).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

/// Sets item `a` of feed `key` to `n` as one patch, with an optional idempotency key.
async fn set(pool: &PgPool, key: &str, n: i64, idem: Option<&str>) -> Written {
    let mut write = match begin(pool, key, idem).await.unwrap() {
        Begin::Done(written) => return written,
        Begin::New(write) => write,
    };
    sqlx::query("UPDATE test_items SET n = $2 WHERE feed = $1 AND id = 'a'")
        .bind(key)
        .bind(n)
        .execute(write.conn())
        .await
        .unwrap();
    let item = Item { id: "a".into(), n };
    write.commit(&[Change::Upsert { id: "a", value: &item }]).await.unwrap()
}

async fn eventually(what: &str, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !ok() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn take(conn: &mut PgConnection, who: &str, lease: Duration) -> bool {
    store::lease(conn, who, lease).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_idempotency_key_commits_once_even_when_retries_race() {
    let Some((db, _)) = urls() else { return eprintln!("skipped: set CORTEN_TEST_DATABASE_URL and CORTEN_TEST_NATS_URL") };
    let pool = pool(&db).await;
    let feed = key("idem");
    new_feed(&pool, &feed).await;
    let idem = format!("{feed}-k1");
    let first = set(&pool, &feed, 1, Some(&idem)).await;
    assert_eq!((first.seq, first.replayed), (Some(1), false));
    let retry = set(&pool, &feed, 99, Some(&idem)).await;
    assert_eq!((retry.seq, retry.replayed), (Some(1), true), "the retry is answered from the key");
    // Eight concurrent attempts with one new key: exactly one writes.
    let idem2 = format!("{feed}-k2");
    let attempts = (0..8).map(|n| {
        let (pool, feed, idem2) = (pool.clone(), feed.clone(), idem2.clone());
        tokio::spawn(async move { set(&pool, &feed, 10 + n, Some(&idem2)).await })
    });
    let results: Vec<Written> = futures_util::future::join_all(attempts).await.into_iter().map(Result::unwrap).collect();
    assert_eq!(results.iter().filter(|w| !w.replayed).count(), 1);
    assert!(results.iter().all(|w| w.seq == Some(2)));
    assert_eq!(seq(&pool, &feed).await.unwrap(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn two_instances_converge_and_a_late_one_backfills_its_replay_log() {
    let Some((db, nats)) = urls() else { return eprintln!("skipped: set CORTEN_TEST_DATABASE_URL and CORTEN_TEST_NATS_URL") };
    let pool = pool(&db).await;
    let feed = key("conv");
    new_feed(&pool, &feed).await;
    let options = |instance: &str| Options { instance: instance.into(), lease: Duration::from_millis(600), ..Options::default() };
    let a = Cluster::start(pool.clone(), async_nats::connect(&nats).await.unwrap(), Items, options("a")).await.unwrap();
    let b = Cluster::start(pool.clone(), async_nats::connect(&nats).await.unwrap(), Items, options("b")).await.unwrap();
    let (fa, fb) = (a.feed(&feed).await.unwrap().unwrap(), b.feed(&feed).await.unwrap().unwrap());
    assert!(a.feed("no-such-feed").await.unwrap().is_none());
    for n in 1..=20 {
        set(&pool, &feed, n, None).await;
    }
    eventually("both instances at seq 20", || fa.seq() == 20 && fb.seq() == 20).await;
    assert_eq!(fa.read(|r| r["a"].n), 20);
    assert_eq!(fb.read(|r| r["a"].n), 20);
    eventually("one relay", || a.relay_active() != b.relay_active()).await;
    // An instance that loads the feed now (it just started) resumes a cursor from seq 15.
    let c = Cluster::start(pool.clone(), async_nats::connect(&nats).await.unwrap(), Items, options("c")).await.unwrap();
    let fc = c.feed(&feed).await.unwrap().unwrap();
    assert_eq!(fc.seq(), 20);
    let cursor = Since(Some(corten::feed::Cursor::new(fc.generation(), 15)));
    let body = fc.live(cursor).into_body();
    let mut stream = body.into_data_stream();
    let first = futures_util::StreamExt::next(&mut stream).await.unwrap().unwrap();
    let text = String::from_utf8(first.to_vec()).unwrap();
    assert!(text.contains(&format!("id: {}-16\n", fc.generation())), "replayed from the outbox: {text}");
    assert!(!text.contains("event: sync"), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_relay_lease_moves_on_when_its_holder_dies_or_stalls() {
    let Some((db, _)) = urls() else { return eprintln!("skipped: set CORTEN_TEST_DATABASE_URL and CORTEN_TEST_NATS_URL") };
    let pool = pool(&db).await;
    let lease = Duration::from_millis(400);
    let mut x = pool.acquire().await.unwrap().detach();
    let mut y = pool.acquire().await.unwrap().detach();
    let end = Instant::now() + Duration::from_secs(10);
    while !take(&mut x, "x", lease).await {
        assert!(Instant::now() < end, "lease never became free");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!take(&mut y, "y", lease).await, "held and fresh");
    assert!(take(&mut x, "x", lease).await, "the holder renews");
    // Stalled: x stops renewing but keeps its connection. y gets the lease after it expires.
    let stalled = Instant::now();
    while !take(&mut y, "y", lease).await {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(stalled.elapsed() >= lease - Duration::from_millis(50), "not before expiry: {:?}", stalled.elapsed());
    assert!(!take(&mut x, "x", lease).await, "the stalled holder cannot renew after losing it");
    // Dead: y's connection closes. x gets the lease at once, long before expiry.
    assert!(take(&mut y, "y", lease).await);
    sqlx::Connection::close(y).await.unwrap();
    let died = Instant::now();
    while !take(&mut x, "x", lease).await {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(died.elapsed() < lease, "taken after the holder died: {:?}", died.elapsed());
    store::release(&mut x).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unused_feed_is_unloaded_and_loads_again_with_its_replay_log() {
    let Some((db, nats)) = urls() else { return eprintln!("skipped: set CORTEN_TEST_DATABASE_URL and CORTEN_TEST_NATS_URL") };
    let pool = pool(&db).await;
    let feed = key("idle");
    new_feed(&pool, &feed).await;
    let options = Options { instance: "idle".into(), idle_unload: Some(Duration::from_millis(200)), ..Options::default() };
    let cluster = Cluster::start(pool.clone(), async_nats::connect(&nats).await.unwrap(), Items, options).await.unwrap();
    let loaded = cluster.feed(&feed).await.unwrap().unwrap();
    let generation = loaded.generation().to_string();

    // A live stream keeps it loaded: a handle alone does not, so without the stream the feed
    // could be unloaded (and stop following) before the relay publishes the patches.
    let stream = loaded.live(Since(Some(corten::feed::Cursor::new(&generation, 0))));
    for n in 1..=3 {
        set(&pool, &feed, n, None).await;
    }
    eventually("seq 3", || loaded.seq() == 3).await;
    drop(loaded);
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(cluster.stats().iter().any(|(k, _, subscribers)| k == &feed && *subscribers == 1), "kept while streamed");

    // Without one it goes, and the next request loads it with the patches a cursor needs.
    drop(stream);
    eventually("unloaded", || !cluster.stats().iter().any(|(k, _, _)| k == &feed)).await;
    let again = cluster.feed(&feed).await.unwrap().unwrap();
    assert_eq!((again.seq(), again.generation()), (3, generation.as_str()));
    let body = again.live(Since(Some(corten::feed::Cursor::new(&generation, 1)))).into_body();
    let first = futures_util::StreamExt::next(&mut body.into_data_stream()).await.unwrap().unwrap();
    let text = String::from_utf8(first.to_vec()).unwrap();
    assert!(text.contains(&format!("id: {generation}-2\n")) && !text.contains("event: sync"), "{text}");
}

/// The feed's routes as an app serves them.
#[corten::routes(state = Cluster<Items>)]
mod url {
    #[route("/feeds/{key}", get = feed_snapshot)]
    pub struct FeedUrl<'a> {
        pub key: &'a str,
    }
}

async fn feed_snapshot(
    corten::prelude::State(cluster): corten::prelude::State<Cluster<Items>>,
    corten::Params(feed): corten::Params<url::FeedUrl<'_>>,
) -> corten::Result<FeedResponse> {
    cluster.snapshot(&feed.key).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_feed_route_answers_a_snapshot_and_json_404_for_a_feed_that_does_not_exist() {
    use tower::ServiceExt;
    let Some((db, nats)) = urls() else { return eprintln!("skipped: set CORTEN_TEST_DATABASE_URL and CORTEN_TEST_NATS_URL") };
    let pool = pool(&db).await;
    let feed = key("route");
    new_feed(&pool, &feed).await;
    let options = Options { instance: "route".into(), ..Options::default() };
    let cluster = Cluster::start(pool, async_nats::connect(&nats).await.unwrap(), Items, options).await.unwrap();
    let get = |path: String| {
        let app = corten::App::new(url::router(cluster.clone())).router();
        async move {
            let req = corten::axum::http::Request::get(path).body(corten::axum::body::Body::empty()).unwrap();
            let res = app.oneshot(req).await.unwrap();
            let status = res.status();
            let kind = res.headers()["content-type"].to_str().unwrap().to_string();
            let body = corten::axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
            (status, kind, String::from_utf8(body.to_vec()).unwrap())
        }
    };
    let (status, _, body) = get(format!("/feeds/{feed}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#""id":"a""#), "{body}");
    // A browser asked too (no Accept for JSON): a feed's route answers its failures as JSON.
    let (status, kind, body) = get(format!("/feeds/{}", key("missing"))).await;
    assert_eq!((status, kind.as_str()), (StatusCode::NOT_FOUND, "application/json"));
    assert!(body.contains(r#""status":404"#) && body.contains("no feed missing"), "{body}");
}
