//! The live benchmark's Corten app: one feed of listing prices. Clients load the snapshot and
//! follow `/live/feed/live` (server-sent events, as `subscribe()` in corten.js does); every
//! `POST /live/publish` is one write, so one numbered patch, encoded once for every client.
use corten::{
    axum::{Json, http::StatusCode},
    feed::{Feed, FeedResponse, Row, Since},
    prelude::*,
};
use serde::{Deserialize, Serialize};

/// A listing's price, with what the benchmark needs to time and order its delivery.
#[derive(Clone, Serialize, Deserialize)]
struct Tick {
    id: String,
    seq: u64,
    sent: f64,
    price: String,
}

impl Row for Tick {
    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Deserialize)]
struct Publish {
    seq: u64,
    sent: f64,
    price: String,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let feed: Feed<Tick> = Feed::new([]);
    corten::serve(App::new(url::router(feed.clone())).api(|routes| ingest::api(routes, feed))).await
}

/// The page, the feed's snapshot and its live stream.
#[corten::routes(state = Feed<Tick>)]
mod url {
    #[route("/", get = home)]
    pub struct Home;

    #[route("/live/feed", get = snapshot)]
    pub struct Snapshot;

    #[route("/live/feed/live", get = live)]
    pub struct Live;
}

/// The publish route, an API of one route, so a failure there is JSON.
#[corten::routes(api = Publish, state = Feed<Tick>)]
mod ingest {
    #[route("/live/publish", post = publish)]
    pub struct Publish;
}

async fn home() -> &'static str {
    "live"
}

async fn snapshot(State(feed): State<Feed<Tick>>) -> FeedResponse {
    feed.snapshot()
}

async fn live(State(feed): State<Feed<Tick>>, since: Since) -> FeedResponse {
    feed.live(since)
}

/// One write: one numbered patch, encoded once for every client.
async fn publish(State(feed): State<Feed<Tick>>, Json(p): Json<Publish>) -> StatusCode {
    feed.write(|tx| tx.insert(Tick { id: "lst_1".into(), seq: p.seq, sent: p.sent, price: p.price }));
    StatusCode::NO_CONTENT
}
