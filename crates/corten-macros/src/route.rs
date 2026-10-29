//! `#[derive(Route)]`: a struct whose fields are a URL's parameters.
//!
//! `#[route("/items/{id}")] struct Item<'a> { id: &'a str, tab: Option<&'a str> }`: fields named in
//! the path fill it (percent-encoded); the rest are query parameters (`None` leaves one out).
//! The path is checked here, so a link cannot name a parameter the route does not have. A query
//! parameter's name is the one serde reads (`rename`, `rename_all`), so the link and the handler
//! that deserializes the struct agree.

use proc_macro::TokenStream;

use proc_macro::{Delimiter, Group, Ident, Span, TokenTree};

use crate::{
    derive::{named_fields, parse, string, Field, Item},
    tokens::{err, is_ident, is_punct, q, string_value, Error},
};

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let item = parse(input, "Route")?;
    if item.is_enum {
        return err(item.name.span(), "Route is derived for a struct: #[route(\"/items/{id}\")] struct Item<'a> { id: &'a str }");
    }
    let fields = match &item.body {
        Some(body) => named_fields(body)?,
        None => Vec::new(),
    };
    let mut fields = fields;
    serde_names(&item.attrs, &mut fields)?;
    check_prefixes(&fields)?;
    // `#[route(query)]`: query parameters shared by several routes, with no path of their own.
    if item.attrs.iter().any(is_route_query) {
        let body = query_body(&fields);
        return Ok(item.impl_block(
            "::corten::route::QueryParams",
            q(
                "#[allow(unused_variables)]
                fn write_params(&self, __corten_first: &mut bool, __corten_html: bool, __OUT: &mut ::std::string::String) {
                    #[allow(unused_imports)]
                    use ::corten::route::__private::{QueryOption as _, QueryValue as _};
                    __A0
                }",
                &[body],
            ),
        ));
    }
    // `#[route("/..", get = show)]` outside a routes module: only a routes module routes.
    for attr in &item.attrs {
        let tokens: Vec<TokenTree> = attr.stream().into_iter().collect();
        if let [TokenTree::Ident(name), TokenTree::Group(args)] = tokens.as_slice() {
            let args: Vec<TokenTree> = args.stream().into_iter().collect();
            if name.to_string() == "route" && args.len() > 1 && matches!(args.first(), Some(TokenTree::Literal(_))) {
                return err(
                    args[1].span(),
                    "a handler (get = show) is named in a #[corten::routes] module; a route outside one is only its path, #[route(\"/..\")]",
                );
            }
        }
    }
    let Some((path, span)) = item.attr_string("route")? else {
        return err(
            item.name.span(),
            "Route needs the path: #[route(\"/items/{id}\")], or #[route(query)] for parameters routes share",
        );
    };
    let segments = parse_path(&path).map_err(|message| (span, message))?;
    for segment in &segments {
        if let Segment::Param(name) | Segment::Rest(name) = segment {
            match fields.iter().find(|f| f.name == *name) {
                None => return err(span, format!("the path has {{{name}}} but {} has no field {name}", item.name)),
                Some(f) if f.attrs.iter().any(is_route_query) => {
                    return err(
                        f.ident.span(),
                        format!("{name} is #[route(query)], so it is query parameters, not {{{name}}} in the path"),
                    );
                }
                Some(f) if route_prefix(f)?.is_some() => {
                    return err(
                        f.ident.span(),
                        format!("{name} is #[route(prefix = ..)], so it is query parameters, not {{{name}}} in the path"),
                    );
                }
                Some(f) if f.key != f.name => {
                    return err(
                        f.ident.span(),
                        format!(
                            "serde reads {name} as {:?}, but the path names it {{{name}}}: rename it #[serde(rename = \"{name}\")] or name the path's parameter {{{}}}",
                            f.key, f.key
                        ),
                    );
                }
                Some(_) => {}
            }
        }
    }
    let mut body = TokenStream::new();
    let mut text = String::new();
    let flush = |text: &mut String, body: &mut TokenStream| {
        if !text.is_empty() {
            body.extend(q("__OUT.push_str(__A0);", &[string(text).into()]));
            text.clear();
        }
    };
    for segment in &segments {
        match segment {
            Segment::Static(s) => text.push_str(s),
            Segment::Param(name) | Segment::Rest(name) => {
                flush(&mut text, &mut body);
                let field = fields.iter().find(|f| f.name == *name).expect("checked above");
                let rest = if matches!(segment, Segment::Rest(_)) { "true" } else { "false" };
                body.extend(q(
                    &format!("(&::corten::route::__private::Segment(&self.__A0)).__segment({rest}, __OUT);"),
                    &[proc_macro::TokenTree::Ident(field.ident.clone()).into()],
                ));
            }
        }
    }
    flush(&mut text, &mut body);
    let query: Vec<_> = fields
        .iter()
        .filter(|f| !segments.iter().any(|s| matches!(s, Segment::Param(n) | Segment::Rest(n) if *n == f.name)))
        .collect();
    if !query.is_empty() {
        body.extend(q("let mut __corten_first = true; let __corten_first = &mut __corten_first;", &[]));
        body.extend(query_writes(&query));
    }
    let mut out = item.impl_block(
        "::corten::route::Route",
        q(
            "const PATH: &'static str = __A0;
            #[allow(unused_variables)]
            fn write_href(&self, __OUT: &mut ::std::string::String, __corten_html: bool) {
                #[allow(unused_imports)]
                use ::corten::route::__private::{QueryOption as _, QueryValue as _, SegmentText as _, SegmentValue as _};
                __A1
            }",
            &[string(&path).into(), body],
        ),
    );
    out.extend(item.impl_block(
        "::corten::view::Render",
        q("fn render(&self, out: &mut ::std::string::String) { ::corten::route::Route::write_href(self, out, true) }", &[]),
    ));
    out.extend(item.impl_block(
        "::corten::view::AttrValue",
        q(
            "fn write_attr(&self, name: &str, out: &mut ::std::string::String) {
                out.push_str(name);
                out.push_str(\"=\\\"\");
                ::corten::route::Route::write_href(self, out, true);
                out.push('\"');
            }",
            &[],
        ),
    ));
    out.extend(from_url(&item, &fields, &segments)?);
    out.extend(item.impl_block(
        "::std::fmt::Display",
        q(
            "fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(&::corten::route::Route::href(self))
            }",
            &[],
        ),
    ));
    Ok(out)
}

