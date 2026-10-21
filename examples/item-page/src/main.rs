mod app;
mod model;
mod source;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).init();
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/static").into());
    let source = source::Source::from_env().expect("item source");
    let router = app::router(app::App::new(source, &static_dir).expect("load static files"));
    let addr = format!("0.0.0.0:{}", std::env::var("PORT").unwrap_or_else(|_| "8080".into()));
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    println!("item-page listening on {addr}");
    axum::serve(listener, router).with_graceful_shutdown(corten::shutdown_signal()).await.expect("serve");
}
