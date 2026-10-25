//! `#[corten::routes]` on a module of routes: each `#[route("/..")]` struct in it is a typed route
//! (`#[derive(Route)]`) and a method of the module's `Handlers` trait, so a route with no handler
//! does not compile; `router(handlers)` puts each method's `MethodRouter` at its route's path, or with
//! `#[corten::routes(area = Admin)]`, `area(area, handlers)` does in that area.

use proc_macro::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};

use crate::{
    names::snake,
    tokens::{err, q, Error},
};

/// A route the module routes: its struct's name and path.
struct Routed {
    name: Ident,
    path: String,
    span: Span,
}

pub fn expand(args: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    let area = area_arg(args)?;
    let tokens: Vec<TokenTree> = item.into_iter().collect();
    let Some(TokenTree::Group(body)) = tokens.last().cloned().filter(|_| is_mod(&tokens)) else {
        let span = tokens.first().map_or(Span::call_site(), TokenTree::span);
        return err(
            span,
            "#[corten::routes] goes on a module of routes: #[corten::routes] mod url { #[route(\"/\")] pub struct Home; }",
        );
    };
    let (mut items, routed) = scan(body.stream())?;
    for (i, a) in routed.iter().enumerate() {
        if let Some(b) = routed[..i].iter().find(|b| same_pattern(&a.path, &b.path)) {
            return err(
                a.span,
                format!("{} and {} have the same path, {}: a request could only reach one", b.name, a.name, a.path),
            );
        }
        if a.name.to_string() == "Handlers" {
            return err(a.name.span(), "a route named Handlers would hide the module's Handlers trait: rename it");
        }
    }
    let mut methods = TokenStream::new();
    let mut calls = TokenStream::new();
    for route in &routed {
        let method = TokenTree::Ident(Ident::new(&snake(&route.name.to_string()), route.name.span()));
        let doc =
            format!("The handler of `{}` ([`{}`]): `get(page)`, `post(save)`, `get(page).post(save)`.", route.path, route.name);
        methods.extend(q(
            "#[doc = __A0] fn __A1(&self) -> ::corten::axum::routing::MethodRouter;",
            &[TokenTree::Literal(proc_macro::Literal::string(&doc)).into(), method.clone().into()],
        ));
        calls.extend(q(
            ".route(<__A0 as ::corten::Route>::PATH, Handlers::__A1(&handlers))",
            &[TokenTree::Ident(route.name.clone()).into(), method.into()],
        ));
    }
    let Some(area) = area else {
        items.extend(q(
            "/// The handler of each route in this module, one method per route: implement it for the
            /// app (a unit struct, or one that holds what the handlers share) and pass that to [`router`].
            /// A route with no handler is a compile error.
            pub trait Handlers {
                __A0
            }

            /// The routes of this module, each at its path with its handler: what `App::new` takes.
            #[allow(dead_code)]
            pub fn router(handlers: impl Handlers) -> ::corten::axum::Router {
                ::corten::axum::Router::new() __A1
            }",
            &[methods, calls],
        ));
        return Ok(finish(tokens, body, items));
    };
    // The area's prefix, when its route is in the module: each route is checked against it here,
    // where the error can point at the route (`Area::route` checks every route when it builds).
    let name = area.to_string().replace(' ', "");
    if let Some(prefix) = routed.iter().find(|r| r.name.to_string() == name) {
        if let Some(outside) = routed.iter().find(|r| !under(&prefix.path, &r.path)) {
            return err(
                outside.span,
                format!(
                    "{} is in the area {} ({}), so its path is at or under {}: {}",
                    outside.name, prefix.name, prefix.path, prefix.path, outside.path
                ),
            );
        }
    }
    items.extend(q(
        "/// The handler of each route in this area, one method per route: implement it for the
        /// app (a unit struct, or one that holds what the handlers share) and pass that to [`area`].
        /// A route with no handler is a compile error.
        pub trait Handlers {
            __A0
        }

        /// The routes of this module in their area, each at its path with its handler:
        /// `.area::<Admin>(|area| url::area(area, Pages).guard(..))`. A route outside the area's
        /// prefix is a compile error.
        #[allow(dead_code)]
        pub fn area<C>(area: ::corten::Area<C, __A2>, handlers: impl Handlers) -> ::corten::Area<C, __A2> {
            area __A1
        }",
        &[methods, area_calls(&routed), area],
    ));
    Ok(finish(tokens, body, items))
}

