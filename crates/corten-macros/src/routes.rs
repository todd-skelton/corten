//! `#[corten::routes]` on a module of routes: each `#[route("/..", get = show)]` struct in it is a
//! typed route (`#[derive(Route)]`) with the handlers that serve it, so a route with no handler
//! does not compile; `router()` puts each handler at its route's path, or with
//! `#[corten::routes(area = Admin)]`, `area(area)` does in that area, and with
//! `#[corten::routes(api = ApiRoot)]`, `api(api)` in that API. An area's or API's own prefix
//! route may have no handler. An area's is then only the prefix its routes are under, with no
//! `href`; an API's stays a route, for an endpoint added at it by hand (`Api::route`).

use proc_macro::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};

use crate::tokens::{err, is_punct, q, stream, Error};

/// The methods a route names a handler for (axum's `routing` functions); `any` is every method,
/// and the route's only handler.
const METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "head", "options", "trace", "any"];

/// A route the module routes: its struct's name, path and handlers.
struct Routed {
    name: Ident,
    path: String,
    span: Span,
    /// Each method with the path of its handler, as written.
    handlers: Vec<(Ident, TokenStream)>,
    /// The struct's arguments when it is named here (`<'_>`, or nothing); `None` when it has a
    /// type parameter, which the module cannot name.
    args: Option<String>,
    /// The rows its `get` serves when that is a feed (`rows = Level`), as written.
    rows: Option<TokenStream>,
}

/// What a struct's `#[route(..)]` says beyond its path.
#[derive(Default)]
struct Serves {
    handlers: Vec<(Ident, TokenStream)>,
    /// `rows = Level`: the rows of the feed its `get` serves.
    rows: Option<TokenStream>,
}

/// What `#[corten::routes(..)]` says: what the handlers share (`state = Db`), the area the
/// routes are in (`area = Admin`, the route at its prefix) or the API (`api = ApiRoot`), and the
/// caller every handler takes (`caller = Viewer`, as `Caller<Viewer>`).
#[derive(Default)]
struct Args {
    state: Option<TokenStream>,
    area: Option<TokenStream>,
    api: Option<TokenStream>,
    caller: Option<TokenStream>,
}

/// Where a module's routes go: the app, an area or an API.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Area,
    Api,
}

fn parse_args(args: TokenStream) -> Result<Args, Error> {
    const USAGE: &str = "#[corten::routes] takes the state the handlers share, the area or API the routes are in and the caller every handler takes, if any: #[corten::routes(state = Db, area = Admin, caller = Staff)] or #[corten::routes(api = ApiRoot)]";
    let tokens: Vec<TokenTree> = args.into_iter().collect();
    let mut out = Args::default();
    for part in tokens.split(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ',')) {
        match part {
            [] => {}
            [TokenTree::Ident(key), TokenTree::Punct(eq), value @ ..] if eq.as_char() == '=' && !value.is_empty() => {
                let slot = match key.to_string().as_str() {
                    "state" => &mut out.state,
                    "area" => &mut out.area,
                    "api" => &mut out.api,
                    "caller" => &mut out.caller,
                    _ => return err(key.span(), USAGE),
                };
                if slot.is_some() {
                    return err(key.span(), format!("{key} is given twice"));
                }
                *slot = Some(stream(value.iter().cloned()));
            }
            [first, ..] => return err(first.span(), USAGE),
        }
    }
    if let (Some(_), Some(api)) = (&out.area, &out.api) {
        let span = api.clone().into_iter().next().map_or(Span::call_site(), |t| t.span());
        return err(
            span,
            "a module's routes are in an area or in an API, not both: an API's routes have no pages, so no stylesheet",
        );
    }
    Ok(out)
}

