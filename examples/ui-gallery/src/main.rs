#[tokio::main]
async fn main() -> std::io::Result<()> {
    corten::serve(ui_gallery::app::app()).await
}
