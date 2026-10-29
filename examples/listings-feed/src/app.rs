//! The app every instance serves: a seller's listings page that follows the feed live, the feed
//! itself, and the JSON routes the failover benchmark writes and reads through.
//!
//! | Route | Piece |
//! |---|---|
//! | `GET /sellers/{seller}` | the seller's listings, rendered from this instance's feed and kept current by a script that follows it (`subscribe`), with what arrived: kill the instance it streams from and it resumes on the other with no change missed; an unknown seller is the 404 page |
//! | `POST /sellers/{seller}/market` | the page's form: five market lows move, committed as `/api/feeds/{seller}/market` commits; the rows change when the feed brings it |
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

use serde::Deserialize;

use crate::{
    listing::{self, Listing},
    listing_row::ListingRow,
    listing_table::{Line, ListingTable, SHOWN},
    page::{self, MoveStatus},
    text::Msg,
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
        .assets(page::Static)
        .components(&[ListingTable, ListingRow])
        .messages::<Msg>()
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
    /// `after` is a write the page should show: the form's answer without JavaScript.
    #[route("/sellers/{seller}", get = page)]
    pub struct Seller<'a> {
        pub seller: &'a str,
        pub after: Option<u64>,
    }

    #[route("/sellers/{seller}/market", post = move_market)]
    pub struct Move<'a> {
        pub seller: &'a str,
    }

    #[route("/feeds/{seller}", get = snapshot, rows = Listing)]
    pub struct Listings<'a> {
        pub seller: &'a str,
    }

    #[route("/feeds/{seller}/live", get = live, rows = Listing)]
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

/// The seller's page, its first listings rendered from this instance's copy of the feed. A seller
/// with no feed is the 404 page, rather than a page whose script asks for a feed that is not
/// there; Postgres out of reach is the 503 page. Asked for a write the feed has not brought yet
/// (`after`), it asks a browser without JavaScript to read it again; the script follows the feed.
async fn page(
    req: Req,
    State(cluster): State<Cluster<Listings>>,
    State(instance): State<Instance>,
    Params(at): Params<url::Seller<'_>>,
) -> HtmlResult {
    let feed = cluster.feed(&at.seller).await.map_err(unavailable)?.ok_or_else(Error::not_found)?;
    let (shown, total): (Vec<Listing>, usize) = feed.read(|rows| (rows.values().take(SHOWN).cloned().collect(), rows.len()));
    let seq = feed.seq();
    let t = req.t();
    let lines: Vec<Line> = shown.iter().map(|listing| Line::new(t, listing)).collect();
    let behind = at.after.is_some_and(|after| seq < after);
    req.page(page::page(&req, &at.seller, &instance.name, &lines, total, behind, t))
}

#[derive(Deserialize, Fields)]
pub struct MoveInput {
    /// This submit, so one sent again (its answer lost on the way) commits once: it is the
    /// write's idempotency key.
    pub submit: SubmitKey,
}

/// The page's form: five market lows move, in one transaction with the feed's outbox row. The
/// answer only says so; the rows change when the feed brings the change, as they do for a write
/// any other instance or client made. Without JavaScript, the page again.
async fn move_market(
    req: Req,
    State(pool): State<PgPool>,
    State(market): State<Market>,
    Params(at): Params<url::Move<'_>>,
    Form(input): Form<MoveInput>,
) -> HtmlResult {
    let roll = market.writes.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ listing::now();
    let written = write_market(&pool, &market, &at.seller, 5, roll, Some(input.submit.as_str())).await.map_err(unavailable)?;
    req.patch_or(
        html! { <MoveStatus text="Market lows moved. The rows change when the feed brings the change, on every instance." /> },
        || async { req.redirect(&url::Seller { seller: &at.seller, after: written.seq }) },
    )
    .await
}

/// The seller's listings: every row and the cursor to follow from. Any instance answers; an
/// unknown seller is a 404 and Postgres out of reach a 503, both as JSON.
async fn snapshot(
    State(cluster): State<Cluster<Listings>>,
    Params(at): Params<url::Listings<'_>>,
) -> corten::Result<FeedResponse<Listing>> {
    cluster.snapshot(&at.seller).await
}

async fn live(
    State(cluster): State<Cluster<Listings>>,
    Params(at): Params<url::ListingsLive<'_>>,
    since: Since,
) -> corten::Result<FeedResponse<Listing>> {
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

#[cfg(test)]
mod tests {
    use corten::prelude::*;

    use crate::{
        listing::{self, Listing},
        listing_table::{Line, ListingTable},
        text::Msg,
    };

    /// The listing type the table's script reads (`types/app.d.ts`), and its row's view.
    #[test]
    fn script_types_match_the_server() {
        corten::types::Types::new()
            .add::<Listing>()
            .components(&[ListingTable, crate::listing_row::ListingRow])
            .sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
    }

    /// Each listing is a row keyed by its id, its prices in the page's money format, marked when
    /// its price is over the market low; the counts are the whole seller's.
    #[test]
    fn the_table_draws_each_listing_and_the_feed_follows_them() {
        let t = Msg::default();
        let mut listings = listing::generate(3, 1);
        listings[0].price_cents = 12_345;
        listings[0].market_cents = 10_000;
        listings[1].status = "paused".into();
        let lines: Vec<Line> = listings.iter().map(|l| Line::new(t, l)).collect();
        let html = html! { <ListingTable seller="s1" lines={&lines} total={5_000} t={t} /> }.to_html();
        assert_eq!(html.matches("<tr data-key=").count(), 3, "{html}");
        assert!(
            html.contains(r#"<tr data-key="lst_000001" data-above>"#) && html.contains("$123.45") && html.contains("$100.00"),
            "{html}"
        );
        assert!(html.contains(r#"<tr data-key="lst_000002" data-paused"#), "{html}");
        assert!(html.contains("3 of 5,000 listings"), "{html}");
        let listings = html.split(r#"<output name="listings""#).nth(1).and_then(|rest| rest.split("</output>").next());
        assert!(listings.is_some_and(|el| el.ends_with(">5000")), "the seller's whole count: {html}");
        assert!(html.contains("listing_table.follow("), "one subscription for the counts and the rows: {html}");
    }
}
