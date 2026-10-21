mod app;

use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    let state = app::State::new();
    tokio::spawn(app::simulate_market(state.cards.clone()));
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/static").into());
    let router = app::router(state).nest_service("/static", ServeDir::new(static_dir));
    let addr = format!("0.0.0.0:{}", std::env::var("PORT").unwrap_or_else(|_| "8080".into()));
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    println!("hello listening on {addr}");
    axum::serve(listener, router).await.expect("serve");
}
