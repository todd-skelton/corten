//! Handler modules: the script next to a `#[component(js)]`, or a shared one from `script!`.
//!
//! The script is read when the crate compiles. Each `export function name(a, b)` becomes a Rust
//! function `name(a, b)` that returns the call the markup puts on an element, and each parameter's
//! JSDoc type (`@param {HTMLButtonElement} button`) becomes the bound on its argument, so
//! `js::enable(rows)` with `rows` on a `<tbody>` does not compile.

use std::path::Path;

use proc_macro::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};

use crate::{
    dom,
    tokens::{err, is_ident, is_punct, literal, q, Error},
};

/// One exported function.
#[derive(Debug)]
pub struct Handler {
    pub name: String,
    /// Each parameter: its name and its Rust bound.
    pub params: Vec<(String, Param)>,
    /// The JSDoc's description, for the Rust function's docs.
    pub doc: String,
}

#[derive(Debug)]
pub enum Param {
    /// A type Corten checks: a path under `::corten::script` (`el::HTMLButtonElement`).
    Checked(String),
    /// Any other JSDoc type: a JSON value, a keyword or a ref.
    Any,
}

/// A script file and its handlers.
pub struct Module {
    /// Its name in the page, the file's stem: `cart_line`.
    pub name: String,
    pub file: String,
    pub path: String,
    pub handlers: Vec<Handler>,
}

impl Module {
    pub fn read(path: &Path, span: Span) -> Result<Module, Error> {
        let source = std::fs::read_to_string(path).map_err(|e| (span, format!("cannot read {}: {e}", path.display())))?;
        let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        let name = file.strip_suffix(".js").unwrap_or(&file).to_string();
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return err(
                span,
                format!("{file}: a script's name is letters, digits, _ and -, since the page names its module by it"),
            );
        }
        let handlers = parse(&source).map_err(|(line, message)| (span, format!("{file}:{line}: {message}")))?;
        Ok(Module { name, file, path: path.display().to_string(), handlers })
    }

    /// `::corten::script::Script { .. }`.
    pub fn literal(&self) -> TokenStream {
        let mut names = TokenStream::new();
        for h in &self.handlers {
            names.extend(literal(&h.name));
            names.extend(q(",", &[]));
        }
        q(
            "::corten::script::Script { name: __A0, file: __A1, source: ::core::include_str!(__A2), handlers: &[__A3] }",
            &[literal(&self.name), literal(&self.file), literal(&self.path), names],
        )
    }

    /// `mod name { pub fn handler(..) -> Call<..> }`. `owner` is the shared module's component,
    /// marked rendered when a handler renders (a component's own script is its component's).
    pub fn module(&self, vis: TokenStream, ident: Ident, owner: TokenStream, items: TokenStream) -> TokenStream {
        let mut fns = items;
        for h in &self.handlers {
            let mut generics = TokenStream::new();
            let mut params = TokenStream::new();
            let mut args = TokenStream::new();
            let mut outs = TokenStream::new();
            for (i, (name, param)) in h.params.iter().enumerate() {
                let ty: TokenStream = TokenTree::Ident(Ident::new(&format!("A{i}"), Span::call_site())).into();
                let arg: TokenStream = TokenTree::Ident(rust_ident(name, i)).into();
                let path: TokenStream = match param {
                    Param::Checked(path) => format!("::corten::script::{path}").parse().expect("a type path parses"),
                    Param::Any => q("::corten::script::ts::Any", &[]),
                };
                generics.extend(q("__A0: ::corten::script::Arg<__A1>,", &[ty.clone(), path.clone()]));
                args.extend(q(
                    "::corten::script::__private::Typed::<__A0, __A1>::new(__A2),",
                    &[ty.clone(), path.clone(), arg.clone()],
                ));
                outs.extend(q("::corten::script::__private::Typed<__A0, __A1>,", &[ty.clone(), path]));
                params.extend(q("__A0: __A1,", &[arg, ty]));
            }
            let doc = if h.doc.is_empty() { format!(" `{}` in {}.", h.name, self.file) } else { format!(" {}", h.doc) };
            fns.extend(q(
                "#[doc = __A0] #[allow(non_snake_case, clippy::too_many_arguments)] #[must_use]
                pub fn __A1<__A2>(__A3) -> ::corten::script::__private::Call<(__A4)> {
                    ::corten::script::__private::Call::new(__A5, __A6, __A7, (__A8))
                }",
                &[
                    literal(&doc),
                    TokenTree::Ident(rust_ident(&h.name, 0)).into(),
                    generics,
                    params,
                    outs,
                    literal(&self.name),
                    literal(&h.name),
                    owner.clone(),
                    args,
                ],
            ));
        }
        let mut out =
            q("#[doc = __A0] #[allow(dead_code, non_snake_case)]", &[literal(&format!(" The handlers in {}.", self.file))]);
        out.extend(vis);
        out.extend(q("mod __A0", &[TokenTree::Ident(ident).into()]));
        out.extend([TokenTree::Group(Group::new(Delimiter::Brace, fns))]);
        out
    }
}

