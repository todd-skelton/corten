//! Small HTML helpers with no dependencies.

/// Escapes text for use in HTML text or a quoted attribute.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
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

/// Custom element tag names in `html`, first occurrence order, without duplicates.
///
/// Corten uses this to preload exactly the island modules a page renders, so pages never declare
/// their islands by hand. Rendered text and attributes are escaped, so every `<` that starts a
/// name is a real tag. `corten-*` tags are built into the runtime and skipped.
pub fn find_islands(html: &str) -> Vec<&str> {
    // Every custom element name has a `-`, and a page has far fewer dashes than tags, so this
    // looks for dashes (memchr, SIMD) and checks whether each one sits in a tag name: about 8x
    // faster than visiting every `<` on the example item page (2.4 µs instead of 21 µs).
    let bytes = html.as_bytes();
    let name_byte = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-';
    let mut found: Vec<&str> = Vec::new();
    let mut i = 0;
    while let Some(offset) = memchr::memchr(b'-', &bytes[i..]) {
        let dash = i + offset;
        let mut start = dash;
        while start > 0 && name_byte(bytes[start - 1]) {
            start -= 1;
        }
        let mut end = dash + 1;
        while end < bytes.len() && name_byte(bytes[end]) {
            end += 1;
        }
        if start > 0 && bytes[start - 1] == b'<' && bytes[start].is_ascii_lowercase() {
            let name = &html[start..end];
            if !name.starts_with("corten-") && !found.contains(&name) {
                found.push(name);
            }
        }
        i = end;
    }
    found
}