/// `FromUrl`, for `corten::Params`: the struct's fields, owned (`__ItemParams`), read back from a
/// request. Only for a struct with fields and no type parameters whose fields are each owned or a
/// reference (`&'a str`, `&'a [T]`, `&'a T`); for another, `Params` does not compile and says so.
fn from_url(item: &Item, fields: &[Field], segments: &[Segment]) -> Result<TokenStream, Error> {
    if fields.is_empty() || !only_lifetimes(&item.impl_generics) {
        return Ok(TokenStream::new());
    }
    let mut types = Vec::new();
    for field in fields {
        match owned(&field.ty) {
            Some(ty) => types.push(ty),
            None => return Ok(TokenStream::new()),
        }
    }
    let twin = Ident::new(&format!("__{}Params", item.name), item.name.span());
    let mut body = TokenStream::new();
    for (field, ty) in fields.iter().zip(&types) {
        body.extend(field.vis.iter().cloned());
        body.extend([TokenTree::Ident(field.ident.clone())]);
        body.extend(q(":", &[]));
        body.extend(ty.clone());
        body.extend(q(",", &[]));
    }
    let mut out = q("#[doc(hidden)] #[allow(dead_code, non_camel_case_types)]", &[]);
    out.extend(item.vis.clone());
    out.extend(q("struct", &[]));
    out.extend([TokenTree::Ident(twin.clone()), TokenTree::Group(Group::new(Delimiter::Brace, body))]);

    // The path's parameters, in its order, as a tuple axum's `Path` reads.
    let in_path = |f: &Field| segments.iter().any(|s| matches!(s, Segment::Param(n) | Segment::Rest(n) if *n == f.name));
    let mut path_types = TokenStream::new();
    let mut path_names = TokenStream::new();
    for segment in segments {
        if let Segment::Param(name) | Segment::Rest(name) = segment {
            let at = fields.iter().position(|f| f.name == *name).expect("checked by expand");
            path_types.extend(types[at].clone());
            path_types.extend(q(",", &[]));
            path_names.extend([TokenTree::Ident(fields[at].ident.clone())]);
            path_names.extend(q(",", &[]));
        }
    }
    let (path_type, path_pattern) = if path_names.is_empty() {
        (q("()", &[]), q("()", &[]))
    } else {
        (
            q("::corten::route::__private::InPath<(__A0)>", &[path_types]),
            q("::corten::route::__private::InPath((__A0))", &[path_names]),
        )
    };
    // Each query field is read through a bound on `Later<'__corten, T>`, which names the impl's
    // lifetime, so it is checked where `Params` is used: a link's field needs only `Display`.
    let mut reads = TokenStream::new();
    let mut bounds = TokenStream::new();
    let mut query = false;
    for (field, ty) in fields.iter().zip(&types) {
        let ident: TokenStream = TokenTree::Ident(field.ident.clone()).into();
        if in_path(field) {
            reads.extend(q("__A0,", &[ident]));
            continue;
        }
        query = true;
        let later = q("::corten::route::__private::Later<'__corten, __A0>", std::slice::from_ref(ty));
        if field.attrs.iter().any(is_route_query) {
            bounds.extend(q("__A0: ::corten::route::__private::Deserialized<T = __A1>,", &[later.clone(), ty.clone()]));
            reads.extend(q("__A0: <__A1 as ::corten::route::__private::Deserialized>::all(&__pairs)?,", &[ident, later]));
        } else if let Some((prefix, _)) = route_prefix(field)? {
            bounds.extend(q("__A0: ::corten::route::__private::Prefixed<T = __A1>,", &[later.clone(), ty.clone()]));
            reads.extend(q(
                "__A0: <__A1 as ::corten::route::__private::Prefixed>::prefixed(&__pairs, __A2)?,",
                &[ident, later, string(&prefix).into()],
            ));
        } else {
            bounds.extend(q("__A0: ::corten::route::__private::Deserialized<T = __A1>,", &[later.clone(), ty.clone()]));
            let default = match serde_default(field)? {
                None => q("::core::option::Option::None", &[]),
                Some(None) => q("::core::option::Option::Some(::core::default::Default::default as fn() -> _)", &[]),
                Some(Some(path)) => q("::core::option::Option::Some(__A0 as fn() -> _)", &[path]),
            };
            reads.extend(q(
                "__A0: <__A1 as ::corten::route::__private::Deserialized>::field(&__pairs, __A2, __A3)?,",
                &[ident, later, string(&field.key).into(), default],
            ));
        }
    }
    let pairs = if query {
        q("let __pairs = ::corten::route::__private::sorted_pairs(__query);", &[])
    } else {
        q("let _ = __query;", &[])
    };
    // `impl<'__corten, 'a> FromUrl for Item<'a> where ..`.
    let mut generics: Vec<TokenTree> = item.impl_generics.clone().into_iter().collect();
    let lifetime = q("'__corten", &[]);
    if generics.is_empty() {
        generics = q("<__A0>", &[lifetime]).into_iter().collect();
    } else {
        generics.splice(1..1, q("__A0,", &[lifetime]));
    }
    let mut where_clause = item.where_clause.clone();
    if !bounds.is_empty() {
        if where_clause.is_empty() {
            where_clause.extend(q("where", &[]));
        } else if !matches!(where_clause.clone().into_iter().last(), Some(TokenTree::Punct(p)) if p.as_char() == ',') {
            where_clause.extend(q(",", &[]));
        }
        where_clause.extend(bounds);
    }
    out.extend(q(
        "#[automatically_derived]
        impl __A5 ::corten::route::FromUrl for __A6 __A7 __A8 {
            type Params = __A0;
            type Path = __A1;
            #[inline]
            fn __read(__path: Self::Path, __query: &str) -> ::core::result::Result<__A0, ::corten::form::FormError> {
                let __A2 = __path;
                __A3
                ::core::result::Result::Ok(__A0 { __A4 })
            }
        }",
        &[
            TokenTree::Ident(twin).into(),
            path_type,
            path_pattern,
            pairs,
            reads,
            generics.into_iter().collect(),
            TokenTree::Ident(item.name.clone()).into(),
            item.type_generics.clone(),
            where_clause,
        ],
    ));
    Ok(out)
}

