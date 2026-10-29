//! `#[derive(UrlValue)]`: an enum of unit variants written in a URL (a route's query or path) as
//! the word serde reads for it, so the words are written once, in `#[serde(rename_all)]` and
//! `#[serde(rename)]`, and a link the app writes is one its handler reads.

use proc_macro::{Delimiter, Group, Literal, TokenStream, TokenTree};

use crate::{
    component::split,
    derive::parse_item,
    to_json::{serde_args_for, variant_rule, Value, RULES},
    tokens::{err, is_punct, q, Error},
};

const NAME: &str = "UrlValue";

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let item = parse_item(input, NAME, false)?;
    if !item.is_enum {
        return err(
            item.name.span(),
            "UrlValue: derive it on an enum of unit variants; a struct in a URL writes its own Display",
        );
    }
    let mut rule = None;
    for arg in serde_args_for(&item.attrs, NAME)? {
        match (arg.key.as_str(), &arg.value) {
            ("rename_all", Value::Str(given)) if RULES.contains(&given.as_str()) => rule = Some(given.clone()),
            ("rename_all", Value::Str(given)) => {
                return err(arg.span, format!("UrlValue: unknown rename_all rule {given:?}; serde's are {}", RULES.join(", ")))
            }
            ("rename_all", _) => {
                return err(arg.span, "UrlValue: use #[serde(rename_all = \"...\")], one rule for reading and writing")
            }
            // The type's own name, serde's path, its error text and bounds: none changes a word.
            ("rename" | "crate" | "expecting" | "bound" | "deny_unknown_fields", _) => {}
            _ => {
                return err(
                    arg.span,
                    format!("UrlValue: #[serde({})] changes the words serde reads in a way UrlValue does not follow", arg.key),
                )
            }
        }
    }
    let Some(body) = &item.body else { return err(item.name.span(), "UrlValue: expected the enum's variants") };
    let mut arms = TokenStream::new();
    for tokens in split(body.stream(), ',') {
        let mut j = 0;
        let mut attrs = Vec::new();
        while is_punct(tokens.get(j), '#') {
            if let Some(TokenTree::Group(g)) = tokens.get(j + 1) {
                attrs.push(g.clone());
            }
            j += 2;
        }
        let Some(TokenTree::Ident(ident)) = tokens.get(j) else { return err(body.span(), "UrlValue: expected a variant") };
        if matches!(tokens.get(j + 1), Some(TokenTree::Group(_))) {
            return err(ident.span(), format!("UrlValue: {ident} has fields; only a unit variant is one word in a URL"));
        }
        let mut rename = None;
        for arg in serde_args_for(&attrs, NAME)? {
            match (arg.key.as_str(), &arg.value) {
                ("rename", Value::Str(given)) => rename = Some(given.clone()),
                ("rename", _) => {
                    return err(arg.span, "UrlValue: use #[serde(rename = \"...\")], one word for reading and writing")
                }
                ("skip" | "skip_deserializing", _) => {
                    return err(
                        arg.span,
                        format!(
                            "UrlValue: serde does not read {ident}, so a link with it would be refused; take it out of the enum"
                        ),
                    )
                }
                // Other words serde also reads, and what only writing JSON uses.
                ("alias" | "other" | "skip_serializing" | "serialize_with" | "bound", _) => {}
                _ => {
                    return err(
                        arg.span,
                        format!(
                            "UrlValue: #[serde({})] changes the words serde reads in a way UrlValue does not follow",
                            arg.key
                        ),
                    )
                }
            }
        }
        let word = rename.unwrap_or_else(|| {
            let text = ident.to_string();
            let text = text.strip_prefix("r#").unwrap_or(&text).to_string();
            rule.as_deref().map_or(text.clone(), |rule| variant_rule(&text, rule))
        });
        arms.extend(q(
            "Self::__A0 => __A1,",
            &[TokenTree::Ident(ident.clone()).into(), TokenTree::Literal(Literal::string(&word)).into()],
        ));
    }
    // The word as `AsRef<str>` (where a list of words is taken: a script's `string[]`, a
    // `RequiredWhen`'s values), as `Display` (a URL, markup) and as a form row's key.
    let mut out = TokenStream::new();
    for (header, body) in [
        (
            "::core::convert::AsRef<::core::primitive::str> for",
            "#[inline]
            fn as_ref(&self) -> &::core::primitive::str {
                match *self { __A0 }
            }",
        ),
        (
            "::corten::form::RowKey for",
            "#[inline]
            fn key_text(&self) -> ::std::borrow::Cow<'_, ::core::primitive::str> {
                ::std::borrow::Cow::Borrowed(::core::convert::AsRef::<::core::primitive::str>::as_ref(self))
            }",
        ),
        (
            "::core::fmt::Display for",
            "#[inline]
            fn fmt(&self, __f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                __f.write_str(::core::convert::AsRef::<::core::primitive::str>::as_ref(self))
            }",
        ),
    ] {
        out.extend(q("#[automatically_derived] impl", &[]));
        out.extend(item.impl_generics.clone());
        out.extend(q(header, &[]));
        out.extend([TokenTree::Ident(item.name.clone())]);
        out.extend(item.type_generics.clone());
        out.extend(item.where_clause.clone());
        out.extend([TokenTree::Group(Group::new(Delimiter::Brace, q(body, &[arms.clone()])))]);
    }
    Ok(out)
}
