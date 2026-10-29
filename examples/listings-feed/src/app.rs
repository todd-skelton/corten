//! The app every instance serves: a seller's listings page that follows the feed live, the feed
//! itself, and the JSON routes the failover benchmark writes and reads through.
//!
//! | Route | Piece |
//! |---|---|
//! | `GET /sellers/{seller}` | a page whose script follows the seller's feed (`subscribe`) and shows what arrived: kill the instance it streams from and it resumes on the other with no change missed; an unknown seller is the 404 page |
//! | `GET /feeds/{seller}`, `/feeds/{seller}/live` | the seller's listings as a feed (`corten-cluster`): any instance answers, and a client resumes on any of them with the same cursor |
//! | `POST /api/feeds/{seller}/market` | a write committed with an outbox row in one transaction, at most once per `Idempotency-Key` |
//! | `GET /api/feeds/{seller}/seq` | the sequence Postgres committed, what every client must reach |
//! | `GET /api/stats` | this instance: the feeds it holds, their subscribers, whether it runs the relay |
//!
//! A route's response names the instance that answered (`X-Instance`, a middleware of the app's),
//! and `/health/ready` is false while Postgres is out of reach or slow to answer.
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use corten::{
    Error,
    axum::{
        extract::Request,
        http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
        response::Response,
    },
    feed::{Change, FeedResponse, Since},
    prelude::*,
};
use corten_cluster::{Begin, Cluster, Source};
use sqlx::{PgConnection, PgPool, types::Json as Jsonb};

use crate::{
    feed_monitor::{self, FeedMonitor},
    listing::{self, Listing},
};

/// The app's rows: `listings`, keyed by seller (the feed key).
pub struct Listings;

impl Source for Listings {
    type Row = Listing;

    async fn rows(&self, conn: &mut PgConnection, key: &str) -> Result<Vec<Listing>, corten_cluster::Error> {
        let rows: Vec<Jsonb<Listing>> =
            sqlx::query_scalar("SELECT data FROM listings WHERE seller = $1 ORDER BY id").bind(key).fetch_all(conn).await?;
        Ok(rows.into_iter().map(|j| j.0).collect())
    }
}

/// What the handlers share. A handler takes the part it needs (`State<Cluster<Listings>>`).
#[derive(Clone, FromRef)]
pub struct Shared {
    pub cluster: Cluster<Listings>,
    pub pool: PgPool,
    pub instance: Instance,
    pub market: Market,
}

/// This instance's name (`INSTANCE`), and the same as a header value.
#[derive(Clone)]
pub struct Instance {
    pub name: Arc<str>,
    pub header: HeaderValue,
}

/// The benchmark's writer: how many listings the seller has, and a count that varies each write.
#[derive(Clone)]
pub struct Market {
    pub rows: usize,
    pub writes: Arc<AtomicU64>,
}

pub fn app(shared: Shared) -> App<InstanceHeader> {
    let header = InstanceHeader(shared.instance.header.clone());
    let pool = shared.pool.clone();
    App::new(url::router(shared.clone()))
        .api(|routes| api::api(routes, shared))
        .components(&[FeedMonitor])
        // Not ready while Postgres is out of reach, or busy past a second: a load balancer that
        // checks it sends this instance no traffic until then.
        .ready(move || {
            let pool = pool.clone();
            async move {
                let ping = sqlx::query("SELECT 1").execute(&pool);
                matches!(tokio::time::timeout(Duration::from_secs(1), ping).await, Ok(Ok(_)))
            }
        })
        .middleware(header)
}

/// The pages and the feeds, which a browser reads.
#[corten::routes(state = Shared)]
pub mod url {
    #[route("/sellers/{seller}", get = page)]
    pub struct Seller<'a> {
        pub seller: &'a str,
    }

    #[route("/feeds/{seller}", get = snapshot)]
    pub struct Listings<'a> {
        pub seller: &'a str,
    }

    #[route("/feeds/{seller}/live", get = live)]
    pub struct ListingsLive<'a> {
        pub seller: &'a str,
    }
}

/// The routes other programs call: JSON only, failures included.
#[corten::routes(api = ApiRoot, state = Shared)]
pub mod api {
    #[route("/api")]
    pub struct ApiRoot;

    /// `n` listings get a new market low (`?n=5`, 1 to 100), committed as one patch.
    #[route("/api/feeds/{seller}/market", post = market)]
    pub struct MarketWrite<'a> {
        pub seller: &'a str,
        pub n: Option<usize>,
    }

    #[route("/api/feeds/{seller}/seq", get = committed_seq)]
    pub struct Seq<'a> {
        pub seller: &'a str,
    }

    #[route("/api/stats", get = stats)]
    pub struct Stats;
}

/// Names the instance that answered (`X-Instance`), so a client can tell where its stream landed.
pub struct InstanceHeader(HeaderValue);

const X_INSTANCE: HeaderName = HeaderName::from_static("x-instance");