/// `<'a, 'b>` or nothing: generics a route's owned twin can leave out.
fn only_lifetimes(generics: &TokenStream) -> bool {
    let tokens: Vec<TokenTree> = generics.clone().into_iter().collect();
    let Some((TokenTree::Punct(open), inner)) = tokens.split_first() else { return true };
    if open.as_char() != '<' {
        return false;
    }
    let inner = &inner[..inner.len().saturating_sub(1)];
    inner.split(|t| is_punct(Some(t), ',')).all(|param| param.is_empty() || is_punct(param.first(), '\''))
}

/// A field's type with each reference owned: `&'a str` is `String`, `&'a [T]` `Vec<T>`, any
/// other `&'a T` `<T as ToOwned>::Owned`. `None` for a type that borrows another way
/// (`Cow<'a, str>`).
fn owned(ty: &[TokenTree]) -> Option<TokenStream> {
    let mut out = TokenStream::new();
    let mut i = 0;
    while i < ty.len() {
        match &ty[i] {
            TokenTree::Punct(p) if p.as_char() == '&' => {
                if !is_punct(ty.get(i + 1), '\'') || !matches!(ty.get(i + 2), Some(TokenTree::Ident(_))) {
                    return None;
                }
                let start = i + 3;
                let mut end = start;
                let mut depth = 0;
                while let Some(token) = ty.get(end) {
                    if let TokenTree::Punct(p) = token {
                        match p.as_char() {
                            '<' => depth += 1,
                            '>' | ',' if depth == 0 => break,
                            '>' => depth -= 1,
                            _ => {}
                        }
                    }
                    end += 1;
                }
                let referent = &ty[start..end];
                if referent
                    .iter()
                    .any(|t| matches!(t, TokenTree::Ident(id) if id.to_string() == "dyn" || id.to_string() == "impl"))
                {
                    return None;
                }
                out.extend(match referent {
                    [TokenTree::Ident(id)] if id.to_string() == "str" => q("::std::string::String", &[]),
                    [TokenTree::Group(g)] if g.delimiter() == Delimiter::Bracket => {
                        let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                        q("::std::vec::Vec<__A0>", &[owned(&inner)?])
                    }
                    _ => q("<__A0 as ::std::borrow::ToOwned>::Owned", &[owned(referent)?]),
                });
                i = end;
            }
            TokenTree::Punct(p) if p.as_char() == '\'' => return None,
            TokenTree::Group(g) => {
                let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                let mut group = Group::new(g.delimiter(), owned(&inner)?);
                group.set_span(g.span());
                out.extend([TokenTree::Group(group)]);
                i += 1;
            }
            other => {
                out.extend([other.clone()]);
                i += 1;
            }
        }
    }
    Some(out)
}

