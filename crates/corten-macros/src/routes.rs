//! `#[corten::routes]` on a module of routes: each `#[route("/..", get = show)]` struct in it is a
//! typed route (`#[derive(Route)]`) with the handlers that serve it, so a route with no handler
//! does not compile; `router()` puts each handler at its route's path. A route another router
//! serves (an area, an API) says so: `#[route("/admin", link)]`.

use proc_macro::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};

use crate::tokens::{err, q, stream, Error};

/// The methods a route can name a handler for: axum's `routing` functions.
const METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "head", "options", "trace"];

/// A route the module routes: its struct's name, path and handlers.
struct Routed {
    name: Ident,
    path: String,
    span: Span,
    /// Each method with the path of its handler, as written.
    handlers: Vec<(Ident, TokenStream)>,
}

/// What a struct's `#[route(..)]` says beyond its path.
#[derive(Default)]
struct Serves {
    handlers: Vec<(Ident, TokenStream)>,
    link: Option<Span>,
}

pub fn expand(args: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    // `state = Market`: what the handlers share, which `router(state)` takes.
    let args: Vec<TokenTree> = args.into_iter().collect();
    let state = match args.as_slice() {
        [] => None,
        [TokenTree::Ident(key), TokenTree::Punct(eq), ty @ ..]
            if key.to_string() == "state" && eq.as_char() == '=' && !ty.is_empty() =>
        {
            Some(stream(ty.iter().cloned()))
        }
        _ => {
            return err(
                args[0].span(),
                "#[corten::routes] takes the state the handlers share, if any: #[corten::routes(state = Db)]",
            )
        }
    };
    let mut tokens: Vec<TokenTree> = item.into_iter().collect();
    let Some(TokenTree::Group(body)) = tokens.last().cloned().filter(|_| is_mod(&tokens)) else {
        let span = tokens.first().map_or(Span::call_site(), TokenTree::span);
        return err(
            span,
            "#[corten::routes] goes on a module of routes: #[corten::routes] mod url { #[route(\"/\", get = home)] pub struct Home; }",
        );
    };
    let TokenTree::Ident(module) = tokens[tokens.len() - 2].clone() else { unreachable!("is_mod checked") };
    let (mut items, routed) = scan(body.stream())?;
    for (i, a) in routed.iter().enumerate() {
        if let Some(b) = routed[..i].iter().find(|b| same_pattern(&a.path, &b.path)) {
            return err(
                a.span,
                format!("{} and {} have the same path, {}: a request could only reach one", b.name, a.name, a.path),
            );
        }
    }
    // The router is written where the module is declared, so a handler is named from there, where
    // handlers live (`get = home`), with no `use super::*` in the module. It is an impl of a trait
    // the module declares: `super::` cannot reach a function body the module is declared in (a
    // test's), and an impl applies wherever it is written.
    // Each handler is checked against the module's state, named for every call, so one that wants
    // another state is an error at its name rather than a mismatch between routers.
    let state_ty = state.clone().unwrap_or_else(|| q("()", &[]));
    let mut calls = TokenStream::new();
    for route in &routed {
        let mut methods = TokenStream::new();
        for (i, (method, handler)) in route.handlers.iter().enumerate() {
            let at = handler.clone().into_iter().last().map_or(method.span(), |t| t.span());
            let call = respan(
                q("::corten::route::__private::__A0::<_, _, __A1>", &[TokenTree::Ident(method.clone()).into(), state_ty.clone()]),
                at,
            );
            let mut call = call;
            call.extend([TokenTree::Group(Group::new(Delimiter::Parenthesis, handler.clone()))]);
            if i > 0 {
                methods.extend([
                    TokenTree::Punct(Punct::new('.', Spacing::Alone)),
                    TokenTree::Ident(Ident::new("merge", Span::call_site())),
                ]);
                methods.extend([TokenTree::Group(Group::new(Delimiter::Parenthesis, call))]);
            } else {
                methods.extend(call);
            }
        }
        calls.extend(q(
            ".route(<__A0::__A1 as ::corten::Route>::PATH, __A2)",
            &[TokenTree::Ident(module.clone()).into(), TokenTree::Ident(route.name.clone()).into(), methods],
        ));
    }
    let router = match &state {
        None => q(
            "/// The routes of this module, each at its path with its handlers: what `App::new` takes.
            #[allow(dead_code)]
            pub fn router() -> ::corten::axum::Router {
                <__CortenRoutes as __CortenRouter<()>>::__router()
            }",
            &[],
        ),
        // The state's type is written where the module is declared, so it is named there, in the
        // impl; `S` here is the one type that impl is for.
        Some(_) => q(
            "/// The routes of this module, each at its path with its handlers, which share `state`
            /// (`State<..>` in a handler): what `App::new` takes.
            #[allow(dead_code)]
            pub fn router<S>(state: S) -> ::corten::axum::Router
            where
                S: ::core::clone::Clone + ::core::marker::Send + ::core::marker::Sync + 'static,
                __CortenRoutes: __CortenRouter<S>,
            {
                <__CortenRoutes as __CortenRouter<S>>::__router().with_state(state)
            }",
            &[],
        ),
    };
    items.extend(router);
    items.extend(q(
        "#[doc(hidden)]
        pub struct __CortenRoutes;

        #[doc(hidden)]
        pub trait __CortenRouter<S> {
            fn __router() -> ::corten::axum::Router<S>;
        }",
        &[],
    ));
    let mut module_body = Group::new(Delimiter::Brace, items);
    module_body.set_span(body.span());
    *tokens.last_mut().expect("the module body") = TokenTree::Group(module_body);
    let mut out: TokenStream = tokens.into_iter().collect();
    let state = state.unwrap_or_else(|| q("()", &[]));
    out.extend(q(
        "#[automatically_derived]
        impl __A0::__CortenRouter<__A2> for __A0::__CortenRoutes {
            fn __router() -> ::corten::axum::Router<__A2> {
                ::corten::axum::Router::new() __A1
            }
        }",
        &[TokenTree::Ident(module.clone()).into(), calls, state],
    ));
    Ok(out)
}

