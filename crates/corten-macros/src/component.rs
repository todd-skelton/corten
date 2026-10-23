//! `#[component]`: a function becomes a struct with one field per parameter, and a
//! `corten::view::Render` impl that runs the function body when the struct renders.
//!
//! With `css`, the component has a stylesheet next to it (see `crate::style`); with `js`, a script
//! of handlers (see `crate::script`); with `view`, a class scripts write its holes through (see
//! `crate::html::view_of`). Any of them gives it a constant with its name in the value
//! namespace, `OfferList: corten::Component`, which the app lists once in `App::components`.

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

/// What `#[component(..)]` takes: `css` or `css = Owner`, and `js`.
#[derive(Default)]
pub struct Args {
    pub css: Option<CssArg>,
    /// `js`: the handlers are the script next to the component, `{name}.js`.
    pub js: Option<Span>,
    /// `view`: scripts get a class with the component's holes, in `corten/views`.
    pub view: Option<Span>,
}

/// Parses `#[component]`, `#[component(css)]`, `#[component(css = Owner)]`, `#[component(js)]`,
/// `#[component(view)]` or several, comma-separated.
pub fn args(args: TokenStream) -> Result<Args, Error> {
    let usage = "#[component] takes css (a stylesheet named after the component, next to it), css = OtherComponent, js (a script named after it) and view (a class scripts write its holes through)";
    let mut out = Args::default();
    for part in split(args, ',') {
        match part.as_slice() {
            [TokenTree::Ident(css)] if css.to_string() == "css" && out.css.is_none() => out.css = Some(CssArg::Own(css.span())),
            [TokenTree::Ident(css), TokenTree::Punct(eq), TokenTree::Ident(owner)]
                if css.to_string() == "css" && eq.as_char() == '=' && out.css.is_none() =>
            {
                out.css = Some(CssArg::Of(owner.clone()))
            }
            [TokenTree::Ident(js)] if js.to_string() == "js" && out.js.is_none() => out.js = Some(js.span()),
            [TokenTree::Ident(view)] if view.to_string() == "view" && out.view.is_none() => out.view = Some(view.span()),
            [first, ..] => return err(first.span(), usage),
            [] => {}
        }
    }
    Ok(out)
}

pub fn expand(args: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    let args = self::args(args)?;
    build(item, args)
}

