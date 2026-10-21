//! Per-page `<head>` tags: description, canonical URL, Open Graph, robots, JSON-LD.
//!
//! A [`crate::Page`] returns them from `head()`. They are printed after `<title>` on a full
//! document and sent with every navigation, where the runtime replaces the previous page's tags,
//! so crawlers and link previews see the same tags however the page was reached.

use std::fmt;

use crate::escape;

/// Head tags for one page. Every tag carries `data-corten-head` so the runtime can swap them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    tags: String,
}

impl Meta {
    pub fn new() -> Self {
        Self::default()
    }

    /// `<meta name="description">`.
    pub fn description(self, text: &str) -> Self {
        self.name("description", text)
    }

    /// `<link rel="canonical">`.
    pub fn canonical(self, url: &str) -> Self {
        self.link("canonical", url)
    }

    /// `<meta name="robots" content="noindex">`, for pages search engines should skip.
    pub fn noindex(self) -> Self {
        self.name("robots", "noindex")
    }

    /// `<meta name="{name}" content="{content}">`.
    pub fn name(mut self, name: &str, content: &str) -> Self {
        self.tags.push_str(&format!(r#"<meta name="{}" content="{}" data-corten-head>"#, escape(name), escape(content)));
        self
    }

    /// `<meta property="{property}" content="{content}">`, for Open Graph (`og:title`).
    pub fn property(mut self, property: &str, content: &str) -> Self {
        self.tags.push_str(&format!(r#"<meta property="{}" content="{}" data-corten-head>"#, escape(property), escape(content)));
        self
    }

    /// `<link rel="{rel}" href="{href}">`.
    pub fn link(mut self, rel: &str, href: &str) -> Self {
        self.tags.push_str(&format!(r#"<link rel="{}" href="{}" data-corten-head>"#, escape(rel), escape(href)));
        self
    }

    /// Structured data as `<script type="application/ld+json">`. It is data, not script, so the
    /// CSP does not apply; `<` is escaped so no value can end the element.
    #[cfg(feature = "json")]
    pub fn json_ld(mut self, value: &impl serde::Serialize) -> Self {
        let json = serde_json::to_string(value).expect("JSON-LD serializes").replace('<', "\\u003c");
        self.tags.push_str(&format!(r#"<script type="application/ld+json" data-corten-head>{json}</script>"#));
        self
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
}

impl fmt::Display for Meta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.tags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_escaped_and_marked() {
        let meta = Meta::new().description("Cards \"&\" <more>").canonical("/items/1").property("og:title", "A");
        assert_eq!(
            meta.to_string(),
            concat!(
                r#"<meta name="description" content="Cards &quot;&amp;&quot; &lt;more&gt;" data-corten-head>"#,
                r#"<link rel="canonical" href="/items/1" data-corten-head>"#,
                r#"<meta property="og:title" content="A" data-corten-head>"#
            )
        );
    }

    #[cfg(feature = "json")]
    #[test]
    fn json_ld_cannot_close_its_script() {
        let meta = Meta::new().json_ld(&serde_json::json!({ "name": "</script><script>alert(1)" }));
        assert_eq!(
            meta.to_string(),
            r#"<script type="application/ld+json" data-corten-head>{"name":"\u003c/script>\u003cscript>alert(1)"}</script>"#
        );
    }
}