pub fn expand(args: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    let Args { state, area, api, caller } = parse_args(args)?;
    // The routes' container: an area or an API, with the struct at its prefix.
    let within = area.map(|a| (Kind::Area, a)).or_else(|| api.map(|a| (Kind::Api, a)));
    let prefix_name = within.as_ref().map(|(_, p)| p.to_string().replace(' ', ""));
    let mut tokens: Vec<TokenTree> = item.into_iter().collect();
    let Some(TokenTree::Group(body)) = tokens.last().cloned().filter(|_| is_mod(&tokens)) else {
        let span = tokens.first().map_or(Span::call_site(), TokenTree::span);
        return err(
            span,
            "#[corten::routes] goes on a module of routes: #[corten::routes] mod url { #[route(\"/\", get = home)] pub struct Home; }",
        );
    };
    let TokenTree::Ident(module) = tokens[tokens.len() - 2].clone() else { unreachable!("is_mod checked") };
    let area = matches!(within, Some((Kind::Area, _)));
    let (mut items, routed) = scan(body.stream(), prefix_name.as_deref(), area)?;
    // Handlers are named from where the module is declared, so one a `use` inside it brings in is
    // not found there: say so at the `use`, not as "cannot find value" at each handler.
    // Only a private `use` the module itself never names is meant for a handler: one the module
    // uses (a field's type, a `pub use`, a `#[cfg(test)]` import) is its own business. A handler
    // written inside the module is the same mistake.
    let (imported, defined) = used_names(body.stream());
    let named = names_outside_routes(body.stream());
    for (_, handler) in routed.iter().flat_map(|r| &r.handlers) {
        let Some(TokenTree::Ident(first)) = handler.clone().into_iter().next() else { continue };
        let first = first.to_string();
        if let Some(name) = imported.iter().find(|i| i.to_string() == first && named.iter().filter(|n| **n == first).count() == 1)
        {
            return err(
                name.span(),
                format!(
                    "{first} is brought in inside the module, but a #[corten::routes] module names its handlers from where it is declared: move this use above mod {module}"
                ),
            );
        }
        if let Some(name) = defined.iter().find(|i| i.to_string() == first && named.iter().filter(|n| **n == first).count() == 1)
        {
            return err(
                name.span(),
                format!(
                    "{first} is written inside the module, but a #[corten::routes] module names its handlers from where it is declared: move it above mod {module}"
                ),
            );
        }
    }
    for (i, a) in routed.iter().enumerate() {
        if let Some(b) = routed[..i].iter().find(|b| same_pattern(&a.path, &b.path)) {
            return err(
                a.span,
                format!("{} and {} have the same path, {}: a request could only reach one", b.name, a.name, a.path),
            );
        }
        // axum matches `{page}.xml` as `{page}`, so two paths that differ only in such text are one
        // to it.
        let matched = crate::route::suffixed(&a.path).0;
        if let Some(b) = routed[..i].iter().find(|b| same_pattern(&matched, &crate::route::suffixed(&b.path).0)) {
            return err(
                a.span,
                format!(
                    "{} ({}) and {} ({}) differ only in the text after a parameter, which the router does not tell apart: give one a segment of its own, like /feeds/rss/{{name}}",
                    b.name, b.path, a.name, a.path
                ),
            );
        }
        // The router holds no parameter where another route has its catch-all, and would name
        // `{page}` for `{page}.xml` in saying so: say it here with the paths as written.
        if let Some(b) = routed[..i].iter().find(|b| {
            let theirs = crate::route::suffixed(&b.path).0;
            (matched != a.path || theirs != b.path)
                && (beside_catch_all(&matched, &theirs) || beside_catch_all(&theirs, &matched))
        }) {
            return err(
                a.span,
                format!(
                    "{} ({}) and {} ({}) have a parameter where the other has its catch-all, which the router cannot hold together: give the parameter's route a segment of its own, like /sitemap/{{page}}.xml",
                    b.name, b.path, a.name, a.path
                ),
            );
        }
    }
    // The area's or API's prefix, when its route is in the module: each route is checked against
    // it here, where the error can point at the route (`Area::route` and `Api::route` check every
    // route when the app builds).
    if let (Some((kind, _)), Some(name)) = (&within, &prefix_name) {
        if let Some(prefix) = routed.iter().find(|r| &r.name.to_string() == name) {
            if let Some(outside) = routed.iter().find(|r| !under(&prefix.path, &r.path)) {
                let what = if *kind == Kind::Area { "area" } else { "API" };
                return err(
                    outside.span,
                    format!(
                        "{} is in the {what} {} ({}), so its path is at or under {}: {}",
                        outside.name, prefix.name, prefix.path, prefix.path, outside.path
                    ),
                );
            }
        }
    }
    // The router is written where the module is declared, so a handler is named from there, where
    // handlers live (`get = home`), with no `use super::*` in the module. It is an impl of a trait
    // the module declares: `super::` cannot reach a function body the module is declared in (a
    // test's), and an impl applies wherever it is written.
    // Each handler is checked against the module's state, named for every call, so one that wants
    // another state is an error at its name rather than a mismatch between routers.
    let state_ty = state.clone().unwrap_or_else(|| q("()", &[]));
    // Each handler of a module with `caller = Viewer` takes `Caller<Viewer>`, checked at its name.
    let required = match &caller {
        Some(caller) => q("::corten::Caller<__A0>", std::slice::from_ref(caller)),
        None => q("()", &[]),
    };
    let mut calls = TokenStream::new();
    for route in routed.iter().filter(|r| !r.handlers.is_empty()) {
        // A route with a parameter in its path: each handler takes its `Params`.
        let params = match &route.args {
            Some(args) if route.path.contains('{') => {
                let args: TokenStream = args.parse().expect("lifetime arguments parse");
                q(
                    "::corten::route::Params<__A0::__A1 __A2>",
                    &[TokenTree::Ident(module.clone()).into(), TokenTree::Ident(route.name.clone()).into(), args],
                )
            }
            None if route.path.contains('{') => {
                return err(
                    route.span,
                    format!("{} has a type parameter, so its handlers cannot be held to taking its Params: give its fields concrete types", route.name),
                );
            }
            _ => q("()", &[]),
        };
        let mut methods = TokenStream::new();
        for (i, (method, handler)) in route.handlers.iter().enumerate() {
            let at = handler.clone().into_iter().last().map_or(method.span(), |t| t.span());
            // The rows the route names are its `get`'s; every other handler serves no feed.
            let rows = match &route.rows {
                Some(rows) if method.to_string() == "get" => rows.clone(),
                _ => q("::corten::answer::NotAFeed", &[]),
            };
            let call = respan(
                q(
                    "::corten::route::__private::__A0::<_, _, __A1, __A2, _, __A3, _, __A4>",
                    &[TokenTree::Ident(method.clone()).into(), state_ty.clone(), required.clone(), params.clone(), rows.clone()],
                ),
                at,
            );
            let mut call = call;
            call.extend([TokenTree::Group(Group::new(Delimiter::Parenthesis, handler.clone()))]);
            if i > 0 {
                methods.extend([
                    TokenTree::Punct(Punct::new('.', Spacing::Alone)),
                    TokenTree::Ident(Ident::new("__merge", Span::call_site())),
                ]);
                methods.extend([TokenTree::Group(Group::new(Delimiter::Parenthesis, call))]);
            } else {
                methods.extend(call);
            }
        }
        let template = if within.is_some() {
            ".route::<__A0::__A1>(::corten::Methods::with_state(__A2, state.clone()))"
        } else {
            ".__route::<__A0::__A1>(::corten::Methods::with_state(__A2, state.clone()))"
        };
        calls.extend(q(
            template,
            &[TokenTree::Ident(module.clone()).into(), TokenTree::Ident(route.name.clone()).into(), methods],
        ));
    }
    // A feed's route says what its rows are, where its handlers are named (and the rows' type
    // with them), for a script's `FeedUrl<Level>` parameter.
    let mut feeds = TokenStream::new();
    for route in &routed {
        if let (Some(rows), Some(args)) = (&route.rows, &route.args) {
            let ty: TokenStream = format!("{module}::{}{args}", route.name).parse().expect("a route's type parses");
            feeds.extend(q(
                "#[automatically_derived]
                impl ::corten::route::__private::FeedRows<__A1> for __A0 {}",
                &[ty, rows.clone()],
            ));
        }
    }
    if let Some((kind, prefix)) = within {
        let mut out = expand_within(kind, tokens, body, items, &module, calls, prefix, state);
        out.extend(feeds);
        return Ok(out);
    }
    let router = match &state {
        None => q(
            "/// The routes of this module, each at its path with its handlers: what `App::new` takes.
            #[allow(dead_code)]
            pub fn router() -> ::corten::Routes {
                <__CortenRoutes as __CortenRouter<()>>::__routes(())
            }",
            &[],
        ),
        // The state's type is written where the module is declared, so it is named there, in the
        // impl; `S` here is the one type that impl is for.
        Some(_) => q(
            "/// The routes of this module, each at its path with its handlers, which share `state`
            /// (`State<..>` in a handler): what `App::new` takes.
            #[allow(dead_code)]
            pub fn router<S>(state: S) -> ::corten::Routes
            where
                __CortenRoutes: __CortenRouter<S>,
            {
                <__CortenRoutes as __CortenRouter<S>>::__routes(state)
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
            fn __routes(state: S) -> ::corten::Routes;
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
            #[allow(unused_variables)]
            fn __routes(state: __A2) -> ::corten::Routes {
                ::corten::Routes::new() __A1
            }
        }",
        &[TokenTree::Ident(module.clone()).into(), calls, state],
    ));
    out.extend(feeds);
    Ok(out)
}

