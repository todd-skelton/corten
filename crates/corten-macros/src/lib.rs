//! Corten's macros, re-exported by the `corten` crate: `html!` and `#[component]` (see
//! `corten::view`), `#[derive(Fields)]` (see `corten::Fields`), `#[derive(Route)]` (see
//! `corten::route`), `#[event]` and `script!` (see `corten::script`), `messages!` (see `corten::i18n`) and
//! `tokens!` and component stylesheets (see `corten::style`).
//!
//! Written against `proc_macro` alone (no `syn` or `quote`), so the crate has no dependencies
//! and builds in about a second.

mod component;
mod derive;
mod dom;
mod event;
mod html;
mod json;
mod messages;
mod names;
mod route;
mod script;
mod style;
mod theme;
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
#[proc_macro_derive(Route, attributes(route))]
pub fn derive_route(input: TokenStream) -> TokenStream {
    route::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
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
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut rename_all = None;
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
    let body = match tokens.get(i) {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => g,
        Some(TokenTree::Punct(p)) if p.as_char() == '<' => return Err("Fields: generic structs are not supported".into()),
        _ => return Err("Fields can only be derived for a struct with named fields".into()),
    };
    let mut names = Vec::new();
    for field in split_fields(body.stream()) {
        let mut rename = None;
        let mut skip = false;
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
    out += &format!("}}\n#[allow(dead_code)]\nimpl {name} {{\n    /// Each field's name as the form posts it.\n    pub const FIELDS: {fields_type} = {fields_type} {{\n");
    for (ident, wire) in &names {
        out += &format!("        {ident}: ::corten::form::FieldName::__new({wire:?}),\n");
    }
    out += "    };\n}\n";
    Ok(out)
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
fn serde_args(attribute: &Group) -> Result<Vec<(String, Option<String>)>, String> {
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
        .ok_or_else(|| format!("Fields: expected a plain string literal, found {literal}"))
}

/// serde's `rename_all` rules, applied to a snake_case field name.
fn apply_rule(field: &str, rule: &str) -> Result<String, String> {
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
        other => return Err(format!("Fields: unknown rename_all rule {other:?}")),
    })
}
