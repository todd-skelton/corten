mod app;
mod lists;
mod market;
mod news;
mod theme;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    corten::serve(app::app(market::Market::new())).await
}
