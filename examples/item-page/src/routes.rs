//! Every URL the app serves and links to. The router mounts `PATH`; views and redirects build
//! links from the same structs, so a link to a missing parameter or a renamed route fails to compile.
use corten::Route;

#[derive(Route)]
#[route("/")]
pub struct Home;

#[derive(Route)]
#[route("/items/{id}")]
pub struct Item<'a> {
    pub id: &'a str,
}

#[derive(Route)]
#[route("/categories/{slug}")]
pub struct Category<'a> {
    pub slug: &'a str,
}

#[derive(Route)]
#[route("/robots.txt")]
pub struct Robots;

#[derive(Route)]
#[route("/sitemap.xml")]
pub struct Sitemap;

/// An item as JSON, for other services (the same data the page shows).
#[derive(Route)]
#[route("/api/items/{id}")]
pub struct ItemJson<'a> {
    pub id: &'a str,
}

/// The MCP endpoint for agents.
#[derive(Route)]
#[route("/mcp")]
pub struct Mcp;
