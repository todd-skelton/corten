//! `#[component]`: a function becomes a struct with one field per parameter, and a
//! `corten::view::Render` impl that runs the function body when the struct renders.
//!
//! With `css`, the component has a stylesheet next to it (see `crate::style`); an island or a
//! component with its own stylesheet also gets a constant with its name in the value namespace,
//! `OfferList: corten::Component`, which the app lists once in `App::components`.

use proc_macro::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};

use crate::{
    style::Css,
    tokens::{err, is_ident, is_punct, literal, q, stream, Error},
};

/// `css` (the component's own `{name}.css`) or `css = Owner` (another component's, in the
/// same module).
pub enum CssArg {
    Own(Span),
    Of(Ident),
}

/// Parses `#[component]`, `#[component(css)]` or `#[component(css = Owner)]`.
pub fn args(args: TokenStream, what: &str) -> Result<Option<CssArg>, Error> {
    let tokens: Vec<TokenTree> = args.into_iter().collect();
    let usage = format!("{what} takes nothing, css (a stylesheet named after the component, next to it) or css = OtherComponent");
    match tokens.as_slice() {
        [] => Ok(None),
        [TokenTree::Ident(css)] if css.to_string() == "css" => Ok(Some(CssArg::Own(css.span()))),
        [TokenTree::Ident(css), TokenTree::Punct(eq), TokenTree::Ident(owner)]
            if css.to_string() == "css" && eq.as_char() == '=' =>
        {
            Ok(Some(CssArg::Of(owner.clone())))
        }
        [first, ..] => err(first.span(), usage),
    }
}

pub fn expand(args: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    let css = self::args(args, "#[component]")?;
    build(item, false, css)
}

/// `#[component]`, or with `island`, `#[island]`: the same struct, wrapped in the island's tag,
/// with the island's names in scope and its description next to it (see `crate::island`).
pub fn build(item: TokenStream, island: bool, css: Option<CssArg>) -> Result<TokenStream, Error> {
    let what = if island { "#[island]" } else { "#[component]" };
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
        return err(span, format!("{what} goes on a function: {what} fn Card(title: &str) -> impl Render {{ html! {{ ... }} }}"));
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

    if island && generics.len() > 2 {
        for param in split(stream(generics[1..generics.len() - 1].iter().cloned()), ',') {
            if !is_punct(param.first(), '\'') {
                return err(param[0].span(), "an #[island] takes lifetime parameters only, so its constant names one island");
            }
        }
    }
    let mut fields = TokenStream::new();
    let mut lets = TokenStream::new();
    let mut elided = false;
    let mut props = Vec::new();
    for param in split(params.stream(), ',') {
        let mut p = 0;
        let mut field_attrs = Vec::new();
        let mut is_prop = false;
        while is_punct(param.get(p), '#') {
            if crate::island::is_prop(&param[p..p + 2]) {
                if !island {
                    return err(param[p].span(), "#[prop] sends a parameter to an island's script; use it in an #[island]");
                }
                is_prop = true;
            } else {
                field_attrs.extend(param[p..p + 2].iter().cloned());
            }
            p += 2;
        }
        if is_ident(param.get(p), "mut") {
            p += 1;
        }
        let (Some(TokenTree::Ident(field)), true) = (param.get(p), is_punct(param.get(p + 1), ':')) else {
            let span = param.first().map_or(params.span(), TokenTree::span);
            return err(span, "component parameters are `name: Type`");
        };
        if is_prop {
            props.push(crate::island::Prop { field: field.clone(), ty: param[p + 2..].to_vec() });
        }
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
    let parts = if island { Some(crate::island::parts(&name, &body.stream(), &props)?) } else { None };
    let tag = parts.as_ref().map(|_| crate::island::kebab(&name.to_string()));
    let css_owner = match &css {
        Some(CssArg::Of(owner)) => Some(owner.clone()),
        _ => None,
    };
    let (css, own_css) = match &css {
        None => (None, false),
        Some(CssArg::Own(span)) => (Some(Css::read(&name, tag.as_deref(), *span)?), true),
        Some(CssArg::Of(owner)) => {
            if owner.to_string() == name.to_string() {
                return err(owner.span(), "css = names another component; for this component's own stylesheet write css");
            }
            (Some(Css::read(owner, None, owner.span())?), false)
        }
    };
    let mut used = Vec::new();
    let body = match &css {
        Some(css) => {
            if let Some(parts) = &parts {
                if let Some(clash) = css.idents.iter().find(|i| parts.names.contains(i)) {
                    return err(name.span(), format!("{clash} is a class in {} and a ref or action; rename one", css.file));
                }
            }
            let mut copy = Group::new(Delimiter::Brace, css.rewrite(body.stream(), false, &mut used)?);
            copy.set_span(body.span());
            copy
        }
        None => body,
    };
    let mut out = stream(attrs);
    out.extend(stream(vis.iter().cloned()));
    out.extend(q(
        "struct __A0 __A1 __A2",
        &[TokenTree::Ident(name.clone()).into(), stream(generics.clone()), stream(where_clause.clone())],
    ));
    out.extend([TokenTree::Group(Group::new(Delimiter::Brace, fields))]);
    let (mut scope, open, close, mut items, island_literal) = match parts {
        Some(p) => (p.scope, p.open, p.close, p.items, Some(p.island)),
        None => Default::default(),
    };
    // The constant the app lists: the island's script and the component's own stylesheet.
    if let Some(css) = &css {
        // A helper (`css = Owner`) uses the owner's constants, so its uses count for the owner.
        let style_owner = css_owner.clone().unwrap_or_else(|| name.clone());
        if own_css {
            items.extend(css.items(&name));
        }
        scope.extend(css.scope(&style_owner, &used));
    }
    let owner = if own_css || island_literal.is_some() { Some(name.clone()) } else { css_owner };
    if own_css || island_literal.is_some() {
        let some = |t: Option<TokenStream>| match t {
            Some(t) => q("::core::option::Option::Some(__A0)", &[t]),
            None => q("::core::option::Option::None", &[]),
        };
        let style = if own_css { css.as_ref().map(Css::style) } else { None };
        let doc = format!(" The script and stylesheet of the `{name}` component, for `App::components`.");
        items.extend(q(
            "#[doc = __A0] #[allow(non_upper_case_globals, dead_code)] __A5 const __A1: ::corten::Component = ::corten::Component {
                name: __A2,
                island: __A3,
                style: __A4,
            };",
            &[
                literal(&doc),
                TokenTree::Ident(name.clone()).into(),
                literal(&name.to_string()),
                some(island_literal),
                some(style),
                stream(vis.iter().cloned()),
            ],
        ));
    }
    let used = match owner {
        Some(owner) => {
            q("#[cfg(debug_assertions)] ::corten::style::__private::rendered(&__A0);", &[TokenTree::Ident(owner).into()])
        }
        None => TokenStream::new(),
    };
    let render = q(
        "fn render(&self, __corten_component_out: &mut ::std::string::String) {
            #[allow(unused_imports)]
            use ::corten::view::__private::{ParamCopy as _, ParamRef as _};
            __A5
            __A2
            __A0
            __A3
            ::corten::view::Render::render(&__A1, __corten_component_out);
            __A4
        }",
        &[lets, TokenTree::Group(body).into(), scope, open, close, used],
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
    out.extend(items);
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
pub(crate) fn split(tokens: TokenStream, sep: char) -> Vec<Vec<TokenTree>> {
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
pub(crate) fn split_generics(generics: &[TokenTree]) -> (TokenStream, TokenStream) {
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
