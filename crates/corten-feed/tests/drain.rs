//! `drain` is process-wide, so it gets its own test binary.
use axum::body::Body;
use corten_feed::{drain, Cursor, Feed, Row, Since};
use http_body_util::BodyExt;
use serde::Serialize;

#[derive(Clone, Serialize)]
struct Item {
    id: String,
}

impl Row for Item {
    fn id(&self) -> &str {
        &self.id
    }
}

async fn next_frame(body: &mut Body) -> Option<String> {
    let frame = tokio::time::timeout(std::time::Duration::from_secs(2), body.frame()).await.expect("stream answered in time")?;
    Some(String::from_utf8(frame.unwrap().into_data().unwrap().to_vec()).unwrap())
}

#[tokio::test]
async fn drain_ends_open_streams_and_new_ones() {
    let feed = Feed::new([Item { id: "a".into() }]);
    let cursor = Cursor::parse(&format!("{}-0", feed.generation())).unwrap();
    let mut open = feed.live(Since(Some(cursor.clone()))).into_body();
    assert_eq!(next_frame(&mut open).await.as_deref(), Some("retry: 1000\n\n"));
    drain();
    assert_eq!(next_frame(&mut open).await, None, "the open stream ends without a sync, so the client resumes elsewhere");
    let mut late = feed.live(Since(Some(cursor))).into_body();
    assert_eq!(next_frame(&mut late).await.as_deref(), Some("retry: 1000\n\n"), "replay (if any) is still sent");
    assert_eq!(next_frame(&mut late).await, None);
}
