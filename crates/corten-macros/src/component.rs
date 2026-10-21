//! `#[component]`: a function becomes a struct with one field per parameter, and a
//! `corten::view::Render` impl that runs the function body when the struct renders.

use proc_macro::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};

use crate::tokens::{err, is_ident, is_punct, q, stream, Error};

pub fn expand(args: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    if let Some(token) = args.into_iter().next() {
        return err(token.span(), "#[component] takes no arguments");
    }
    let tokens: Vec<TokenTree> = item.into_iter().collect();
    let mut i = 0;
    let mut attrs = Vec::new();
    let mut vis = Vec::new();
    // Attributes and doc comments, then the visibility.
    loop {
        match tokens.get(i) {
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                attrs.extend(tokens[i..i + 2].iter().cloned());
                i += 2;
            }
            Some(TokenTree::Ident(id)) if id.to_string() == "pub" => {
                vis.push(tokens[i].clone());
                i += 1;
                if let Some(TokenTree::Group(g)) = tokens.get(i) {
                    if g.delimiter() == Delimiter::Parenthesis {
                        vis.push(tokens[i].clone());
                        i += 1;
                    }
                }
            }
            _ => break,
        }
    }
    let span = tokens.get(i).map_or(Span::call_site(), TokenTree::span);
    if !is_ident(tokens.get(i), "fn") {
        return err(span, "#[component] goes on a function: #[component] fn Card(title: &str) -> impl Render { html! { ... } }");
    }
    i += 1;
    let name = match tokens.get(i) {
        Some(TokenTree::Ident(name)) => name.clone(),
        _ => return err(span, "expected the component's name"),
    };
    if !name.to_string().starts_with(|c: char| c.is_ascii_uppercase()) {
        return err(name.span(), "a component's name starts with an uppercase letter, so html! can tell <Card> from <div>");
    }
    i += 1;
    // Generics, kept as written for the struct; the impl needs the same list without defaults,
    // and the type needs only the parameter names.
    let mut generics: Vec<TokenTree> = Vec::new();
    if is_punct(tokens.get(i), '<') {
        let mut depth = 0;
        let mut previous_dash = false;
        loop {
            let Some(token) = tokens.get(i) else { return err(span, "unclosed generics") };
            i += 1;
            if let TokenTree::Punct(p) = token {
                match p.as_char() {
                    '<' => depth += 1,
                    '>' if !previous_dash => depth -= 1,
                    _ => {}
                }
                previous_dash = p.as_char() == '-' && p.spacing() == Spacing::Joint;
            } else {
                previous_dash = false;
            }
            generics.push(token.clone());
            if depth == 0 {
                break;
            }
        }
    }
    let params = match tokens.get(i) {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => g.clone(),
        _ => return err(span, "expected the parameter list"),
    };
    i += 1;
    let mut where_clause = Vec::new();
    let mut in_where = false;
    let body = loop {
        match tokens.get(i) {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace && i + 1 == tokens.len() => break g.clone(),
            Some(TokenTree::Ident(id)) if id.to_string() == "where" => {
                in_where = true;
                where_clause.push(tokens[i].clone());
            }
            Some(token) if in_where => where_clause.push(token.clone()),
            Some(_) => {}
            None => return err(span, "expected the function body"),
        }
        i += 1;
    };

    let mut fields = TokenStream::new();
    let mut lets = TokenStream::new();
    let mut elided = false;
    for param in split(params.stream(), ',') {
        let mut p = 0;
        let mut field_attrs = Vec::new();
        while is_punct(param.get(p), '#') {
            field_attrs.extend(param[p..p + 2].iter().cloned());
            p += 2;
        }
        if is_ident(param.get(p), "mut") {
            p += 1;
        }
        let (Some(TokenTree::Ident(field)), true) = (param.get(p), is_punct(param.get(p + 1), ':')) else {
            let span = param.first().map_or(params.span(), TokenTree::span);
            return err(span, "component parameters are `name: Type`");
        };
        let ty = name_elided_lifetimes(param[p + 2..].to_vec(), &mut elided);
        fields.extend(stream(field_attrs));
        fields.extend(stream(vis.iter().cloned()));
        fields.extend(q("__A0: __A1,", &[TokenTree::Ident(field.clone()).into(), ty]));
        lets.extend(q(
            "#[allow(unused_variables)] let __A0 = (&::corten::view::__private::Param(&self.__A0)).__param();",
            &[TokenTree::Ident(field.clone()).into()],
        ));
    }

    if elided {
        // `title: &str` needs a named lifetime in a struct field; every elided one gets the same.
        let mut named = vec![TokenTree::Punct(Punct::new('<', Spacing::Alone))];
        named.extend(lifetime());
        if generics.len() > 2 {
            named.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
            named.extend(generics[1..].iter().cloned());
        } else {
            named.push(TokenTree::Punct(Punct::new('>', Spacing::Alone)));
        }
        generics = named;
    }
    let (impl_generics, type_generics) = split_generics(&generics);
    let mut out = stream(attrs);
    out.extend(stream(vis));
    out.extend(q(
        "struct __A0 __A1 __A2",
        &[TokenTree::Ident(name.clone()).into(), stream(generics.clone()), stream(where_clause.clone())],
    ));
    out.extend([TokenTree::Group(Group::new(Delimiter::Brace, fields))]);
    let render = q(
        "fn render(&self, __corten_component_out: &mut ::std::string::String) {
            #[allow(unused_imports)]
            use ::corten::view::__private::{ParamCopy as _, ParamRef as _};
            __A0
            ::corten::view::Render::render(&__A1, __corten_component_out)
        }",
        &[lets, TokenTree::Group(body).into()],
    );
    out.extend(q(
        "#[automatically_derived] impl __A0 ::corten::view::Render for __A1 __A2 __A3",
        &[
            impl_generics,
            TokenTree::Ident(Ident::new(&name.to_string(), name.span())).into(),
            type_generics,
            stream(where_clause),
        ],
    ));
    out.extend([TokenTree::Group(Group::new(Delimiter::Brace, render))]);
    Ok(out)
}