/// The module of an area's or an API's routes: `area(area)` (or `area(area, state)`) adds each
/// route to the `Area` that `App::area` gives, where the area's stylesheet and settings apply to
/// it, and `api(api)` likewise to the `Api` that `App::api` gives, where every failure is JSON.
#[allow(clippy::too_many_arguments)]
fn expand_within(
    kind: Kind,
    mut tokens: Vec<TokenTree>,
    body: Group,
    mut items: TokenStream,
    module: &Ident,
    calls: TokenStream,
    prefix: TokenStream,
    state: Option<TokenStream>,
) -> TokenStream {
    let (container, function, doc) = match kind {
        Kind::Area => ("Area", "area", "in their area, each at its path with its handlers: `.area(admin::area)`, `.area(|area| admin::area(area).components(&[..]))`. A route outside the area's prefix is a compile error."),
        Kind::Api => ("Api", "api", "in their API, each at its path with its handlers, every failure answered as JSON: `.api(prices::api)`, `.api(|api| prices::api(api).allow_origin(..))`. A route outside the API's prefix is a compile error."),
    };
    let container = Ident::new(container, Span::call_site());
    let function = Ident::new(function, Span::call_site());
    let names = [TokenTree::Ident(container.clone()).into(), TokenTree::Ident(function.clone()).into(), prefix];
    let doc = TokenTree::Literal(proc_macro::Literal::string(&format!("The routes of this module {doc}")));
    let mut item = q("#[doc = __A0]", &[doc.into()]);
    item.extend(match &state {
        None => q(
            "#[allow(dead_code)]
            pub fn __A1(routes: ::corten::__A0<__A2>) -> ::corten::__A0<__A2> {
                <__CortenRoutes as __CortenWithin<(), __A2>>::__within(routes, ())
            }",
            &names,
        ),
        Some(_) => q(
            "#[allow(dead_code)]
            pub fn __A1<S>(routes: ::corten::__A0<__A2>, state: S) -> ::corten::__A0<__A2>
            where
                __CortenRoutes: __CortenWithin<S, __A2>,
            {
                <__CortenRoutes as __CortenWithin<S, __A2>>::__within(routes, state)
            }",
            &names,
        ),
    });
    items.extend(item);
    items.extend(q(
        "#[doc(hidden)]
        pub struct __CortenRoutes;

        #[doc(hidden)]
        pub trait __CortenWithin<S, P> {
            fn __within(routes: ::corten::__A0<P>, state: S) -> ::corten::__A0<P>;
        }",
        &[TokenTree::Ident(container.clone()).into()],
    ));
    let mut module_body = Group::new(Delimiter::Brace, items);
    module_body.set_span(body.span());
    *tokens.last_mut().expect("the module body") = TokenTree::Group(module_body);
    let mut out: TokenStream = tokens.into_iter().collect();
    let state = state.unwrap_or_else(|| q("()", &[]));
    // The prefix is named where the module is declared too, so its type stays generic here; the
    // container's own check (`Area::route`, `Api::route`) holds each route under its prefix.
    out.extend(q(
        "#[automatically_derived]
        impl<P: ::corten::Prefix> __A0::__CortenWithin<__A2, P> for __A0::__CortenRoutes {
            #[allow(unused_variables)]
            fn __within(routes: ::corten::__A3<P>, state: __A2) -> ::corten::__A3<P> {
                routes __A1
            }
        }",
        &[TokenTree::Ident(module.clone()).into(), calls, state, TokenTree::Ident(container).into()],
    ));
    out
}

