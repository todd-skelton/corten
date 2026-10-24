//! The live benchmark's Corten app: one feed of listing prices. Clients load the snapshot and
//! follow `/live/feed/live` (server-sent events, as `subscribe()` in corten.js does); every
//! `POST /live/publish` is one write, so one numbered patch, encoded once for every client.
use corten::{
    axum::{Json, http::StatusCode},
    feed::{Feed, Row, Since},
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
    let routes = Router::new()
        .route("/", get(|| async { "live" }))
        .route("/live/feed", get(|State(feed): State<Feed<Tick>>| async move { feed.snapshot() }))
        .route("/live/feed/live", get(|State(feed): State<Feed<Tick>>, since: Since| async move { feed.live(since) }))
        .with_state(feed.clone());
    let publish = post(|State(feed): State<Feed<Tick>>, Json(p): Json<Publish>| async move {
        feed.write(|tx| tx.insert(Tick { id: "lst_1".into(), seq: p.seq, sent: p.sent, price: p.price }));
        StatusCode::NO_CONTENT
    })
    .with_state(feed);
    corten::serve(App::new(routes).api::<PublishRoute>(|api| api.route::<PublishRoute>(publish))).await
}

#[derive(Route)]
#[route("/live/publish")]
struct PublishRoute;
