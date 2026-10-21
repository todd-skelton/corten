mod app;
mod model;
mod source;
mod views;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let source = source::Source::from_env().map_err(|e| std::io::Error::other(e.to_string()))?;
    app::app(source).serve().await
}