/// The names the module's own private `use` items without attributes bring in (`use
/// super::page::{show, edit as change};` brings in `show` and `change`), and the functions it
/// declares, each with the span to point at.
fn used_names(body: TokenStream) -> (Vec<Ident>, Vec<Ident>) {
    let tokens: Vec<TokenTree> = body.into_iter().collect();
    let (mut names, mut fns) = (Vec::new(), Vec::new());
    let mut i = 0;
    // Whether the current item has an attribute or a visibility, which keeps its `use` out.
    let mut marked = false;
    while i < tokens.len() {
        match &tokens[i] {
            TokenTree::Punct(p) if p.as_char() == '#' => marked = true,
            TokenTree::Ident(id) if id.to_string() == "pub" => marked = true,
            TokenTree::Ident(id) if id.to_string() == "use" => {
                let end = tokens[i..]
                    .iter()
                    .position(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ';'))
                    .map_or(tokens.len(), |at| i + at);
                if !marked {
                    use_tree(&tokens[i + 1..end], &mut names);
                }
                i = end;
                marked = false;
            }
            TokenTree::Ident(id) if id.to_string() == "fn" => {
                if let Some(TokenTree::Ident(name)) = tokens.get(i + 1) {
                    fns.push(name.clone());
                }
                marked = false;
            }
            TokenTree::Punct(p) if p.as_char() == ';' => marked = false,
            TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => marked = false,
            _ => {}
        }
        i += 1;
    }
    (names, fns)
}

