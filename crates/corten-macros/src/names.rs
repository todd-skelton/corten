//! Name conversions between Rust, HTML and JavaScript.

/// `OfferList` to `offer-list`.
pub fn kebab(name: &str) -> String {
    words(name).join("-")
}

/// `OfferList` to `offer_list`.
pub fn snake(name: &str) -> String {
    words(name).join("_")
}

/// Lowercase words of an UpperCamelCase name; a run of capitals is one word (`URLField` is
/// `url`, `field`).
fn words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut words: Vec<String> = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        let starts = i == 0
            || (c.is_ascii_uppercase()
                && (chars[i - 1].is_ascii_lowercase()
                    || chars[i - 1].is_ascii_digit()
                    || chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase())));
        if starts {
            words.push(String::new());
        }
        words.last_mut().expect("a word started").push(c.to_ascii_lowercase());
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_convert_between_rust_and_html_and_javascript() {
        assert_eq!(kebab("OfferList"), "offer-list");
        assert_eq!(snake("OfferList"), "offer_list");
        assert_eq!(kebab("Counter"), "counter");
        assert_eq!(snake("URLField"), "url_field");
        assert_eq!(kebab("Grid2Rows"), "grid2-rows");
    }
}