fn lifetime() -> [TokenTree; 2] {
    [TokenTree::Punct(Punct::new('\'', Spacing::Joint)), TokenTree::Ident(Ident::new("__corten", Span::call_site()))]
}

/// `&T` becomes `&'__corten T` and `'_` becomes `'__corten`, except inside `Fn(..)` sugar, where
/// elision means something else.
fn name_elided_lifetimes(tokens: Vec<TokenTree>, elided: &mut bool) -> TokenStream {
    let mut out: Vec<TokenTree> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            TokenTree::Punct(p) if p.as_char() == '&' => {
                out.push(tokens[i].clone());
                if !is_punct(tokens.get(i + 1), '\'') {
                    out.extend(lifetime());
                    *elided = true;
                }
            }
            TokenTree::Punct(p) if p.as_char() == '\'' && is_ident(tokens.get(i + 1), "_") => {
                out.extend(lifetime());
                *elided = true;
                i += 1;
            }
            TokenTree::Group(g) => {
                let after_fn = matches!(out.last(), Some(TokenTree::Ident(id)) if matches!(id.to_string().as_str(), "Fn" | "FnMut" | "FnOnce"));
                let inner = if after_fn { g.stream() } else { name_elided_lifetimes(g.stream().into_iter().collect(), elided) };
                let mut copy = Group::new(g.delimiter(), inner);
                copy.set_span(g.span());
                out.push(TokenTree::Group(copy));
            }
            other => out.push(other.clone()),
        }
        i += 1;
    }
    stream(out)
}

/// Splits tokens at top-level `sep`; `<..>` nests (`->` does not close it).
fn split(tokens: TokenStream, sep: char) -> Vec<Vec<TokenTree>> {
    let mut parts = vec![Vec::new()];
    let mut depth = 0i32;
    let mut previous_dash = false;
    for token in tokens {
        if let TokenTree::Punct(p) = &token {
            match p.as_char() {
                '<' => depth += 1,
                '>' if !previous_dash => depth -= 1,
                c if c == sep && depth == 0 => {
                    parts.push(Vec::new());
                    previous_dash = false;
                    continue;
                }
                _ => {}
            }
            previous_dash = p.as_char() == '-' && p.spacing() == Spacing::Joint;
        } else {
            previous_dash = false;
        }
        parts.last_mut().expect("never empty").push(token);
    }
    parts.retain(|p| !p.is_empty());
    parts
}

/// `<'a, T: Render = Foo, const N: usize>` gives the impl list without defaults
/// (`<'a, T: Render, const N: usize>`) and the type's arguments (`<'a, T, N>`).
fn split_generics(generics: &[TokenTree]) -> (TokenStream, TokenStream) {
    if generics.len() < 2 {
        return (TokenStream::new(), TokenStream::new());
    }
    let inner: TokenStream = generics[1..generics.len() - 1].iter().cloned().collect();
    let mut impl_params = Vec::new();
    let mut type_params = Vec::new();
    for param in split(inner, ',') {
        let default = param.iter().position(|t| is_punct(Some(t), '='));
        let without_default: Vec<TokenTree> = param[..default.unwrap_or(param.len())].to_vec();
        let name: Vec<TokenTree> = match param.first() {
            Some(TokenTree::Punct(p)) if p.as_char() == '\'' => param[..2].to_vec(),
            Some(TokenTree::Ident(id)) if id.to_string() == "const" => param[1..2].to_vec(),
            _ => param[..1].to_vec(),
        };
        impl_params.push(stream(without_default));
        type_params.push(stream(name));
    }
    let join = |parts: Vec<TokenStream>| {
        let mut out = q("<", &[]);
        for (i, part) in parts.into_iter().enumerate() {
            if i > 0 {
                out.extend(q(",", &[]));
            }
            out.extend(part);
        }
        out.extend(q(">", &[]));
        out
    };
    (join(impl_params), join(type_params))
}
