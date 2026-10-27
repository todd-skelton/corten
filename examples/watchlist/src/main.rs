mod app;
mod theme;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let market = app::Market::new();
    tokio::spawn(app::simulate_market(market.clone()));
    corten::serve(app::app(market)).await
}
