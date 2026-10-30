mod app;
mod room;
mod store;
mod text;
mod views;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // A file, so bids and sessions outlive a restart; `sqlite::memory:` for one that does not.
    let database = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:auction.db?mode=rwc".into());
    let auction = app::Auction::open(&database, std::env::var("HOUSE_KEY").ok()).await.map_err(std::io::Error::other)?;
    corten::serve(app::app(auction)).await
}
