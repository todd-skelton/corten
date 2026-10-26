//! Corten's macros, re-exported by the `corten` crate: `html!` and `#[component]` (see
//! `corten::view`), `#[derive(Fields)]` (see `corten::Fields`), `#[derive(ToJson)]` and `#[derive(FromJson)]` (see `corten::json`), `#[derive(Route)]` (see
//! `corten::route`), `#[derive(Id)]` (see `corten::view::Id`), `#[event]` and `script!` (see `corten::script`), `messages!` (see `corten::i18n`) and
//! `tokens!` and component stylesheets (see `corten::style`).
//!
//! Written against `proc_macro` alone (no `syn` or `quote`), so the crate has no dependencies
//! and builds in about a second.

mod component;
mod derive;
mod dom;
mod event;
mod from_json;
mod html;
mod id;
mod json;
mod locale_data;
mod messages;
mod names;
mod route;
mod routes;
mod script;
mod style;
mod theme;
mod to_json;
mod tokens;

use proc_macro::{Delimiter, Group, Spacing, TokenStream, TokenTree};

/// JSX-like markup compiled to string pushes. Documented at `corten::view`.
#[proc_macro]
pub fn html(input: TokenStream) -> TokenStream {
    html::expand(input)
}

/// A function as a component struct. Documented at `corten::view::component`.
#[proc_macro_attribute]
pub fn component(args: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(args, item).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A struct whose fields are a URL's parameters. Documented at `corten::route`.
#[proc_macro_derive(Route, attributes(route, serde))]
pub fn derive_route(input: TokenStream) -> TokenStream {
    route::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A module of routes and the trait of their handlers. Documented at `corten::route::routes`.
#[proc_macro_attribute]
pub fn routes(args: TokenStream, item: TokenStream) -> TokenStream {
    routes::expand(args, item).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// An element id as a type. Documented at `corten::view::Id`.
#[proc_macro_derive(Id)]
pub fn derive_id(input: TokenStream) -> TokenStream {
    id::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A struct or an enum written as JSON, laid out when the crate compiles. Documented at
/// `corten::json`.
#[proc_macro_derive(ToJson, attributes(serde))]
pub fn derive_to_json(input: TokenStream) -> TokenStream {
    to_json::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A struct, or an enum of unit variants, read from JSON with serde_json as the fallback.
/// Documented at `corten::json`.
#[proc_macro_derive(FromJson, attributes(serde))]
pub fn derive_from_json(input: TokenStream) -> TokenStream {
    from_json::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A struct as a typed DOM event. Documented at `corten::script`.
#[proc_macro_attribute]
pub fn event(args: TokenStream, item: TokenStream) -> TokenStream {
    event::expand(args, item).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A handler module shared by components. Documented at `corten::script`.
#[proc_macro]
pub fn script(input: TokenStream) -> TokenStream {
    script::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A design-token file as CSS custom properties, typed scales and utility classes. Documented
/// at `corten::style`.
#[proc_macro]
pub fn tokens(input: TokenStream) -> TokenStream {
    theme::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// Message catalogs checked and compiled into a type. Documented at `corten::i18n`.
#[proc_macro]
pub fn messages(input: TokenStream) -> TokenStream {
    messages::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// Every locale's number, currency and date data, for corten's test against Intl. Not a public API.
#[doc(hidden)]
#[proc_macro]
pub fn __all_locales(_: TokenStream) -> TokenStream {
    messages::all_locales()
}

/// Field names of a form input struct as constants: `Input::FIELDS.email` is the
/// `corten::form::FieldName` `"email"`, with serde's `rename` and `rename_all` applied. Templates
/// print them for `name=` and handlers pass them to `Errors`, so a renamed or misspelled field is a
/// compile error.
#[proc_macro_derive(Fields, attributes(serde))]
pub fn derive_fields(input: TokenStream) -> TokenStream {
    match fields(input) {
        Ok(code) => code.parse().expect("generated code parses"),
        Err(message) => format!("::core::compile_error!({message:?});").parse().expect("compile_error parses"),
    }
}

fn fields(input: TokenStream) -> Result<String, String> {
    let source = input.clone();
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut rename_all = None;
    let mut container_default = false;
    let mut visibility = String::new();
    let mut i = 0;
    let name = loop {
        match tokens.get(i) {
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                if let Some(TokenTree::Group(group)) = tokens.get(i + 1) {
                    for (key, value) in serde_args(group)? {
                        if key == "rename_all" {
                            rename_all = Some(value.ok_or("Fields: rename_all needs a value, like rename_all = \"camelCase\"")?);
                        }
                        container_default |= key == "default";
                    }
                }
                i += 2;
            }
            Some(TokenTree::Ident(id)) if id.to_string() == "pub" => {
                visibility = "pub".into();
                if let Some(TokenTree::Group(g)) = tokens.get(i + 1) {
                    if g.delimiter() == Delimiter::Parenthesis {
                        visibility = format!("pub({})", g.stream());
                        i += 1;
                    }
                }
                i += 1;
            }
            Some(TokenTree::Ident(id)) if id.to_string() == "struct" => match tokens.get(i + 1) {
                Some(TokenTree::Ident(name)) => {
                    i += 2;
                    break name.to_string();
                }
                _ => return Err("Fields: expected a struct name".into()),
            },
            Some(TokenTree::Ident(id)) if id.to_string() == "enum" || id.to_string() == "union" => {
                return Err("Fields can only be derived for a struct with named fields".into())
            }
            Some(_) => i += 1,
            None => return Err("Fields: expected a struct".into()),
        }
    };
    // A struct with generics (a query struct that borrows): the names do not depend on them.
    let generic = matches!(tokens.get(i), Some(TokenTree::Punct(p)) if p.as_char() == '<');
    let item = if generic { Some(derive::parse(source, "Fields").map_err(|(_, message)| message)?) } else { None };
    let named = "Fields can only be derived for a struct with named fields";
    let body = match (&item, tokens.get(i)) {
        (Some(item), _) => item.body.clone().ok_or(named)?,
        (None, Some(TokenTree::Group(g))) if g.delimiter() == Delimiter::Brace => g.clone(),
        _ => return Err(named.into()),
    };
    let (impl_generics, type_generics, where_clause) = match &item {
        Some(item) => (item.impl_generics.to_string(), item.type_generics.to_string(), item.where_clause.to_string()),
        None => Default::default(),
    };
    let mut names = Vec::new();
    for field in split_fields(body.stream()) {
        let mut rename = None;
        let mut skip = false;
        let mut default = container_default;
        let mut ident = None;
        let mut j = 0;
        while j < field.len() {
            match &field[j] {
                TokenTree::Punct(p) if p.as_char() == '#' => {
                    if let Some(TokenTree::Group(group)) = field.get(j + 1) {
                        for (key, value) in serde_args(group)? {
                            match key.as_str() {
                                "rename" => {
                                    rename = Some(
                                        value
                                            .ok_or("Fields: use rename = \"name\"; rename(deserialize = ..) is not supported")?,
                                    )
                                }
                                "skip" | "skip_deserializing" => skip = true,
                                "default" => default = true,
                                "flatten" => {
                                    return Err(
                                        "Fields: #[serde(flatten)] is not supported; list the fields in the form struct".into()
                                    )
                                }
                                _ => {}
                            }
                        }
                    }
                    j += 2;
                }
                TokenTree::Ident(id) if id.to_string() == "pub" => {
                    j += 1;
                    if matches!(field.get(j), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis) {
                        j += 1;
                    }
                }
                TokenTree::Ident(id) => {
                    ident = Some(id.to_string());
                    // After the name and its `:`, the type.
                    if !default {
                        if let Some(what) = left_out_when_empty(field.get(j + 2..).unwrap_or_default()) {
                            return Err(format!(
                                "Fields: `{id}` is {what}; add #[serde(default)] so the form reads it as {}",
                                if what.starts_with("a bool") { "false" } else { "empty" }
                            ));
                        }
                    }
                    break;
                }
                _ => j += 1,
            }
        }
        let Some(ident) = ident else { continue };
        if skip {
            continue;
        }
        let bare = ident.strip_prefix("r#").unwrap_or(&ident).to_string();
        let wire = match (rename, &rename_all) {
            (Some(name), _) => name,
            (None, Some(rule)) => apply_rule(&bare, rule)?,
            (None, None) => bare.clone(),
        };
        names.push((ident, wire));
    }
    let fields_type = format!("{name}Fields");
    let mut out = format!(
        "/// Form field names of [`{name}`], from `#[derive(Fields)]`.\n#[derive(Clone, Copy, Debug)]\n#[allow(dead_code)]\n{visibility} struct {fields_type} {{\n"
    );
    for (ident, _) in &names {
        out += &format!("    pub {ident}: ::corten::form::FieldName,\n");
    }
    out += &format!("}}\n#[allow(dead_code)]\nimpl {impl_generics} {name} {type_generics} {where_clause} {{\n    /// Each field's name as the form posts it.\n    pub const FIELDS: {fields_type} = {fields_type} {{\n");
    for (ident, wire) in &names {
        out += &format!("        {ident}: ::corten::form::FieldName::__new({wire:?}),\n");
    }
    out += "    };\n}\n";
    Ok(out)
}

/// What a browser leaves out of the form when it has no value, for a field type that is a
/// `bool` or a `Vec`: the key is missing, so without a default the form fails to read.
fn left_out_when_empty(ty: &[TokenTree]) -> Option<&'static str> {
    let name = ty.iter().take_while(|t| !matches!(t, TokenTree::Punct(p) if p.as_char() == '<')).filter_map(|t| match t {
        TokenTree::Ident(id) => Some(id.to_string()),
        _ => None,
    });
    match name.last().as_deref() {
        Some("bool") if ty.len() == 1 => Some("a bool, and a browser sends nothing for an unchecked checkbox"),
        Some("Vec") => Some("a list, and a browser sends nothing for a list with nothing checked or selected"),
        _ => None,
    }
}

/// Splits a brace group's tokens at top-level commas. Commas inside `<..>` belong to a type.
fn split_fields(stream: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut fields = vec![Vec::new()];
    let mut depth = 0i32;
    let mut previous_dash = false;
    for token in stream {
        if let TokenTree::Punct(p) = &token {
            match p.as_char() {
                '<' => depth += 1,
                // `->` in a fn pointer type is not a closing angle bracket.
                '>' if !previous_dash => depth -= 1,
                ',' if depth == 0 => {
                    fields.push(Vec::new());
                    previous_dash = false;
                    continue;
                }
                _ => {}
            }
            previous_dash = p.as_char() == '-' && p.spacing() == Spacing::Joint;
        } else {
            previous_dash = false;
        }
        fields.last_mut().expect("never empty").push(token);
    }
    fields.retain(|f| !f.is_empty());
    fields
}

/// `key` or `key = "value"` pairs of a `#[serde(...)]` attribute group; other attributes yield nothing.
pub(crate) fn serde_args(attribute: &Group) -> Result<Vec<(String, Option<String>)>, String> {
    let tokens: Vec<TokenTree> = attribute.stream().into_iter().collect();
    let (Some(TokenTree::Ident(path)), Some(TokenTree::Group(args))) = (tokens.first(), tokens.get(1)) else {
        return Ok(Vec::new());
    };
    if path.to_string() != "serde" {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let args: Vec<TokenTree> = args.stream().into_iter().collect();
    let mut k = 0;
    while k < args.len() {
        if let TokenTree::Ident(key) = &args[k] {
            let key = key.to_string();
            match (args.get(k + 1), args.get(k + 2)) {
                (Some(TokenTree::Punct(eq)), Some(TokenTree::Literal(value))) if eq.as_char() == '=' => {
                    out.push((key, Some(unquote(&value.to_string())?)));
                    k += 3;
                }
                (Some(TokenTree::Group(_)), _) => {
                    out.push((key, None));
                    k += 2;
                }
                _ => {
                    out.push((key, None));
                    k += 1;
                }
            }
        } else {
            k += 1;
        }
    }
    Ok(out)
}

fn unquote(literal: &str) -> Result<String, String> {
    literal
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .filter(|s| !s.contains('\\'))
        .map(str::to_string)
        .ok_or_else(|| format!("expected a plain string literal, found {literal}"))
}

/// serde's `rename_all` rules, applied to a snake_case field name.
pub(crate) fn apply_rule(field: &str, rule: &str) -> Result<String, String> {
    let words: Vec<&str> = field.split('_').filter(|w| !w.is_empty()).collect();
    let capitalized = |w: &str| {
        let mut c = w.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    };
    Ok(match rule {
        "lowercase" => field.to_lowercase(),
        "UPPERCASE" => field.to_uppercase(),
        "snake_case" => field.to_string(),
        "SCREAMING_SNAKE_CASE" => field.to_uppercase(),
        "kebab-case" => field.replace('_', "-"),
        "SCREAMING-KEBAB-CASE" => field.replace('_', "-").to_uppercase(),
        "PascalCase" => words.iter().map(|w| capitalized(w)).collect(),
        "camelCase" => {
            let pascal: String = words.iter().map(|w| capitalized(w)).collect();
            let mut c = pascal.chars();
            c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
        other => return Err(format!("unknown rename_all rule {other:?}")),
    })
}