impl Middleware for InstanceHeader {
    async fn call(&self, req: Request, next: impl Next) -> Response {
        let mut res = next.run(req).await;
        res.headers_mut().insert(X_INSTANCE, self.0.clone());
        res
    }
}

/// The seller's page. A seller with no feed is the 404 page, rather than a page whose script asks
/// for a feed that is not there; Postgres out of reach is the 503 page.
async fn page(req: Req, State(cluster): State<Cluster<Listings>>, Params(at): Params<url::Seller<'_>>) -> HtmlResult {
    cluster.feed(&at.seller).await.map_err(unavailable)?.ok_or_else(Error::not_found)?;
    req.page(feed_monitor::page(&at.seller))
}

/// The seller's listings: every row and the cursor to follow from. Any instance answers; an
/// unknown seller is a 404 and Postgres out of reach a 503, both as JSON.
async fn snapshot(
    State(cluster): State<Cluster<Listings>>,
    Params(at): Params<url::Listings<'_>>,
) -> corten::Result<FeedResponse> {
    cluster.snapshot(&at.seller).await
}

async fn live(
    State(cluster): State<Cluster<Listings>>,
    Params(at): Params<url::ListingsLive<'_>>,
    since: Since,
) -> corten::Result<FeedResponse> {
    cluster.live(&at.seller, since).await
}

/// What a market write answers: the patch's sequence and when it committed, Unix ms. `replayed`
/// is true when the idempotency key had committed before, so nothing was written this time.
#[derive(ToJson)]
struct Written {
    seq: Option<u64>,
    committed_at: u64,
    replayed: bool,
}

/// `n` listings get a new market low, committed as one patch. Send an `Idempotency-Key` header and
/// a retry of the same request commits at most once.
async fn market(
    State(pool): State<PgPool>,
    State(market): State<Market>,
    Params(at): Params<api::MarketWrite<'_>>,
    headers: HeaderMap,
) -> JsonResult<Written> {
    let key = headers.get("idempotency-key").and_then(|v| v.to_str().ok());
    let roll = market.writes.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ listing::now();
    let n = at.n.unwrap_or(5).clamp(1, 100);
    Ok(Json(write_market(&pool, &market, &at.seller, n, roll, key).await.map_err(unavailable)?))
}

async fn write_market(
    pool: &PgPool,
    market: &Market,
    seller: &str,
    n: usize,
    roll: u64,
    key: Option<&str>,
) -> Result<Written, corten_cluster::Error> {
    let mut rng = listing::Rng::new(roll);
    let mut ids: Vec<String> = (0..n).map(|_| listing::id(1 + rng.below(market.rows))).collect();
    ids.sort_unstable();
    ids.dedup();
    let mut write = match corten_cluster::begin(pool, seller, key).await? {
        Begin::Done(w) => return Ok(Written { seq: w.seq, committed_at: w.committed_at, replayed: true }),
        Begin::New(write) => write,
    };
    // Lock the rows first, always in id order (no deadlocks between writers).
    let current: Vec<Jsonb<Listing>> =
        sqlx::query_scalar("SELECT data FROM listings WHERE seller = $1 AND id = ANY($2) ORDER BY id FOR UPDATE")
            .bind(seller)
            .bind(&ids)
            .fetch_all(write.conn())
            .await?;
    let at = listing::now();
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
        .bind(seller)
        .bind(serde_json::to_value(&rows)?)
        .execute(write.conn())
        .await?;
    let changes: Vec<Change<'_, Listing>> = rows.iter().map(|r| Change::Upsert { id: &r.id, value: r }).collect();
    let w = write.commit(&changes).await?;
    Ok(Written { seq: w.seq, committed_at: w.committed_at, replayed: false })
}

/// The sequence committed in Postgres: the benchmark's source of truth for lost patches.
async fn committed_seq(State(pool): State<PgPool>, Params(at): Params<api::Seq<'_>>) -> JsonResult<u64> {
    let seller = at.seller;
    let seq = corten_cluster::seq(&pool, &seller).await.map_err(unavailable)?;
    Ok(Json(seq.ok_or_else(|| Error::new(format!("no seller {seller}")).with_status(StatusCode::NOT_FOUND))?))
}

#[derive(ToJson)]
struct InstanceStats {
    instance: Arc<str>,
    relay_active: bool,
    feeds: BTreeMap<String, FeedStats>,
}

#[derive(ToJson)]
struct FeedStats {
    seq: u64,
    subscribers: usize,
}

async fn stats(State(cluster): State<Cluster<Listings>>, State(instance): State<Instance>) -> JsonResult<InstanceStats> {
    let feeds = cluster.stats().into_iter().map(|(key, seq, subscribers)| (key, FeedStats { seq, subscribers })).collect();
    Ok(Json(InstanceStats { instance: instance.name, relay_active: cluster.relay_active(), feeds }))
}

/// A database or JetStream failure: a 503, its cause in the log.
fn unavailable(e: corten_cluster::Error) -> Error {
    Error::new(e).with_status(StatusCode::SERVICE_UNAVAILABLE)
}