/// `#[serde(default)]` (`Some(None)`) or `#[serde(default = "path")]` (`Some(Some(path))`).
fn serde_default(field: &Field) -> Result<Option<Option<TokenStream>>, Error> {
    for attr in &field.attrs {
        for (key, value) in crate::serde_args(attr).map_err(|m| (attr.span(), m))? {
            if key == "default" {
                return match value {
                    None => Ok(Some(None)),
                    Some(path) => match path.parse::<TokenStream>() {
                        Ok(path) => Ok(Some(Some(path))),
                        Err(_) => err(attr.span(), format!("#[serde(default = {path:?})] is not a path")),
                    },
                };
            }
        }
    }
    Ok(None)
}

/// Each field's query key: the name serde reads it by, with the struct's `rename_all` and the
/// field's `rename` applied (a field's `skip` is not a parameter; it is left as it is).
fn serde_names(attrs: &[proc_macro::Group], fields: &mut [Field]) -> Result<(), Error> {
    let mut rule = None;
    for attr in attrs {
        for (key, value) in crate::serde_args(attr).map_err(|m| (attr.span(), m))? {
            if key == "rename_all" {
                let Some(value) = value else {
                    return err(attr.span(), "use rename_all = \"camelCase\"; rename_all(deserialize = ..) is not supported");
                };
                rule = Some(value);
            }
        }
    }
    for field in fields {
        let mut rename = None;
        for attr in &field.attrs {
            for (key, value) in crate::serde_args(attr).map_err(|m| (attr.span(), m))? {
                if key == "rename" {
                    let Some(value) = value else {
                        return err(attr.span(), "use rename = \"priceMin\"; rename(deserialize = ..) is not supported");
                    };
                    rename = Some(value);
                }
            }
        }
        field.key = match (rename, &rule) {
            (Some(name), _) => name,
            (None, Some(rule)) => crate::apply_rule(&field.name, rule).map_err(|m| (field.ident.span(), m))?,
            (None, None) => field.name.clone(),
        };
        // Written into links as it is.
        if field.key.is_empty() || !field.key.chars().all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)) {
            return err(
                field.ident.span(),
                format!("a query parameter's name is letters, digits and -._~, which a URL writes as they are: {:?}", field.key),
            );
        }
    }
    Ok(())
}

