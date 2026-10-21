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
