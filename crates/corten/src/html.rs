//! Small HTML helpers with no dependencies.

/// Escapes text for use in HTML text or a quoted attribute.
/// To append to a string instead, use [`crate::view::escape_into`].
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    crate::view::escape_into(text, &mut out);
    out
}

/// Text that [`escape`] wrote, read back: the five entities it writes become their characters
/// again, and nothing else changes.
#[cfg(feature = "json")]
pub(crate) fn unescape(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('&') {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let entity = [("&amp;", '&'), ("&lt;", '<'), ("&gt;", '>'), ("&quot;", '"'), ("&#39;", '\'')]
            .into_iter()
            .find(|(entity, _)| rest.starts_with(entity));
        match entity {
            Some((entity, c)) => {
                out.push(c);
                rest = &rest[entity.len()..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    std::borrow::Cow::Owned(out)
}

#[cfg(all(test, feature = "json"))]
#[test]
fn unescape_reads_back_what_escape_wrote() {
    let text = "Pika <Promo> & \"Friends\" 'x' &amp";
    assert_eq!(unescape(&escape(text)), text);
    assert_eq!(unescape("plain"), "plain");
}

/// `text` as a JSON string literal, quotes included. `<` is escaped too, so the result is safe
/// inside an inline `<script>`.
pub(crate) fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '<' => out.push_str("\\u003c"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// What the browser would drop from a patch (`req.patch`, a [`crate::live`] fragment): the first
/// top-level element without an `id`, or top-level text, described for an error. The runtime
/// replaces each top-level element by its id and ignores the rest, so either is a view that never
/// shows. Reads markup as `html!` writes it (attribute values quoted, `<` escaped in text).
/// Checked in debug builds only.
pub(crate) fn patch_problem(html: &str) -> Option<String> {
    const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];
    const RAW_TEXT: &[&str] = &["script", "style", "textarea", "title"];
    let bytes = html.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            let end = html[i..].find('<').map_or(bytes.len(), |n| i + n);
            let text = html[i..end].trim();
            if depth == 0 && !text.is_empty() {
                let shown: String = text.chars().take(40).collect();
                return Some(format!("text {shown:?} outside an element"));
            }
            i = end;
            continue;
        }
        if html[i..].starts_with("<!--") {
            i = html[i..].find("-->").map_or(bytes.len(), |n| i + n + 3);
            continue;
        }
        if bytes.get(i + 1) == Some(&b'/') {
            depth = depth.saturating_sub(1);
            i = html[i..].find('>').map_or(bytes.len(), |n| i + n + 1);
            continue;
        }
        // A start tag: its name, then its attributes up to `>`.
        let start = i;
        i += 1;
        let name_end = html[i..].find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/').map_or(bytes.len(), |n| i + n);
        let name = html[i..name_end].to_ascii_lowercase();
        i = name_end;
        let mut id = false;
        let mut self_closing = false;
        while i < bytes.len() && bytes[i] != b'>' {
            let c = bytes[i];
            if c.is_ascii_whitespace() {
                i += 1;
            } else if c == b'/' {
                self_closing = true;
                i += 1;
            } else {
                self_closing = false;
                let attr_end = html[i..]
                    .find(|c: char| c.is_ascii_whitespace() || c == '=' || c == '>' || c == '/')
                    .map_or(bytes.len(), |n| i + n);
                let attr = &html[i..attr_end];
                i = attr_end;
                let mut value = "";
                if bytes.get(i) == Some(&b'=') {
                    i += 1;
                    match bytes.get(i) {
                        Some(&q @ (b'"' | b'\'')) => {
                            let close = html[i + 1..].find(q as char).map_or(bytes.len(), |n| i + 1 + n);
                            value = &html[i + 1..close];
                            i = close + 1;
                        }
                        _ => {
                            let end =
                                html[i..].find(|c: char| c.is_ascii_whitespace() || c == '>').map_or(bytes.len(), |n| i + n);
                            value = &html[i..end];
                            i = end;
                        }
                    }
                }
                id |= attr.eq_ignore_ascii_case("id") && !value.is_empty();
            }
        }
        i = (i + 1).min(bytes.len());
        if depth == 0 && !id {
            let tag = &html[start..i];
            let shown: String = tag.chars().take(80).collect();
            let more = if shown.len() < tag.len() { ".." } else { "" };
            return Some(format!("{shown}{more} has no id"));
        }
        if RAW_TEXT.contains(&name.as_str()) {
            // Its text up to the end tag, and the end tag: nothing in it is markup.
            let close = format!("</{name}");
            i = html[i..].to_ascii_lowercase().find(&close).map_or(bytes.len(), |n| i + n);
            i = html[i..].find('>').map_or(bytes.len(), |n| i + n + 1);
        } else if !VOID.contains(&name.as_str()) && !self_closing {
            depth += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::patch_problem;

    #[test]
    fn a_patch_names_its_first_top_level_element_without_an_id() {
        assert_eq!(patch_problem(r#"<div id="a"><p>x</p></div> <form id="f"><input name="q"></form>"#), None);
        assert_eq!(patch_problem(r#"<template id="row-7" data-remove></template>"#), None);
        assert_eq!(patch_problem(r#"<template id="rows" data-append><tr><td>8</td></tr></template>"#), None);
        assert_eq!(patch_problem(""), None, "an empty patch changes nothing, and says so");
        assert_eq!(patch_problem("<!-- note --><p id=b>1</p>\n"), None);
        assert_eq!(
            patch_problem(r#"<div id="recovery">ok</div><div class="write-off"><form id="w">..</form></div>"#).as_deref(),
            Some(r#"<div class="write-off"> has no id"#),
            "the pilot's Write off form, wrapped"
        );
        assert_eq!(patch_problem(r#"<p id="">x</p>"#).as_deref(), Some(r#"<p id=""> has no id"#));
        assert_eq!(
            patch_problem(r#"<p data-id="x" title="id=y">x</p>"#).as_deref(),
            Some(r#"<p data-id="x" title="id=y"> has no id"#)
        );
        assert_eq!(patch_problem(r#"<p id="a">x</p>saved"#).as_deref(), Some(r#"text "saved" outside an element"#));
        // Nested markup, void elements and raw text do not end an element early.
        let nested = r#"<section id="s"><img alt="a > b"><br/><script>if (a < b) "</div>";</script><textarea><p></textarea><div><p>x</p></div></section><p id="t"></p>"#;
        assert_eq!(patch_problem(nested), None);
        let long = format!(r#"<div class="{}">x</div>"#, "c".repeat(100));
        assert!(patch_problem(&long).unwrap().ends_with(".. has no id"));
    }
}