/// `#[route(query)]`.
fn is_route_query(attr: &proc_macro::Group) -> bool {
    let tokens: Vec<proc_macro::TokenTree> = attr.stream().into_iter().collect();
    matches!(tokens.as_slice(), [proc_macro::TokenTree::Ident(name), proc_macro::TokenTree::Group(args)]
        if name.to_string() == "route" && args.stream().to_string().trim() == "query")
}

/// `#[route(prefix = "dim.")]`: the prefix and its span.
fn route_prefix(field: &Field) -> Result<Option<(String, Span)>, Error> {
    for attr in &field.attrs {
        let tokens: Vec<TokenTree> = attr.stream().into_iter().collect();
        let [route, TokenTree::Group(args)] = tokens.as_slice() else { continue };
        if !is_ident(Some(route), "route") || args.delimiter() != Delimiter::Parenthesis {
            continue;
        }
        let args: Vec<TokenTree> = args.stream().into_iter().collect();
        if !is_ident(args.first(), "prefix") {
            continue;
        }
        let literal = match args.as_slice() {
            [_, eq, TokenTree::Literal(literal)] if is_punct(Some(eq), '=') => literal,
            _ => return err(attr.span(), "expected #[route(prefix = \"dim.\")]"),
        };
        let Some(prefix) = string_value(literal) else { return err(literal.span(), "expected a string") };
        return Ok(Some((prefix, literal.span())));
    }
    Ok(None)
}

/// Each prefix is written as it is, so it has only characters a query key keeps (letters, digits,
/// `-._~`), and names no other parameter: reading `dim.x` back, a parameter `dimmed` beside a
/// prefix `dim` would be one of the family, and so would every key of a longer prefix.
fn check_prefixes(fields: &[Field]) -> Result<(), Error> {
    let mut prefixes = Vec::new();
    for field in fields {
        if let Some((prefix, span)) = route_prefix(field)? {
            if field.attrs.iter().any(is_route_query) {
                return err(span, "a field is #[route(query)] or #[route(prefix = ..)], not both");
            }
            if prefix.is_empty() || !prefix.chars().all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)) {
                return err(span, format!("a prefix is letters, digits and -._~, which a URL writes as they are: {prefix:?}"));
            }
            prefixes.push((prefix, span, field.name.clone()));
        }
    }
    for (prefix, span, name) in &prefixes {
        for field in fields.iter().filter(|f| f.name != *name) {
            let other = match route_prefix(field)? {
                Some((other, _)) => other,
                None if field.attrs.iter().any(is_route_query) => continue,
                None => field.key.clone(),
            };
            // Checked from both sides, so one prefix inside another is found from the shorter.
            if other.starts_with(prefix.as_str()) {
                return err(
                    *span,
                    format!(
                        "the prefix {prefix:?} of {name} and {other:?} of {} overlap: a parameter read back could be either's",
                        field.name
                    ),
                );
            }
        }
    }
    Ok(())
}

fn query_body(fields: &[Field]) -> TokenStream {
    query_writes(&fields.iter().collect::<Vec<_>>())
}