/// A JavaScript name as a Rust identifier: as is, raw for a Rust keyword, `a{i}` otherwise.
fn rust_ident(name: &str, i: usize) -> Ident {
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "dyn", "else", "enum", "extern", "fn", "for", "if", "impl", "in",
        "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "static", "struct", "trait", "type", "unsafe",
        "use", "where", "while", "yield", "macro", "box", "priv", "final", "override", "abstract", "become", "do", "typeof",
        "unsized", "virtual", "try", "gen",
    ];
    let plain = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !plain {
        Ident::new(&format!("a{i}"), Span::call_site())
    } else if matches!(name, "self" | "Self" | "super" | "crate" | "_") {
        Ident::new(&format!("{name}_"), Span::call_site())
    } else if KEYWORDS.contains(&name) {
        Ident::new_raw(name, Span::call_site())
    } else {
        Ident::new(name, Span::call_site())
    }
}

/// The handlers of a script: every top-level `export function`, with its JSDoc. Errors carry
/// the 1-based line.
pub fn parse(source: &str) -> Result<Vec<Handler>, (usize, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut handlers: Vec<Handler> = Vec::new();
    let mut doc: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.starts_with("/**") {
            let mut block = String::new();
            let start = i;
            loop {
                block.push_str(lines[i]);
                block.push('\n');
                if lines[i].contains("*/") {
                    break;
                }
                i += 1;
                if i == lines.len() {
                    return Err((start + 1, "unclosed /** comment".into()));
                }
            }
            doc = Some(block);
            i += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("export ") {
            let rest = rest.trim_start();
            let rest = rest.strip_prefix("async ").map(str::trim_start).unwrap_or(rest);
            let Some(rest) = rest.strip_prefix("function ") else {
                return Err((
                    i + 1,
                    "a handler module exports only functions, `export function name(a, b) { .. }`; keep other code unexported"
                        .into(),
                ));
            };
            let start = i;
            let mut head = rest.to_string();
            while !head.contains(')') {
                i += 1;
                if i == lines.len() {
                    return Err((start + 1, "expected ) to end the parameter list".into()));
                }
                head.push(' ');
                head.push_str(lines[i].trim());
            }
            let (name, params) = head.split_once('(').ok_or((start + 1, "expected ( after the function name".to_string()))?;
            let name = name.trim().trim_start_matches('*').trim();
            let params = &params[..params.find(')').unwrap_or(params.len())];
            if handlers.iter().any(|h| h.name == name) {
                return Err((start + 1, format!("{name} is exported twice")));
            }
            let jsdoc = doc.take().unwrap_or_default();
            let types = param_types(&jsdoc);
            let mut list = Vec::new();
            for param in params.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                if !param.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
                    return Err((
                        start + 1,
                        format!("{name}: parameters are plain names (no destructuring, defaults or ...rest): {param}"),
                    ));
                }
                let Some(ty) = types.iter().find(|(n, _)| n == param).map(|(_, t)| t) else {
                    return Err((
                        start + 1,
                        format!(
                            "{name}({params}): give {param} a type in the JSDoc above it, /** @param {{HTMLElement}} {param} */"
                        ),
                    ));
                };
                list.push((param.to_string(), param_type(ty)));
            }
            handlers.push(Handler { name: name.to_string(), params: list, doc: description(&jsdoc) });
            i += 1;
            continue;
        }
        if !trimmed.is_empty() && !trimmed.starts_with("//") {
            doc = None;
        }
        i += 1;
    }
    Ok(handlers)
}

/// `@param {Type} name` pairs of a JSDoc block, as (name, type).
fn param_types(jsdoc: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = jsdoc;
    while let Some(at) = rest.find("@param") {
        rest = &rest[at + 6..];
        let Some(open) = rest.find('{') else { break };
        if !rest[..open].trim().is_empty() {
            continue;
        }
        let mut depth = 0;
        let mut close = None;
        for (j, c) in rest[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + j);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else { break };
        let ty = rest[open + 1..close].trim().to_string();
        rest = &rest[close + 1..];
        let name: String =
            rest.trim_start().chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$').collect();
        out.push((name, ty));
    }
    out
}

/// The JSDoc's text before its first tag, on one line.
fn description(jsdoc: &str) -> String {
    let mut text = Vec::new();
    for line in jsdoc.lines() {
        let line = line.trim().trim_start_matches("/**").trim_end_matches("*/").trim().trim_start_matches('*').trim();
        if line.starts_with('@') {
            break;
        }
        if !line.is_empty() {
            text.push(line);
        }
    }
    text.join(" ")
}

