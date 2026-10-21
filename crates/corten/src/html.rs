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

/// Custom element tag names in `html`, first occurrence order, without duplicates.
///
/// Corten uses this to preload exactly the island modules a page renders, so pages never declare
/// their islands by hand. Rendered text and attributes are escaped, so every `<` that starts a
/// name is a real tag. `corten-*` tags are built into the runtime and skipped.
pub fn find_islands(html: &str) -> Vec<&str> {
    let bytes = html.as_bytes();
    let mut found: Vec<&str> = Vec::new();
    let mut i = 0;
    while let Some(offset) = bytes[i..].iter().position(|&b| b == b'<') {
        let start = i + offset + 1;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_lowercase() || bytes[end].is_ascii_digit() || bytes[end] == b'-') {
            end += 1;
        }
        let name = &html[start..end];
        if name.starts_with(|c: char| c.is_ascii_lowercase())
            && name.contains('-')
            && !name.starts_with("corten-")
            && !found.contains(&name)
        {
            found.push(name);
        }
        i = end.max(start);
    }
    found
}
