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

    #[route("/categories/{slug}", get = app::category)]
    pub struct Category<'a> {
        pub slug: &'a str,
    }

    #[route("/robots.txt", get = app::robots)]
    pub struct Robots;

    #[route("/sitemap.xml", get = app::sitemap)]
    pub struct Sitemap;

    /// Where the JSON routes are: `App::api`'s prefix.
    #[route("/api", link)]
    pub struct ApiRoot;

    /// An item as JSON, for other services (the same data the page shows), served by the API.
    #[route("/api/items/{id}", link)]
    pub struct ItemJson<'a> {
        pub id: &'a str,
    }

    /// The MCP endpoint for agents, served by its API.
    #[route("/mcp", link)]
    pub struct Mcp;
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let source = source::Source::from_env().map_err(|e| std::io::Error::other(e.to_string()))?;
    corten::serve(app::app(source)).await
}
