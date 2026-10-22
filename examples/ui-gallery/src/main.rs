mod app;
mod field;
mod gallery;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    app::app().serve().await
}
