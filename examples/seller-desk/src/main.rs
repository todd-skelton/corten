mod app;
mod text;
/// The UI kit (`corten ui`): public, so a component not used yet is not a warning.
pub mod ui;
mod views;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let rows = std::env::var("DESK_ROWS").ok().and_then(|n| n.parse().ok()).unwrap_or(500);
    corten::serve(app::app(app::Desk::new(rows))).await
}
