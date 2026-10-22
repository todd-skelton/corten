mod app;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let market = app::Market::new();
    tokio::spawn(app::simulate_market(market.clone()));
    app::app(market).serve().await
}