/// Every identifier in the module, at any depth, but inside `#[route(..)]`: a name the module
/// uses is there more than once (its `use` and the use).
fn names_outside_routes(body: TokenStream) -> Vec<String> {
    let mut out = Vec::new();
    let tokens: Vec<TokenTree> = body.into_iter().collect();
    for (i, token) in tokens.iter().enumerate() {
        match token {
            TokenTree::Ident(id) => out.push(id.to_string()),
            TokenTree::Group(g) => {
                let route = g.delimiter() == Delimiter::Bracket
                    && matches!(tokens.get(i.wrapping_sub(1)), Some(TokenTree::Punct(p)) if p.as_char() == '#')
                    && matches!(g.stream().into_iter().next(), Some(TokenTree::Ident(id)) if id.to_string() == "route");
                if !route {
                    out.extend(names_outside_routes(g.stream()));
                }
            }
            _ => {}
        }
    }
    out
}

/// The names one use tree brings in: its last segment, the name after `as`, or each of a
/// `{..}` group's (`self` in one is the segment before it). A `*` brings in names it does not
/// list, so none are counted.
fn use_tree(tree: &[TokenTree], names: &mut Vec<Ident>) {
    let mut last: Option<Ident> = None;
    let mut i = 0;
    while i < tree.len() {
        match &tree[i] {
            TokenTree::Ident(id) if id.to_string() == "as" => {
                last = match tree.get(i + 1) {
                    Some(TokenTree::Ident(alias)) if alias.to_string() != "_" => Some(alias.clone()),
                    _ => None,
                };
                break;
            }
            TokenTree::Ident(id) => last = Some(id.clone()),
            TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => {
                let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                for part in inner.split(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ',')) {
                    if matches!(part, [TokenTree::Ident(s)] if s.to_string() == "self") {
                        names.extend(last.clone());
                    } else {
                        use_tree(part, names);
                    }
                }
                return;
            }
            TokenTree::Punct(p) if p.as_char() == '*' => return,
            _ => {}
        }
        i += 1;
    }
    names.extend(last.filter(|id| !matches!(id.to_string().as_str(), "self" | "super" | "crate")));
}

