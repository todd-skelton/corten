//! `#[derive(FromRef)]`: a handler takes one part of the app's state (`State<Market>`) instead of
//! all of it. Each field gets `impl FromRef<State> for FieldType`, which clones the field: what
//! an app would write by hand, with `proc_macro` alone (axum's derive needs its `macros` feature).

use proc_macro::{Delimiter, TokenStream, TokenTree};

use crate::{
    component::split,
    derive::parse,
    tokens::{err, is_ident, is_punct, q, Error},
};

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let item = parse(input, "FromRef")?;
    let usage = "FromRef is derived for a struct with named fields, one per part a handler takes";
    let body = match (&item.body, item.is_enum) {
        (Some(body), false) => body.clone(),
        _ => return err(item.name.span(), usage),
    };
    let mut state = TokenStream::from(TokenTree::Ident(item.name.clone()));
    state.extend(item.type_generics.clone());
    let mut out = TokenStream::new();
    // Each type once: two fields of one type would be two impls of FromRef for that type.
    let mut seen: Vec<(String, String)> = Vec::new();
    for field in split(body.stream(), ',') {
        let mut j = 0;
        let mut skip = false;
        loop {
            match field.get(j) {
                Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                    if let Some(TokenTree::Group(g)) = field.get(j + 1) {
                        let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                        if is_ident(inner.first(), "from_ref") {
                            match inner.get(1) {
                                Some(TokenTree::Group(args))
                                    if args.delimiter() == Delimiter::Parenthesis && args.stream().to_string() == "skip" =>
                                {
                                    skip = true
                                }
                                _ => return err(g.span(), "expected #[from_ref(skip)]"),
                            }
                        }
                    }
                    j += 2;
                }
                Some(TokenTree::Ident(id)) if id.to_string() == "pub" => {
                    j += 1;
                    if matches!(field.get(j), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis) {
                        j += 1;
                    }
                }
                _ => break,
            }
        }
        let (Some(TokenTree::Ident(name)), true) = (field.get(j), is_punct(field.get(j + 1), ':')) else {
            return err(field.get(j).map_or(item.name.span(), TokenTree::span), "expected a named field");
        };
        if skip {
            continue;
        }
        // `Self` in a field's type is the state; outside the struct it would be the field's type.
        let ty = replace_self(field[j + 2..].iter().cloned().collect(), &state);
        let key = ty.to_string();
        if let Some((_, first)) = seen.iter().find(|(t, _)| *t == key) {
            return err(
                name.span(),
                format!("`{first}` and `{name}` are both `{key}`, so a handler's State<{key}> could be either: mark one #[from_ref(skip)]"),
            );
        }
        seen.push((key, name.to_string()));
        let mut header: TokenStream = "#[automatically_derived] impl".parse().expect("parses");
        header.extend(item.impl_generics.clone());
        header.extend(q("::corten::axum::extract::FromRef<__A0> for __A1", &[state.clone(), ty.clone()]));
        // The field's type clones for every parameter the impl allows (`Part<T>` only where `T`
        // does), and the struct's own bounds still hold.
        let mut bounds = replace_self(item.where_clause.clone(), &state);
        let trailing = bounds.clone().into_iter().last();
        match trailing {
            None => bounds = q("where", &[]),
            Some(TokenTree::Punct(p)) if p.as_char() == ',' => {}
            Some(_) => bounds.extend(q(",", &[])),
        }
        bounds.extend(q("__A0: ::core::clone::Clone", std::slice::from_ref(&ty)));
        header.extend(bounds);
        out.extend(header);
        out.extend([TokenTree::Group(proc_macro::Group::new(
            Delimiter::Brace,
            q(
                "#[inline] fn from_ref(state: &__A0) -> Self { ::core::clone::Clone::clone(&state.__A1) }",
                &[state.clone(), TokenStream::from(TokenTree::Ident(name.clone()))],
            ),
        ))]);
    }
    Ok(out)
}

/// `tokens` with each `Self` replaced by `state`, inside groups too.
fn replace_self(tokens: TokenStream, state: &TokenStream) -> TokenStream {
    let mut out = TokenStream::new();
    for token in tokens {
        match token {
            TokenTree::Ident(id) if id.to_string() == "Self" => out.extend(state.clone()),
            TokenTree::Group(g) => {
                let mut inner = proc_macro::Group::new(g.delimiter(), replace_self(g.stream(), state));
                inner.set_span(g.span());
                out.extend([TokenTree::Group(inner)]);
            }
            other => out.extend([other]),
        }
    }
    out
}
