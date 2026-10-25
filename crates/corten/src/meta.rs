//! A page's title and `<head>` tags: description, canonical URL, Open Graph, robots, JSON-LD.
//!
//! `req.page("About", view)` takes the title alone; `req.page(Meta::new(title).description(..), view)`
//! adds tags. They are printed after `<title>` on a full document and sent with every navigation,
//! where the runtime replaces the previous page's tags, so crawlers and link previews see the
//! same tags however the page was reached.
//!
//! A page that shows a read still catching up with a write ("saved, updating") asks to be read
//! again with [`Meta::read_again`], however it was reached.

use std::{fmt, time::Duration};

use crate::view::escape_into;

/// A page's title and head tags. Every tag carries `data-corten-head` so the runtime can swap them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    title: String,
    tags: String,
}

impl From<&str> for Meta {
    fn from(title: &str) -> Self {
        Meta::new(title)
    }
}

impl From<String> for Meta {
    fn from(title: String) -> Self {
        Meta::new(title)
    }
}

impl Meta {
    /// A page titled `title` (escaped by Corten).
    pub fn new(title: impl Into<String>) -> Self {
        Meta { title: title.into(), tags: String::new() }
    }

    /// The text for `<title>`.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The tags, rendered.
    pub fn tags(&self) -> &str {
        &self.tags
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
    pub fn name(self, name: &str, content: &str) -> Self {
        self.tag(r#"<meta name=""#, name, r#"" content=""#, content)
    }

    /// `<meta property="{property}" content="{content}">`, for Open Graph (`og:title`).
    pub fn property(self, property: &str, content: &str) -> Self {
        self.tag(r#"<meta property=""#, property, r#"" content=""#, content)
    }

    /// `<link rel="{rel}" href="{href}">`.
    pub fn link(self, rel: &str, href: &str) -> Self {
        self.tag(r#"<link rel=""#, rel, r#"" href=""#, href)
    }

    /// `{open}{first}{between}{second}" data-corten-head>`, both values escaped into the tags
    /// with no string in between.
    fn tag(mut self, open: &str, first: &str, between: &str, second: &str) -> Self {
        let tags = &mut self.tags;
        tags.push_str(open);
        escape_into(first, tags);
        tags.push_str(between);
        escape_into(second, tags);
        tags.push_str(r#"" data-corten-head>"#);
        self
    }

    /// Structured data as `<script type="application/ld+json">`. It is data, not script, so the
    /// CSP does not apply; `<` is escaped so no value can end the element.
    #[cfg(feature = "json")]
    pub fn json_ld(mut self, value: &(impl crate::json::ToJson + ?Sized)) -> Self {
        let json = value.to_json();
        self.tags.push_str(r#"<script type="application/ld+json" data-corten-head>"#);
        // `<` only ever appears inside a JSON string, where `\u003c` reads the same.
        for (i, part) in json.split('<').enumerate() {
            if i > 0 {
                self.tags.push_str("\\u003c");
            }
            self.tags.push_str(part);
        }
        self.tags.push_str("</script>");
        self
    }

    /// Read this page again after `after` (whole seconds, at least one): for a page that shows a
    /// read still catching up with a write. The runtime fetches the same URL and swaps it in,
    /// replacing the history entry and keeping the scroll position and the focused element (by
    /// its id), however the page
    /// was reached (a full load, a navigation, a form's redirect); a navigation away stops it,
    /// and a hidden tab waits until it is shown. Without JavaScript the browser reloads it
    /// (`<meta http-equiv="refresh">` in `<noscript>`). The page asks again each time it is
    /// read, so it stops when the server stops asking (the write has landed, or is too old to
    /// wait for).
    ///
    /// Not the `Refresh` header: browsers keep a full load's header after the runtime has
    /// navigated away, and then load that first URL again. Debug builds answer 500 to a page
    /// that sends one.
    pub fn read_again(mut self, after: Duration) -> Self {
        use fmt::Write;
        let seconds = after.as_secs() + u64::from(after.subsec_nanos() > 0);
        let seconds = seconds.max(1);
        let _ = write!(
            self.tags,
            r#"<noscript data-corten-read-again="{seconds}" data-corten-head><meta http-equiv="refresh" content="{seconds}"></noscript>"#
        );
        self
    }

    /// True when there are no tags (the title does not count).
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
}

/// The tags (not the title).
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
        let meta = Meta::new("T").description("Cards \"&\" <more>").canonical("/items/1").property("og:title", "A");
        assert_eq!(
            meta.to_string(),
            concat!(
                r#"<meta name="description" content="Cards &quot;&amp;&quot; &lt;more&gt;" data-corten-head>"#,
                r#"<link rel="canonical" href="/items/1" data-corten-head>"#,
                r#"<meta property="og:title" content="A" data-corten-head>"#
            )
        );
    }

    #[test]
    fn read_again_is_whole_seconds_and_works_without_javascript() {
        let tag = |after| Meta::new("T").read_again(after).to_string();
        assert_eq!(
            tag(Duration::from_secs(2)),
            r#"<noscript data-corten-read-again="2" data-corten-head><meta http-equiv="refresh" content="2"></noscript>"#
        );
        assert!(tag(Duration::from_millis(1500)).contains(r#"read-again="2""#), "rounded up");
        assert!(tag(Duration::ZERO).contains(r#"read-again="1""#), "never sooner than a second");
    }

    #[cfg(feature = "json")]
    #[test]
    fn json_ld_cannot_close_its_script() {
        let meta = Meta::new("T").json_ld(&serde_json::json!({ "name": "</script><script>alert(1)" }));
        assert_eq!(
            meta.to_string(),
            r#"<script type="application/ld+json" data-corten-head>{"name":"\u003c/script>\u003cscript>alert(1)"}</script>"#
        );
    }
}