/// The tokens with every span set to `span`, so an error in them points there.
fn respan(tokens: TokenStream, span: Span) -> TokenStream {
    tokens
        .into_iter()
        .map(|mut token| {
            if let TokenTree::Group(g) = &token {
                let mut copy = Group::new(g.delimiter(), respan(g.stream(), span));
                copy.set_span(span);
                token = TokenTree::Group(copy);
            } else {
                token.set_span(span);
            }
            token
        })
        .collect()
}

/// `[attributes] [pub[(..)]] mod name { .. }`.
fn is_mod(tokens: &[TokenTree]) -> bool {
    let n = tokens.len();
    n >= 3
        && matches!(&tokens[n - 3], TokenTree::Ident(i) if i.to_string() == "mod")
        && matches!(&tokens[n - 2], TokenTree::Ident(_))
}

/// The module's items with `#[derive(::corten::Route)]` on each routed struct and its
/// `#[route(..)]` cut to the path, and those structs with their handlers.
fn scan(body: TokenStream) -> Result<(TokenStream, Vec<Routed>), Error> {
    let tokens: Vec<TokenTree> = body.into_iter().collect();
    let mut out: Vec<TokenTree> = Vec::new();
    let mut routed = Vec::new();
    // Where the current item's attributes start in `out`, whether one derives `Route`, and its
    // route's path and what it serves.
    let mut start = None;
    let mut derives = None;
    let mut route: Option<(String, Span, Serves)> = None;
    let mut i = 0;
    while i < tokens.len() {
        match (&tokens[i], tokens.get(i + 1)) {
            (TokenTree::Punct(p), Some(TokenTree::Group(g))) if p.as_char() == '#' && g.delimiter() == Delimiter::Bracket => {
                start.get_or_insert(out.len());
                if derives_route(g) {
                    derives = Some(g.span());
                }
                let attr = match route_attr(g)? {
                    Some((path, span, serves, cut)) => {
                        route = Some((path, span, serves));
                        cut
                    }
                    None => g.clone(),
                };
                out.extend([tokens[i].clone(), TokenTree::Group(attr)]);
                i += 2;
                continue;
            }
            (TokenTree::Ident(id), next) if id.to_string() == "pub" => {
                start.get_or_insert(out.len());
                out.push(tokens[i].clone());
                i += 1;
                if let Some(TokenTree::Group(g)) = next {
                    if g.delimiter() == Delimiter::Parenthesis {
                        out.push(tokens[i].clone());
                        i += 1;
                    }
                }
                continue;
            }
            (TokenTree::Ident(id), Some(TokenTree::Ident(name))) if id.to_string() == "struct" => {
                if let Some((path, span, serves)) = route.take() {
                    if let Some(derive) = derives {
                        return err(
                            derive,
                            format!(
                                "in a #[corten::routes] module the module derives Route for {name}: remove #[derive(Route)] (a route another router serves is #[route(\"{path}\", link)])"
                            ),
                        );
                    }
                    match (serves.handlers.is_empty(), serves.link) {
                        (true, None) => {
                            return err(
                                span,
                                format!(
                                    "{name} has no handler: name the one that serves it, #[route(\"{path}\", get = {})], or write #[route(\"{path}\", link)] if an area or API serves it",
                                    crate::names::snake(&name.to_string())
                                ),
                            )
                        }
                        (false, Some(link)) => {
                            return err(link, format!("{name} names a handler, so it is not only a link: remove link"))
                        }
                        _ => {}
                    }
                    let at = start.unwrap_or(out.len());
                    let derive: Vec<TokenTree> = q("#[derive(::corten::Route)]", &[]).into_iter().collect();
                    out.splice(at..at, derive);
                    if serves.link.is_none() {
                        routed.push(Routed { name: name.clone(), path, span, handlers: serves.handlers });
                    }
                }
            }
            _ => {}
        }
        start = None;
        derives = None;
        route = None;
        out.push(tokens[i].clone());
        i += 1;
    }
    Ok((out.into_iter().collect(), routed))
}