/// The module with its new body.
fn finish(mut tokens: Vec<TokenTree>, body: Group, items: TokenStream) -> TokenStream {
    let mut module = Group::new(Delimiter::Brace, items);
    module.set_span(body.span());
    *tokens.last_mut().expect("the module body") = TokenTree::Group(module);
    tokens.into_iter().collect()
}

/// `area = Admin`: the route whose path is the area's prefix, or nothing.
fn area_arg(args: TokenStream) -> Result<Option<TokenStream>, Error> {
    let args: Vec<TokenTree> = args.into_iter().collect();
    match args.as_slice() {
        [] => Ok(None),
        [TokenTree::Ident(name), TokenTree::Punct(eq), path @ ..]
            if name.to_string() == "area" && eq.as_char() == '=' && !path.is_empty() =>
        {
            Ok(Some(path.iter().cloned().collect()))
        }
        [first, ..] => {
            err(first.span(), "#[corten::routes] takes nothing, or the area its routes are in: #[corten::routes(area = Admin)]")
        }
    }
}

/// `.route::<Item>(Handlers::item(&handlers))` for each route, on an `Area`.
fn area_calls(routed: &[Routed]) -> TokenStream {
    let mut calls = TokenStream::new();
    for route in routed {
        let method = TokenTree::Ident(Ident::new(&snake(&route.name.to_string()), route.name.span()));
        calls.extend(q(
            ".route::<__A0>(Handlers::__A1(&handlers))",
            &[TokenTree::Ident(route.name.clone()).into(), method.into()],
        ));
    }
    calls
}

/// `path` is `prefix` or under it, a segment at a time (`/admin` holds `/admin/users`, not
/// `/admins`).
fn under(prefix: &str, path: &str) -> bool {
    prefix == "/" || path == prefix || path.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/'))
}

/// `[attributes] [pub[(..)]] mod name { .. }`.
fn is_mod(tokens: &[TokenTree]) -> bool {
    let n = tokens.len();
    n >= 3
        && matches!(&tokens[n - 3], TokenTree::Ident(i) if i.to_string() == "mod")
        && matches!(&tokens[n - 2], TokenTree::Ident(_))
}

/// The module's items with `#[derive(::corten::Route)]` on each routed struct, and those structs.
/// A struct that derives `Route` itself is a link the module does not route (an area's page, an
/// API, a feed): it gets no handler.
fn scan(body: TokenStream) -> Result<(TokenStream, Vec<Routed>), Error> {
    let tokens: Vec<TokenTree> = body.into_iter().collect();
    let mut out: Vec<TokenTree> = Vec::new();
    let mut routed = Vec::new();
    // Where the current item's attributes start in `out`, and the attributes themselves.
    let mut start = None;
    let mut attrs: Vec<Group> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match (&tokens[i], tokens.get(i + 1)) {
            (TokenTree::Punct(p), Some(TokenTree::Group(g))) if p.as_char() == '#' && g.delimiter() == Delimiter::Bracket => {
                start.get_or_insert(out.len());
                attrs.push(g.clone());
                out.extend([tokens[i].clone(), tokens[i + 1].clone()]);
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
                let derives_route = attrs.iter().any(derives_route);
                if let (Some((path, span)), false) = (route_path(&attrs)?, derives_route) {
                    let at = start.unwrap_or(out.len());
                    let derive: Vec<TokenTree> = q("#[derive(::corten::Route)]", &[]).into_iter().collect();
                    out.splice(at..at, derive);
                    routed.push(Routed { name: name.clone(), path, span });
                }
            }
            _ => {}
        }
        start = None;
        attrs.clear();
        out.push(tokens[i].clone());
        i += 1;
    }
    Ok((out.into_iter().collect(), routed))
}

/// The path of `#[route("/..")]`, if the attributes have one (`#[route(query)]` has none).
fn route_path(attrs: &[Group]) -> Result<Option<(String, Span)>, Error> {
    for attr in attrs {
        let inner: Vec<TokenTree> = attr.stream().into_iter().collect();
        let [TokenTree::Ident(name), TokenTree::Group(args)] = inner.as_slice() else { continue };
        if name.to_string() != "route" {
            continue;
        }
        if let Some(TokenTree::Literal(lit)) = args.stream().into_iter().next() {
            let text = lit.to_string();
            let Some(path) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) else {
                return err(lit.span(), "a route's path is a string: #[route(\"/items/{id}\")]");
            };
            return Ok(Some((path.to_string(), lit.span())));
        }
    }
    Ok(None)
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
