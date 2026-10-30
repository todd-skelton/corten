mod app;
mod cards;
mod text;
mod views;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    corten::serve(app::app(app::Builder::new())).await
}