/// The struct, its `Render` impl and, for a component with a script or stylesheet, the constant
/// the app lists.
pub fn build(item: TokenStream, args: Args) -> Result<TokenStream, Error> {
    let what = "#[component]";
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

    let mut fields = TokenStream::new();
    let mut lets = TokenStream::new();
    let mut elided = false;
    // Each parameter's name and type as written, for a view's holes.
    let mut typed: Vec<(String, Vec<TokenTree>)> = Vec::new();
    for param in split(params.stream(), ',') {
        let mut p = 0;
        let mut field_attrs = Vec::new();
        while is_punct(param.get(p), '#') {
            if is_prop(&param[p..p + 2]) {
                return err(
                    param[p].span(),
                    "#[prop] is gone: pass the value to the handler that uses it, on:connect={js::start(signal, value)}",
                );
            }
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
        typed.push((field.to_string(), param[p + 2..].to_vec()));
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

    // Refs (`let (rows, rest) = refs();`) and the scope they belong to: the component's one root
    // element, marked `data-scope` by its html!.
    let scope_name = match crate::script::prefix() {
        Some(prefix) => format!("{prefix}-{}", crate::names::kebab(&name.to_string())),
        None => crate::names::kebab(&name.to_string()),
    };
    let view = match args.view {
        Some(span) => Some(crate::html::view_of(&body, &name.to_string(), &typed, span)?),
        None => None,
    };
    // A view's number property is a JavaScript number in the page: debug builds stop a value it
    // cannot hold exactly (a u64 past 2^53), as for a handler's number argument.
    for field in view.iter().flat_map(|v| &v.numbers) {
        lets.extend(q(
            "::corten::script::__private::ViewNumber::check(&self.__A0, __A1);",
            &[TokenTree::Ident(Ident::new(field, Span::call_site())).into(), literal(field)],
        ));
    }
    let (body, has_refs) = refs(&body, &scope_name)?;
    let scoped = has_refs || mentions(&body.stream(), "scope");
    let body = if scoped { mark_root(&body, &scope_name)? } else { body };

    let script = match args.js {
        Some(span) => {
            let file = format!("{}.js", crate::names::snake(&name.to_string()));
            let path = crate::style::locate(&file, span)?;
            Some(crate::script::Module::read(&path, span)?)
        }
        None => None,
    };

    let css = args.css;
    let css_owner = match &css {
        Some(CssArg::Of(owner)) => Some(owner.clone()),
        _ => None,
    };
    let (css, own_css) = match &css {
        None => (None, false),
        Some(CssArg::Own(span)) => (Some(Css::read(&name, *span)?), true),
        Some(CssArg::Of(owner)) => {
            if owner.to_string() == name.to_string() {
                return err(owner.span(), "css = names another component; for this component's own stylesheet write css");
            }
            (Some(Css::read(owner, owner.span())?), false)
        }
    };
    let mut used = Vec::new();
    let body = match &css {
        Some(css) => {
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
    let mut scope = TokenStream::new();
    let mut items = TokenStream::new();
    // The constant the app lists: the component's script and its own stylesheet.
    if let Some(css) = &css {
        // A helper (`css = Owner`) uses the owner's constants, so its uses count for the owner.
        let style_owner = css_owner.clone().unwrap_or_else(|| name.clone());
        if own_css {
            items.extend(css.items(&name));
        }
        scope.extend(css.scope(&style_owner, &used));
    }
    let has_constant = own_css || script.is_some() || view.is_some();
    if let Some(module) = &script {
        // Inside `render`, so two components in one file each have their own `js`.
        scope.extend(module.module(
            TokenStream::new(),
            Ident::new("js", Span::call_site()),
            q("::core::option::Option::None", &[]),
            TokenStream::new(),
        ));
    }
    let owner = if has_constant { Some(name.clone()) } else { css_owner };
    if has_constant {
        let some = |t: Option<TokenStream>| match t {
            Some(t) => q("::core::option::Option::Some(__A0)", &[t]),
            None => q("::core::option::Option::None", &[]),
        };
        let style = if own_css { css.as_ref().map(Css::style) } else { None };
        let doc = format!(" The script, stylesheet and view of the `{name}` component, for `App::components`.");
        let view = view.as_ref().map(|v| {
            q(
                "::corten::script::ViewDef { class: __A0, js: __A1, ts: __A2 }",
                &[literal(&name.to_string()), literal(&v.js), literal(&v.ts)],
            )
        });
        items.extend(q(
            "#[doc = __A0] #[allow(non_upper_case_globals, dead_code)] __A5 const __A1: ::corten::Component = ::corten::Component {
                name: __A2,
                script: __A3,
                style: __A4,
                event: ::core::option::Option::None,
                view: __A6,
            };",
            &[
                literal(&doc),
                TokenTree::Ident(name.clone()).into(),
                literal(&name.to_string()),
                some(script.as_ref().map(crate::script::Module::literal)),
                some(style),
                stream(vis.iter().cloned()),
                some(view),
            ],
        ));
    }
    let used = match owner {
        Some(owner) => {
            q("#[cfg(debug_assertions)] ::corten::style::__private::rendered(&__A0);", &[TokenTree::Ident(owner).into()])
        }
        None => TokenStream::new(),
    };
    let draw = if scoped {
        q(
            "::corten::script::__private::scoped(__A1, || ::corten::view::Render::render(&__A0, __corten_component_out));",
            &[TokenTree::Group(body).into(), literal(&scope_name)],
        )
    } else {
        q("::corten::view::Render::render(&__A0, __corten_component_out);", &[TokenTree::Group(body).into()])
    };
    let render = q(
        "fn render(&self, __corten_component_out: &mut ::std::string::String) {
            #[allow(unused_imports)]
            use ::corten::view::__private::{ParamCopy as _, ParamRef as _};
            __A0
            __A1
            __A3
            __A2
        }",
        &[lets, scope, draw, used],
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

/// True for the parameter attribute `#[prop]` (no longer supported).
fn is_prop(attr: &[TokenTree]) -> bool {
    matches!(attr, [TokenTree::Punct(p), TokenTree::Group(g)] if p.as_char() == '#' && {
        let inner: Vec<TokenTree> = g.stream().into_iter().collect();
        inner.len() == 1 && is_ident(inner.first(), "prop")
    })
}

/// Replaces `let (rows, rest) = refs();` at the top level of the body with each ref, named and
/// scoped to this component. Returns whether there were any.
fn refs(body: &Group, scope: &str) -> Result<(Group, bool), Error> {
    let tokens: Vec<TokenTree> = body.stream().into_iter().collect();
    let mut out: Vec<TokenTree> = Vec::with_capacity(tokens.len());
    let mut found = false;
    let mut i = 0;
    while i < tokens.len() {
        let is_refs_call = |at: usize| {
            is_ident(tokens.get(at), "refs")
                && matches!(tokens.get(at + 1), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis && g.stream().is_empty())
        };
        if is_ident(tokens.get(i), "let") && is_punct(tokens.get(i + 2), '=') && is_refs_call(i + 3) {
            let names: Vec<Ident> = match &tokens[i + 1] {
                TokenTree::Ident(one) => vec![one.clone()],
                TokenTree::Group(g) if g.delimiter() == Delimiter::Parenthesis => {
                    let mut list = Vec::new();
                    for part in split(g.stream(), ',') {
                        match part.as_slice() {
                            [TokenTree::Ident(n)] => list.push(n.clone()),
                            [first, ..] => return err(first.span(), "refs() names plain variables: let (rows, rest) = refs();"),
                            [] => {}
                        }
                    }
                    list
                }
                other => return err(other.span(), "refs() names plain variables: let (rows, rest) = refs();"),
            };
            for n in &names {
                let text = n.to_string();
                if !text.starts_with(|c: char| c.is_ascii_lowercase())
                    || !text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    return err(n.span(), format!("{text}: a ref is a snake_case variable, its name in the page"));
                }
            }
            let make = |n: &Ident| {
                q("::corten::script::__private::new_ref::<_, _>(__A0, __A1)", &[literal(scope), literal(&n.to_string())])
            };
            let value = if names.len() == 1 && matches!(tokens[i + 1], TokenTree::Ident(_)) {
                make(&names[0])
            } else {
                let mut list = TokenStream::new();
                for n in &names {
                    list.extend(make(n));
                    list.extend(q(",", &[]));
                }
                crate::tokens::parens(list)
            };
            out.extend(tokens[i..i + 3].iter().cloned());
            out.extend(value);
            found = true;
            i += 5;
            continue;
        }
        if is_refs_call(i) {
            return err(tokens[i].span(), "refs() goes at the top of a component's body: let (rows, rest) = refs();");
        }
        out.push(tokens[i].clone());
        i += 1;
    }
    let mut copy = Group::new(Delimiter::Brace, stream(out));
    copy.set_span(body.span());
    Ok((copy, found))
}

/// True when `ident` appears anywhere in `tokens`.
fn mentions(tokens: &TokenStream, ident: &str) -> bool {
    tokens.clone().into_iter().any(|t| match t {
        TokenTree::Ident(i) => i.to_string() == ident,
        TokenTree::Group(g) => mentions(&g.stream(), ident),
        _ => false,
    })
}

/// Tells the body's final `html! { .. }` to mark its root element with the component's scope.
fn mark_root(body: &Group, scope: &str) -> Result<Group, Error> {
    let mut tokens: Vec<TokenTree> = body.stream().into_iter().collect();
    let n = tokens.len();
    let at_end = n >= 3
        && is_ident(tokens.get(n - 3), "html")
        && is_punct(tokens.get(n - 2), '!')
        && matches!(tokens.get(n - 1), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace);
    if !at_end {
        return err(
            body.span(),
            "a component with refs ends with html! { <root> .. </root> }: the root element is where its refs end",
        );
    }
    let TokenTree::Group(g) = &tokens[n - 1] else { unreachable!() };
    let mut inner = q("@ scope __A0 ;", &[literal(scope)]);
    inner.extend(g.stream());
    let mut marked = Group::new(Delimiter::Brace, inner);
    marked.set_span(g.span());
    tokens[n - 1] = TokenTree::Group(marked);
    let mut copy = Group::new(Delimiter::Brace, stream(tokens));
    copy.set_span(body.span());
    Ok(copy)
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
