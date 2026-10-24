mod app;
mod model;
mod routes;
mod source;
mod text;
mod theme;
mod views;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let source = source::Source::from_env().map_err(|e| std::io::Error::other(e.to_string()))?;
    corten::serve(app::app(source)).await
}