/// The Rust bound for a JSDoc type.
fn param_type(ty: &str) -> Param {
    let ty = ty.trim().trim_start_matches('(').trim_end_matches(')').trim();
    let element = |t: &str| dom::ELEMENTS.contains(&t);
    let item = ty.strip_suffix("[]").or_else(|| ty.strip_prefix("Array<").and_then(|t| t.strip_suffix('>')));
    if let Some(item) = item {
        let item = item.trim().trim_start_matches('(').trim_end_matches(')').trim();
        return match common(item, element) {
            Some(e) => Param::Checked(format!("ts::Array<::corten::script::el::{e}>")),
            None => Param::Any,
        };
    }
    if let Some(e) = common(ty, element) {
        return Param::Checked(format!("el::{e}"));
    }
    if dom::EVENTS.contains(&ty) {
        return Param::Checked(format!("ev::{ty}"));
    }
    match ty {
        "AbortSignal" => Param::Checked("ts::AbortSignal".into()),
        "string" => Param::Checked("ts::String".into()),
        "number" => Param::Checked("ts::Number".into()),
        "boolean" => Param::Checked("ts::Boolean".into()),
        _ => Param::Any,
    }
}

/// One element interface, or for a union of them the closest one they all are.
fn common(ty: &str, element: impl Fn(&str) -> bool) -> Option<String> {
    let parts: Vec<&str> = ty.split('|').map(str::trim).collect();
    if !parts.iter().all(|p| element(p)) {
        return None;
    }
    if parts.len() == 1 {
        return Some(parts[0].to_string());
    }
    Some(if parts.iter().all(|p| p.starts_with("HTML")) { "HTMLElement" } else { "Element" }.to_string())
}

/// `script!(pub mod validity = "shared/validity.js");`: a handler module shared by components,
/// with a `COMPONENT` the app lists once.
pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut i = 0;
    let mut vis = TokenStream::new();
    if is_ident(tokens.first(), "pub") {
        vis.extend([tokens[0].clone()]);
        i = 1;
        if let Some(TokenTree::Group(g)) = tokens.get(1) {
            if g.delimiter() == Delimiter::Parenthesis {
                vis.extend([tokens[1].clone()]);
                i = 2;
            }
        }
    }
    let usage = "script!(pub mod validity = \"validity.js\");: a module name and the script's path, from this file's directory";
    let (Some(TokenTree::Ident(kw)), Some(TokenTree::Ident(name)), true, Some(TokenTree::Literal(path))) =
        (tokens.get(i), tokens.get(i + 1), is_punct(tokens.get(i + 2), '='), tokens.get(i + 3))
    else {
        return err(tokens.first().map_or(Span::call_site(), TokenTree::span), usage);
    };
    if kw.to_string() != "mod" {
        return err(kw.span(), usage);
    }
    let file = crate::tokens::string_value(path).ok_or((path.span(), usage.to_string()))?;
    let located = crate::style::locate(&file, path.span())?;
    let module = Module::read(&located, path.span())?;
    let items = q(
        "#[doc = \" The module, for `App::components`.\"] pub const COMPONENT: ::corten::Component = ::corten::Component {
            name: __A0, script: ::core::option::Option::Some(__A1), style: ::core::option::Option::None, event: ::core::option::Option::None,
        };",
        &[literal(&module.name), module.literal()],
    );
    Ok(module.module(vis, name.clone(), q("::core::option::Option::Some(&COMPONENT)", &[]), items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_and_jsdoc_types_become_handlers() {
        let source = r#"// @ts-check
import { state } from "corten";

/**
 * Moves the rest into the table.
 * @param {HTMLTableSectionElement} rows
 * @param {HTMLTemplateElement} rest
 * @param {MouseEvent} event
 */
export function showAll(rows, rest, event) {}

/** @param {HTMLTableCellElement[]} cells @param {Stock} stock @param {AbortSignal} signal @param {string} url */
export async function watch(cells, stock, signal, url) {}

function helper() {}
"#;
        let handlers = parse(source).unwrap();
        assert_eq!(handlers.len(), 2);
        assert_eq!(handlers[0].name, "showAll");
        assert_eq!(handlers[0].doc, "Moves the rest into the table.");
        let kinds: Vec<String> = handlers
            .iter()
            .flat_map(|h| h.params.iter())
            .map(|(n, p)| match p {
                Param::Checked(t) => format!("{n}:{t}"),
                Param::Any => format!("{n}:any"),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "rows:el::HTMLTableSectionElement",
                "rest:el::HTMLTemplateElement",
                "event:ev::MouseEvent",
                "cells:ts::Array<::corten::script::el::HTMLTableCellElement>",
                "stock:any",
                "signal:ts::AbortSignal",
                "url:ts::String",
            ]
        );
    }

    #[test]
    fn untyped_parameters_and_other_exports_are_errors() {
        assert!(parse("export function f(a) {}").unwrap_err().1.contains("give a a type"));
        assert!(parse("export const x = 1;").unwrap_err().1.contains("exports only functions"));
        assert!(parse("/** @param {Element} a */\nexport function f({ a }) {}").unwrap_err().1.contains("plain names"));
        let union = parse("/** @param {HTMLAnchorElement | HTMLButtonElement} tab */\nexport function f(tab) {}").unwrap();
        assert!(matches!(&union[0].params[0].1, Param::Checked(t) if t == "el::HTMLElement"));
    }
}
