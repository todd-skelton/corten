mod app;
mod text;
mod views;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let rows = std::env::var("DESK_ROWS").ok().and_then(|n| n.parse().ok()).unwrap_or(500);
    let desk = app::Desk::new(rows);
    tokio::spawn(app::simulate_sales(desk.clone()));
    corten::serve(app::app(desk)).await
}