/// Each field as `key=value` (or, for a `#[route(query)]` field, the parameters it holds), with
/// `__corten_first: &mut bool` choosing `?` or `&`.
fn query_writes(fields: &[&Field]) -> TokenStream {
    let mut body = TokenStream::new();
    for field in fields {
        let ident: TokenStream = proc_macro::TokenTree::Ident(field.ident.clone()).into();
        if field.attrs.iter().any(is_route_query) {
            body.extend(q(
                "::corten::route::QueryParams::write_params(&self.__A0, __corten_first, __corten_html, __OUT);",
                &[ident],
            ));
        } else if let Ok(Some((prefix, _))) = route_prefix(field) {
            body.extend(q(
                "::corten::route::Pairs::write_pairs(&self.__A0, __A1, __corten_first, __corten_html, __OUT);",
                &[ident, string(&prefix).into()],
            ));
        } else {
            body.extend(q(
                "(&::corten::route::__private::Query(&self.__A0)).__query(__A1, __corten_first, __corten_html, __OUT);",
                &[ident, string(&format!("{}=", field.key)).into()],
            ));
        }
    }
    body
}

#[derive(Debug, PartialEq)]
pub enum Segment {
    Static(String),
    Param(String),
    /// `{*rest}`: the rest of the path, slashes included.
    Rest(String),
}

/// Characters a path may contain as written; they need no escaping in a URL or in HTML.
fn plain(c: char) -> bool {
    c.is_ascii_alphanumeric() || "/-._~!$*+,;=:@".contains(c)
}

/// Splits an axum path into static text and parameters, checking it as axum would.
pub fn parse_path(path: &str) -> Result<Vec<Segment>, String> {
    if !path.starts_with('/') {
        return Err(format!("a route path starts with /: {path:?}"));
    }
    let mut segments = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let parts: Vec<&str> = path.split('/').collect();
    for (n, part) in parts.iter().enumerate() {
        if n > 0 {
            push_static(&mut segments, "/");
        }
        if let Some(inner) = part.strip_prefix('{').and_then(|p| p.strip_suffix('}')) {
            let (rest, name) = match inner.strip_prefix('*') {
                Some(name) => (true, name),
                None => (false, inner),
            };
            let valid = name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
                && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
            if !valid {
                return Err(format!("{{{inner}}} is not a parameter: use a field name like {{id}}"));
            }
            if rest && n + 1 != parts.len() {
                return Err(format!("{{*{name}}} takes the rest of the path, so it comes last"));
            }
            if names.iter().any(|m| m == name) {
                return Err(format!("{{{name}}} appears twice"));
            }
            names.push(name.to_string());
            segments.push(if rest { Segment::Rest(name.into()) } else { Segment::Param(name.into()) });
        } else if part.contains(['{', '}']) {
            return Err(format!("a parameter is a whole segment, like /items/{{id}}: {part:?}"));
        } else if let Some(c) = part.chars().find(|c| !plain(*c)) {
            return Err(format!("{c:?} is not allowed in a route path; write the path as it is matched"));
        } else {
            push_static(&mut segments, part);
        }
    }
    Ok(segments)
}

fn push_static(segments: &mut Vec<Segment>, text: &str) {
    match segments.last_mut() {
        Some(Segment::Static(s)) => s.push_str(text),
        _ => segments.push(Segment::Static(text.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_split_into_text_and_parameters() {
        assert_eq!(parse_path("/").unwrap(), vec![Segment::Static("/".into())]);
        assert_eq!(
            parse_path("/items/{id}/offers").unwrap(),
            vec![Segment::Static("/items/".into()), Segment::Param("id".into()), Segment::Static("/offers".into())]
        );
        assert_eq!(parse_path("/files/{*path}").unwrap()[1], Segment::Rest("path".into()));
        assert!(parse_path("items").is_err());
        assert!(parse_path("/items/item_{id}").is_err());
        assert!(parse_path("/{*path}/x").is_err());
        assert!(parse_path("/{id}/{id}").is_err());
        assert!(parse_path("/a b").is_err());
        assert!(parse_path("/{Id}").is_err());
    }
}
