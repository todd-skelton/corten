//! `#[derive(Route)]`: a struct whose fields are a URL's parameters.
//!
//! `#[route("/items/{id}")] struct Item<'a> { id: &'a str, tab: Option<&'a str> }`: fields named in
//! the path fill it (percent-encoded); the rest are query parameters (`None` leaves one out).
//! The path is checked here, so a link cannot name a parameter the route does not have.

use proc_macro::TokenStream;

use crate::{
    derive::{named_fields, parse, string},
    tokens::{err, q, Error},
};

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let item = parse(input, "Route")?;
    if item.is_enum {
        return err(item.name.span(), "Route is derived for a struct: #[route(\"/items/{id}\")] struct Item<'a> { id: &'a str }");
    }
    let Some((path, span)) = item.attr_string("route")? else {
        return err(item.name.span(), "Route needs the path: #[route(\"/items/{id}\")]");
    };
    let segments = parse_path(&path).map_err(|message| (span, message))?;
    let fields = match &item.body {
        Some(body) => named_fields(body)?,
        None => Vec::new(),
    };
    for segment in &segments {
        if let Segment::Param(name) | Segment::Rest(name) = segment {
            if !fields.iter().any(|f| f.name == *name) {
                return err(span, format!("the path has {{{name}}} but {} has no field {name}", item.name));
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
                let function = if matches!(segment, Segment::Rest(_)) { "rest" } else { "segment" };
                body.extend(q(
                    &format!("::corten::route::__private::{function}(&self.__A0, __OUT);"),
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
        body.extend(q("let mut __corten_first = true;", &[]));
        for field in query {
            body.extend(q(
                "(&::corten::route::__private::Query(&self.__A0)).__query(__A1, &mut __corten_first, __corten_html, __OUT);",
                &[proc_macro::TokenTree::Ident(field.ident.clone()).into(), string(&format!("{}=", field.name)).into()],
            ));
        }
    }
    let mut out = item.impl_block(
        "::corten::route::Route",
        q(
            "const PATH: &'static str = __A0;
            #[allow(unused_variables)]
            fn write_href(&self, __OUT: &mut ::std::string::String, __corten_html: bool) {
                #[allow(unused_imports)]
                use ::corten::route::__private::{QueryOption as _, QueryValue as _};
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
