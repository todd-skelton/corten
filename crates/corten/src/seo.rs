//! `robots.txt` and XML sitemaps, written as the search engine protocols define them.

use axum::{
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
};

use crate::view::escape_into;

/// Most URLs one sitemap file may list. Split larger sets with [`sitemap_index`].
pub const SITEMAP_MAX_URLS: usize = 50_000;

/// One `<url>` entry.
pub struct SitemapUrl<'a> {
    pub loc: &'a str,
    /// `YYYY-MM-DD` or a full W3C datetime.
    pub lastmod: Option<&'a str>,
}

/// A `<urlset>` sitemap response. URLs must be absolute; more than [`SITEMAP_MAX_URLS`] panics in
/// debug builds, because search engines reject the file.
pub fn sitemap<'a>(urls: impl IntoIterator<Item = SitemapUrl<'a>>) -> Response {
    let mut out =
        String::from(r#"<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">"#);
    let mut count = 0;
    for url in urls {
        count += 1;
        out.push_str("<url><loc>");
        escape_into(url.loc, &mut out);
        out.push_str("</loc>");
        if let Some(lastmod) = url.lastmod {
            out.push_str("<lastmod>");
            escape_into(lastmod, &mut out);
            out.push_str("</lastmod>");
        }
        out.push_str("</url>");
    }
    debug_assert!(count <= SITEMAP_MAX_URLS, "a sitemap holds at most {SITEMAP_MAX_URLS} URLs");
    out.push_str("</urlset>");
    xml(out)
}

/// A `<sitemapindex>` that points at sitemap pages (`/sitemap/items/1.xml`, ...).
pub fn sitemap_index<'a>(sitemaps: impl IntoIterator<Item = &'a str>) -> Response {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">"#,
    );
    for loc in sitemaps {
        out.push_str("<sitemap><loc>");
        escape_into(loc, &mut out);
        out.push_str("</loc></sitemap>");
    }
    out.push_str("</sitemapindex>");
    xml(out)
}

fn xml(body: String) -> Response {
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("application/xml; charset=utf-8")),
            (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600")),
        ],
        body,
    )
        .into_response()
}

/// `robots.txt`: every path in `disallow` is closed to all crawlers, and `sitemap` is announced.
pub fn robots(disallow: &[&str], sitemap: Option<&str>) -> Response {
    let mut out = String::from("User-agent: *\n");
    if disallow.is_empty() {
        out.push_str("Allow: /\n");
    }
    for path in disallow {
        assert!(!path.contains('\n'), "one path per line");
        out.push_str(&format!("Disallow: {path}\n"));
    }
    if let Some(sitemap) = sitemap {
        out.push_str(&format!("Sitemap: {sitemap}\n"));
    }
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8")),
            (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600")),
        ],
        out,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn text(res: Response) -> String {
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }

    #[tokio::test]
    async fn sitemap_escapes_urls() {
        let res = sitemap([SitemapUrl { loc: "https://a.com/s?q=1&b=<2>", lastmod: Some("2026-10-20") }]);
        assert_eq!(res.headers()["content-type"], "application/xml; charset=utf-8");
        assert!(text(res)
            .await
            .contains("<url><loc>https://a.com/s?q=1&amp;b=&lt;2&gt;</loc><lastmod>2026-10-20</lastmod></url>"));
        assert!(text(sitemap_index(["https://a.com/sitemap/items/1.xml"]))
            .await
            .contains("<sitemap><loc>https://a.com/sitemap/items/1.xml</loc></sitemap>"));
    }

    #[tokio::test]
    async fn robots_lists_rules_and_sitemap() {
        let body = text(robots(&["/account/", "/api/"], Some("https://a.com/sitemap.xml"))).await;
        assert_eq!(body, "User-agent: *\nDisallow: /account/\nDisallow: /api/\nSitemap: https://a.com/sitemap.xml\n");
    }
}