/// `#[route("/..", get = show, post = save)]` or `#[route("/..", link)]`: the path, its span, what
/// it serves, and the attribute cut to `#[route("/..")]` for `#[derive(Route)]`. `None` for
/// another attribute, and for `#[route(query)]`, which has no path.
fn route_attr(attr: &Group) -> Result<Option<(String, Span, Serves, Group)>, Error> {
    let inner: Vec<TokenTree> = attr.stream().into_iter().collect();
    let [TokenTree::Ident(name), TokenTree::Group(args)] = inner.as_slice() else { return Ok(None) };
    if name.to_string() != "route" || args.delimiter() != Delimiter::Parenthesis {
        return Ok(None);
    }
    let args: Vec<TokenTree> = args.stream().into_iter().collect();
    let Some(TokenTree::Literal(lit)) = args.first() else { return Ok(None) };
    let text = lit.to_string();
    let Some(path) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) else {
        return err(lit.span(), "a route's path is a string: #[route(\"/items/{id}\", get = show)]");
    };
    let mut serves = Serves::default();
    let mut rest = &args[1..];
    while let Some(first) = rest.first() {
        if !matches!(first, TokenTree::Punct(p) if p.as_char() == ',') {
            return err(first.span(), "expected a comma, then a handler (get = show) or link");
        }
        rest = &rest[1..];
        let end = rest.iter().position(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ',')).unwrap_or(rest.len());
        let (part, after) = rest.split_at(end);
        rest = after;
        match part {
            [] => {}
            [TokenTree::Ident(word)] if word.to_string() == "link" => serves.link = Some(word.span()),
            [TokenTree::Ident(method), TokenTree::Punct(eq), handler @ ..] if eq.as_char() == '=' => {
                let key = method.to_string();
                if !METHODS.contains(&key.as_str()) {
                    return err(method.span(), format!("{key} is not a method: one of {}", METHODS.join(", ")));
                }
                if serves.handlers.iter().any(|(m, _)| m.to_string() == key) {
                    return err(method.span(), format!("{key} is named twice: a route has one handler per method"));
                }
                if handler.is_empty() || !is_path(handler) {
                    let span = handler.first().map_or(method.span(), TokenTree::span);
                    return err(span, format!("name a handler function: {key} = show (a path, not a closure)"));
                }
                serves.handlers.push((method.clone(), stream(handler.iter().cloned())));
            }
            _ => return err(part[0].span(), "expected a handler (get = show) or link"),
        }
    }
    let mut cut_args = Group::new(Delimiter::Parenthesis, stream([TokenTree::Literal(lit.clone())]));
    cut_args.set_span(attr.span());
    let mut cut = Group::new(Delimiter::Bracket, stream([TokenTree::Ident(name.clone()), TokenTree::Group(cut_args)]));
    cut.set_span(attr.span());
    Ok(Some((path.to_string(), lit.span(), serves, cut)))
}

/// `show`, `items::show`, `crate::items::show`: identifiers joined by `::`.
fn is_path(tokens: &[TokenTree]) -> bool {
    tokens.iter().all(|t| match t {
        TokenTree::Ident(_) => true,
        TokenTree::Punct(p) => p.as_char() == ':',
        _ => false,
    }) && matches!(tokens.last(), Some(TokenTree::Ident(_)))
}

/// `#[derive(.., Route, ..)]`, by any path ending in `Route`.
fn derives_route(attr: &Group) -> bool {
    let inner: Vec<TokenTree> = attr.stream().into_iter().collect();
    match inner.as_slice() {
        [TokenTree::Ident(name), TokenTree::Group(list)] if name.to_string() == "derive" => {
            list.stream().into_iter().any(|t| matches!(t, TokenTree::Ident(id) if id.to_string() == "Route"))
        }
        _ => false,
    }
}

/// Two paths a request matches the same way: equal once each `{param}` is just a parameter.
fn same_pattern(a: &str, b: &str) -> bool {
    fn shape(path: &str) -> String {
        let mut out = String::new();
        let mut in_param = false;
        for c in path.chars() {
            match c {
                '{' => {
                    in_param = true;
                    out.push('{');
                }
                '}' => {
                    in_param = false;
                    out.push('}');
                }
                '*' if in_param => out.push('*'),
                _ if in_param => {}
                _ => out.push(c),
            }
        }
        out
    }
    shape(a) == shape(b)
}
