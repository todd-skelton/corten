mod app;
mod model;
mod source;
mod text;
mod theme;
mod views;

/// Every URL the app serves and links to, with the handlers that serve it (in `app`), which share
/// the item source. Views and redirects build links from the same structs, so a link to a missing
/// parameter or a renamed route fails to compile, and a route without a handler does too.
#[corten::routes(state = source::Source)]
mod routes {
    #[route("/", get = app::home)]
    pub struct Home;

    #[route("/items/{id}", get = app::show, post = app::create_alert)]
    pub struct Item<'a> {
        pub id: &'a str,
    }

    /// Where an item was before the site moved to `/items/`: a permanent redirect, so links and
    /// search results that still name it land on the page.
    #[route("/item/{id}", get = app::old_item)]
    pub struct OldItem<'a> {
        pub id: &'a str,
    }

    #[route("/categories/{slug}", get = app::category)]
    pub struct Category<'a> {
        pub slug: &'a str,
    }

    #[route("/robots.txt", get = app::robots)]
    pub struct Robots;

    #[route("/sitemap.xml", get = app::sitemap)]
    pub struct Sitemap;
}

/// The JSON routes, for other services: every failure under `/api` is JSON, whatever the client
/// accepts. `ApiRoot` is the prefix, so it needs no handler of its own.
#[corten::routes(api = ApiRoot, state = source::Source)]
mod api {
    #[route("/api")]
    pub struct ApiRoot;

    /// An item as JSON (the same data the page shows).
    #[route("/api/items/{id}", get = app::item_json)]
    pub struct ItemJson<'a> {
        pub id: &'a str,
    }
}

/// The MCP endpoint for agents: an API of one route, which `app::tools` serves.
#[corten::routes(api = Mcp)]
mod mcp {
    #[route("/mcp")]
    pub struct Mcp;
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let source = source::Source::from_env().map_err(|e| std::io::Error::other(e.to_string()))?;
    corten::serve(app::app(source)).await
}