/// `path` is `prefix` or under it, a segment at a time (`/admin` holds `/admin/users`, not
/// `/admins`).
fn under(prefix: &str, path: &str) -> bool {
    prefix == "/" || path == prefix || path.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/'))
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
/// `#[route(..)]` cut to the path, and those structs with their handlers. `prefix` is the
/// struct an area or API module is under, the one route that may name no handler. An area's
/// (`area`) is then a prefix and nothing more (`::corten::Prefix`, no `href`), since no page is
/// at its URL; an API's stays a route, which an endpoint built from a value (the MCP endpoint) is
/// added at by hand.
fn scan(body: TokenStream, prefix: Option<&str>, area: bool) -> Result<(TokenStream, Vec<Routed>), Error> {
    let tokens: Vec<TokenTree> = body.into_iter().collect();
    let mut out: Vec<TokenTree> = Vec::new();
    let mut routed = Vec::new();
    // Where the current item's attributes start in `out`, whether one derives `Route`, and its
    // route's path and what it serves.
    let mut start = None;
    let mut derives = None;
    let mut route: Option<(String, Span, Serves)> = None;
    // Where the current item's `#[route(..)]` is in `out`, and the prefix's impl, if any.
    let mut route_at = None;
    let mut prefix_impl = TokenStream::new();
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
                        route_at = Some(out.len());
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
                                "in a #[corten::routes] module the module derives Route for {name}: remove #[derive(Route)] (a route another module serves is declared in that module)"
                            ),
                        );
                    }
                    if serves.handlers.is_empty() && prefix != Some(name.to_string().as_str()) {
                        return err(
                            span,
                            format!(
                                "{name} has no handler: name the one that serves it, #[route(\"{path}\", get = {})]; a route an API serves goes in its #[corten::routes(api = ..)] module",
                                crate::names::snake(&name.to_string())
                            ),
                        );
                    }
                    if serves.handlers.is_empty() && area {
                        // The area's prefix with no page at its URL: no link to it can compile.
                        if !matches!(tokens.get(i + 2), Some(TokenTree::Punct(p)) if p.as_char() == ';') {
                            return err(
                                name.span(),
                                format!("{name} is the prefix, with no page at {path}, so it has no fields: pub struct {name};"),
                            );
                        }
                        let at = route_at.expect("the route attribute was read");
                        out.drain(at..at + 2);
                        prefix_impl.extend(q(
                            "#[automatically_derived]
                            impl ::corten::Prefix for __A0 {
                                const PREFIX: &'static str = __A1;
                            }",
                            &[
                                TokenTree::Ident(name.clone()).into(),
                                TokenTree::Literal(proc_macro::Literal::string(&path)).into(),
                            ],
                        ));
                        routed.push(Routed { name: name.clone(), path, span, handlers: Vec::new(), args: None, rows: None });
                        start = None;
                        derives = None;
                        route_at = None;
                        out.push(tokens[i].clone());
                        i += 1;
                        continue;
                    }
                    let at = start.unwrap_or(out.len());
                    let derive: Vec<TokenTree> = q("#[derive(::corten::Route)]", &[]).into_iter().collect();
                    out.splice(at..at, derive);
                    let args = lifetime_args(tokens.get(i + 2..).unwrap_or_default());
                    if let Some(rows) = &serves.rows {
                        let at = rows.clone().into_iter().last().map_or(span, |t| t.span());
                        if !serves.handlers.iter().any(|(m, _)| m.to_string() == "get") {
                            return err(
                                at,
                                format!("rows name what {name}'s get serves: #[route(\"{path}\", get = .., rows = ..)]"),
                            );
                        }
                        if args.is_none() {
                            return err(
                                at,
                                format!(
                                    "{name} has a type parameter, so it cannot name its rows: give its fields concrete types"
                                ),
                            );
                        }
                    }
                    routed.push(Routed { name: name.clone(), path, span, handlers: serves.handlers, args, rows: serves.rows });
                }
            }
            _ => {}
        }
        start = None;
        derives = None;
        route = None;
        route_at = None;
        out.push(tokens[i].clone());
        i += 1;
    }
    out.extend(prefix_impl);
    Ok((out.into_iter().collect(), routed))
}

