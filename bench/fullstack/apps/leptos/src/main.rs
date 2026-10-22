#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use item_page_leptos::app::{shell, App};
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use tower_http::compression::CompressionLayer;

    let conf = get_configuration(None).expect("leptos configuration");
    // Load the fixture (if any) before listening, like the other apps.
    item_page_leptos::app::warm_fixture();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);
    let app = Router::new()
        .route("/api/items/{id}", axum::routing::get(item_page_leptos::app::item_json))
        .merge(live::routes())
        .leptos_routes(&leptos_options, routes, {
            let options = leptos_options.clone();
            move || shell(options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
        .layer(CompressionLayer::new())
        .with_state(leptos_options);
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    println!("leptos item page listening on {addr}");
    axum::serve(listener, app.into_make_service()).await.expect("serve");
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}

/// The live benchmark (bench/live): `GET /live/prices` is a server-sent event stream and every
/// `POST /live/publish` body goes to each open stream, through a tokio broadcast channel.
#[cfg(feature = "ssr")]
mod live {
    use std::{convert::Infallible, sync::LazyLock};

    use axum::{
        http::StatusCode,
        response::sse::{Event, KeepAlive, Sse},
        routing::{get, post},
        Router,
    };
    use futures::Stream;
    use tokio::sync::broadcast::{self, error::RecvError};

    static PRICES: LazyLock<broadcast::Sender<String>> = LazyLock::new(|| broadcast::channel(1024).0);

    pub fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
        Router::new().route("/live/prices", get(prices)).route("/live/publish", post(publish))
    }

    async fn prices() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
        let stream = futures::stream::unfold(PRICES.subscribe(), |mut rx| async move {
            loop {
                match rx.recv().await {
                    Ok(data) => return Some((Ok(Event::default().data(data)), rx)),
                    // A slow client skips what it missed rather than holding the others back.
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => return None,
                }
            }
        });
        Sse::new(stream).keep_alive(KeepAlive::default())
    }

    async fn publish(body: String) -> StatusCode {
        let _ = PRICES.send(body);
        StatusCode::NO_CONTENT
    }
}
