//! `#[derive(Refs)]`: the `data-ref` names of one island, as an enum.
//!
//! `#[island("listing-sort")] enum ListingSortRef { Toggle, Rows }` prints `toggle` and `rows`
//! (kebab-case), and `corten::types` writes the same names as a TypeScript union for the island.

use proc_macro::{TokenStream, TokenTree};

use crate::{
    derive::{parse, string, unit_variants},
    tokens::{err, q, Error},
};

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let item = parse(input, "Refs")?;
    let usage = "Refs is derived for an enum of unit variants: #[derive(Refs)] #[island(\"listing-sort\")] enum ListingSortRef { Toggle, Rows }";
    let (true, Some(body)) = (item.is_enum, &item.body) else { return err(item.name.span(), usage) };
    if !item.type_generics.is_empty() {
        return err(item.name.span(), "Refs: the enum cannot be generic");
    }
    let Some((island, span)) = item.attr_string("island")? else {
        return err(item.name.span(), "Refs needs the island's tag: #[island(\"listing-sort\")]");
    };
    if !is_custom_element(&island) {
        return err(span, format!("{island:?} is not a custom element name: lowercase, starting with a letter, with a -"));
    }
    let variants = unit_variants(body, "Refs")?;
    if variants.is_empty() {
        return err(item.name.span(), "Refs: an island with no refs needs no enum");
    }
    let mut arms = TokenStream::new();
    let mut names = TokenStream::new();
    for variant in &variants {
        let name = kebab(&variant.to_string());
        arms.extend(q("Self::__A0 => __A1,", &[TokenTree::Ident(variant.clone()).into(), string(&name).into()]));
        names.extend(q("__A0,", &[string(&name).into()]));
    }
    let mut out = item.impl_block(
        "::corten::island::Refs",
        q(
            "const ISLAND: &'static str = __A0;
            const TYPE: &'static str = __A1;
            const NAMES: &'static [&'static str] = &[__A2];
            fn name(&self) -> &'static str { match self { __A3 } }",
            &[string(&island).into(), string(&item.name.to_string()).into(), names, arms],
        ),
    );
    out.extend(item.impl_block(
        "::corten::view::Render",
        q("fn render(&self, out: &mut ::std::string::String) { out.push_str(::corten::island::Refs::name(self)) }", &[]),
    ));
    out.extend(item.impl_block(
        "::corten::view::AttrValue",
        q(
            "fn write_attr(&self, name: &str, out: &mut ::std::string::String) {
                out.push_str(name);
                out.push_str(\"=\\\"\");
                out.push_str(::corten::island::Refs::name(self));
                out.push('\"');
            }",
            &[],
        ),
    ));
    Ok(out)
}

pub fn is_custom_element(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.contains('-')
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' || c == '.')
}

/// `SortButton` to `sort-button`.
pub fn kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_kebab_case_and_tags_are_checked() {
        assert_eq!(kebab("Rows"), "rows");
        assert_eq!(kebab("SortButton"), "sort-button");
        assert!(is_custom_element("listing-sort"));
        assert!(!is_custom_element("listing"));
        assert!(!is_custom_element("Listing-Sort"));
    }
}