/// `#[route("/..", get = show, post = save)]`: the path, its span, what it serves, and the attribute cut to `#[route("/..")]` for `#[derive(Route)]`. `None` for
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
    // `prefetch = false`, kept for `#[derive(Route)]`, which reads it.
    let mut keep: Vec<TokenTree> = Vec::new();
    let mut rest = &args[1..];
    while let Some(first) = rest.first() {
        if !matches!(first, TokenTree::Punct(p) if p.as_char() == ',') {
            return err(first.span(), "expected a comma, then a handler: get = show");
        }
        rest = &rest[1..];
        let end = rest.iter().position(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == ',')).unwrap_or(rest.len());
        let (part, after) = rest.split_at(end);
        rest = after;
        match part {
            [] => {}
            [TokenTree::Ident(key), TokenTree::Punct(eq), ..] if eq.as_char() == '=' && key.to_string() == "prefetch" => {
                keep.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
                keep.extend(part.iter().cloned());
            }
            [TokenTree::Ident(key), TokenTree::Punct(eq), rows @ ..] if eq.as_char() == '=' && key.to_string() == "rows" => {
                if serves.rows.is_some() {
                    return err(key.span(), "rows is named twice");
                }
                if rows.is_empty() || !is_path(rows) {
                    let span = rows.first().map_or(key.span(), TokenTree::span);
                    return err(span, "rows name the feed's row type: rows = Level (a path, as FeedResponse<Level> names it)");
                }
                serves.rows = Some(stream(rows.iter().cloned()));
            }
            [TokenTree::Ident(method), TokenTree::Punct(eq), handler @ ..] if eq.as_char() == '=' => {
                let key = method.to_string();
                if !METHODS.contains(&key.as_str()) {
                    return err(method.span(), format!("{key} is not a method: one of {}", METHODS.join(", ")));
                }
                if serves.handlers.iter().any(|(m, _)| m.to_string() == key) {
                    return err(method.span(), format!("{key} is named twice: a route has one handler per method"));
                }
                if !serves.handlers.is_empty() && (key == "any" || serves.handlers.iter().any(|(m, _)| m.to_string() == "any")) {
                    return err(
                        method.span(),
                        "any = .. serves every method, so it is the route's only handler: #[route(\"/{*rest}\", any = relay)]",
                    );
                }
                if handler.iter().any(|t| is_punct(Some(t), '<')) {
                    return err(
                        handler[0].span(),
                        format!("{key} = names a handler without type arguments: a generic handler's route is picked by the Params it takes, and a route without a parameter takes a handler of its own ({key} = relay_here) that calls the shared code"),
                    );
                }
                if handler.is_empty() || !is_path(handler) {
                    let span = handler.first().map_or(method.span(), TokenTree::span);
                    return err(span, format!("name a handler function: {key} = show (a path, not a closure)"));
                }
                serves.handlers.push((method.clone(), stream(handler.iter().cloned())));
            }
            _ => return err(part[0].span(), "expected a handler: get = show"),
        }
    }
    let mut cut_args = Group::new(Delimiter::Parenthesis, stream(std::iter::once(TokenTree::Literal(lit.clone())).chain(keep)));
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

/// A struct's generics, written as arguments that name it in a function body: `<'_, '_>` for its
/// lifetimes, nothing when it has none, and `None` when it has a type parameter.
fn lifetime_args(after_name: &[TokenTree]) -> Option<String> {
    if !matches!(after_name.first(), Some(TokenTree::Punct(p)) if p.as_char() == '<') {
        return Some(String::new());
    }
    let mut depth = 0;
    let mut params = vec![Vec::new()];
    for token in after_name {
        if let TokenTree::Punct(p) = token {
            match p.as_char() {
                '<' => {
                    depth += 1;
                    if depth == 1 {
                        continue;
                    }
                }
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                ',' if depth == 1 => {
                    params.push(Vec::new());
                    continue;
                }
                _ => {}
            }
        }
        params.last_mut().expect("one at least").push(token.clone());
    }
    params.retain(|p| !p.is_empty());
    let lifetimes = params.iter().all(|p| matches!(p.first(), Some(TokenTree::Punct(q)) if q.as_char() == '\''));
    lifetimes.then(|| format!("<{}>", vec!["'_"; params.len()].join(", ")))
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
/// Whether `path` has a parameter where `other` has its catch-all, after the same segments
/// (`/{name}` and `/{*page}`), which axum's router refuses to hold together.
fn beside_catch_all(path: &str, other: &str) -> bool {
    let param = |s: &str| s.starts_with('{') && s.ends_with('}');
    let mut ours = path.split('/');
    for theirs in other.split('/') {
        let Some(mine) = ours.next() else { return false };
        if theirs.starts_with("{*") {
            return param(mine) && !mine.starts_with("{*");
        }
        if !(mine == theirs || param(mine) && param(theirs)) {
            return false;
        }
    }
    false
}

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

#[cfg(test)]
mod tests {
    use super::beside_catch_all;

    #[test]
    fn a_parameter_beside_a_catch_all_is_found() {
        for (path, other, beside) in [
            ("/{name}", "/{*page}", true),
            ("/docs/{name}", "/docs/{*rest}", true),
            ("/{kind}/{name}", "/{a}/{*rest}", true),
            ("/{name}/more", "/{*page}", true),
            ("/docs/{name}", "/{*page}", false),
            ("/docs/{name}", "/blog/{*rest}", false),
            ("/docs", "/{*page}", false),
            ("/{*page}", "/{*page}", false),
            ("/docs/{name}", "/docs/{id}", false),
        ] {
            assert_eq!(beside_catch_all(path, other), beside, "{path} beside {other}");
        }
    }
}
