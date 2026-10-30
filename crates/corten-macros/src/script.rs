//! Handler modules: the script next to a `#[component(js)]`, or a shared one from `script!`.
//!
//! The script is read when the crate compiles. Each `export function name(a, b)` becomes a Rust
//! function `name(a, b)` that returns the call the markup puts on an element, and each parameter's
//! JSDoc type (`@param {HTMLButtonElement} button`) becomes the bound on its argument, so
//! `js::enable(rows)` with `rows` on a `<tbody>` does not compile.
//!
//! A `@typedef` of elements (a group, `@typedef {object} Selection` with `@property` lines)
//! becomes a Rust struct `js::Selection { .. }` the markup builds and passes as one argument, and
//! a class imported from `"corten/events"` is the Rust `#[event]` type of the same name.

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

#[derive(Debug, PartialEq)]
pub enum Param {
    /// A type Corten checks: a path under `::corten::script` (`el::HTMLButtonElement`).
    Checked(String),
    /// A group, a `@typedef` of the same script: the struct `js::Name`.
    Group(String),
    /// A typed event imported from `corten/events`: the Rust `#[event]` type in the component's
    /// module.
    Event(String),
    /// `FeedUrl<Level>`: the URL of a feed of the Rust type `Level` in the component's module.
    Feed(String),
    /// A type `corten::types` generated, `import("../types/app").SortBy`: a value of the Rust type
    /// `SortBy` in the component's module.
    Generated(String),
    /// A type Corten would check but cannot read (`FeedUrl<Level[]>`): why, for the error.
    Unreadable(String),
    /// Any other JSDoc type: a JSON value.
    Any,
}

/// A `@typedef` whose properties include elements: one argument made of several.
#[derive(Debug)]
pub struct ArgGroup {
    pub name: String,
    pub fields: Vec<(String, Param)>,
    pub doc: String,
}

/// A script file and its handlers.
pub struct Module {
    /// Its name in the page: the file's stem, after the crate's prefix (`ui.combo_box`).
    pub name: String,
    pub file: String,
    pub path: String,
    pub handlers: Vec<Handler>,
    pub groups: Vec<ArgGroup>,
    /// It imports `keep` from `corten`: its handlers keep their component's state, so the
    /// component is scoped.
    pub keeps: bool,
    /// The classes it imports from `corten/events` when it runs (not a JSDoc type's), each the
    /// Rust `#[event]` in scope where the script is read: served with the script.
    pub imports: Vec<String>,
    /// The script modules it imports from its own directory (`import { load } from "./stripe.js"`
    /// is `stripe`), which a page that loads it preloads with it.
    pub siblings: Vec<String>,
}

impl Module {
    pub fn read(path: &Path, span: Span) -> Result<Module, Error> {
        let source = std::fs::read_to_string(path).map_err(|e| (span, format!("cannot read {}: {e}", path.display())))?;
        let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        let stem = file.strip_suffix(".js").unwrap_or(&file).to_string();
        let plain = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !plain(&stem) {
            return err(
                span,
                format!("{file}: a script's name is letters, digits, _ and -, since the page names its module by it"),
            );
        }
        let prefix = prefix();
        let name = match prefix.clone() {
            Some(prefix) if !plain(&prefix) => {
                return err(
                    span,
                    format!("[package.metadata.corten] scripts = \"{prefix}\": a prefix is letters, digits, _ and -"),
                )
            }
            Some(prefix) => format!("{prefix}.{stem}"),
            None => stem,
        };
        let at = |(line, message): (usize, String)| (span, format!("{file}:{line}: {message}"));
        let (handlers, groups, imports, siblings) = parse_all(&source).map_err(at)?;
        // A library's scripts are served by their prefixed names, so a sibling is imported by its.
        if let Some(prefix) = &prefix {
            if let Some(other) = siblings.iter().find(|s| !s.starts_with(&format!("{prefix}."))) {
                return err(
                    span,
                    format!("{file}: imports ./{other}.js, but this crate's scripts are served as {prefix}.{{name}}.js: write ./{prefix}.{other}.js"),
                );
            }
        }
        let keeps = keeps(&source).map_err(at)?;
        Ok(Module { name, file, path: path.display().to_string(), handlers, groups, keeps, imports, siblings })
    }

    /// `::corten::script::Script { .. }`, where the events it imports are named by `at` and
    /// their names (`""` in the component's module, `"super::"` in a `script!` module).
    pub fn literal(&self, at: &str) -> TokenStream {
        let mut names = TokenStream::new();
        for h in &self.handlers {
            names.extend(literal(&h.name));
            names.extend(q(",", &[]));
        }
        let mut events = TokenStream::new();
        // Each import is an `#[event]`, not another component's constant.
        let mut checks = TokenStream::new();
        for class in &self.imports {
            events.extend(format!("{at}{class},").parse::<TokenStream>().expect("a path parses"));
            checks.extend(
                format!("const _: fn() = ::corten::script::__private::is_event::<{at}{class}>;")
                    .parse::<TokenStream>()
                    .expect("a path parses"),
            );
        }
        let mut siblings = TokenStream::new();
        for name in &self.siblings {
            siblings.extend(literal(name));
            siblings.extend(q(",", &[]));
        }
        q(
            "{ __A5 ::corten::script::Script { name: __A0, file: __A1, source: ::core::include_str!(__A2), handlers: &[__A3], events: &[__A4], imports: &[__A6] } }",
            &[literal(&self.name), literal(&self.file), literal(&self.path), names, events, checks, siblings],
        )
    }

    /// `mod name { pub fn handler(..) -> Call<..> }`. `owner` is the shared module's component,
    /// marked rendered when a handler renders (a component's own script is its component's).
    /// `home` is the scoped component the handlers belong to, when it is known here (a scoped
    /// component's own script); otherwise the one rendering when the call is made.
    pub fn module(
        &self,
        vis: TokenStream,
        ident: Ident,
        owner: TokenStream,
        home: Option<&str>,
        items: TokenStream,
    ) -> TokenStream {
        let home = match home {
            Some(scope) => q("::core::option::Option::Some(__A0)", &[literal(scope)]),
            None => q("::corten::script::__private::current_scope()", &[]),
        };
        let mut fns = items;
        for g in &self.groups {
            fns.extend(group(g, &self.file));
        }
        for h in &self.handlers {
            let mut generics = TokenStream::new();
            let mut params = TokenStream::new();
            let mut args = TokenStream::new();
            let mut outs = TokenStream::new();
            for (i, (name, param)) in h.params.iter().enumerate() {
                let ty: TokenStream = TokenTree::Ident(Ident::new(&format!("A{i}"), Span::call_site())).into();
                let arg: TokenStream = TokenTree::Ident(rust_ident(name, i)).into();
                let path = param_path(param);
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
                    ::corten::script::__private::Call::new(__A5, __A6, __A9, __A10, __A7, (__A8))
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
                    home.clone(),
                    q(if self.keeps { "true" } else { "false" }, &[]),
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

/// The Rust type a parameter's argument is checked against.
fn param_path(param: &Param) -> TokenStream {
    match param {
        Param::Checked(path) => format!("::corten::script::{path}").parse().expect("a type path parses"),
        Param::Group(name) => TokenTree::Ident(Ident::new(&format!("__{name}Group"), Span::call_site())).into(),
        // The `#[event]` type, in the module of the component or `script!` that reads the script.
        Param::Event(class) => format!("super::{class}").parse().expect("a type path parses"),
        Param::Feed(rows) => format!("::corten::script::ts::FeedUrl<super::{rows}>").parse().expect("a type path parses"),
        Param::Generated(name) => format!("::corten::script::ts::Generated<super::{name}>").parse().expect("a type path parses"),
        Param::Unreadable(_) => q("::corten::script::ts::Any", &[]),
        Param::Any => q("::corten::script::ts::Any", &[]),
    }
}

/// A group: `pub struct Selection<A0: Arg<..>, ..> { pub rows: A0, .. }`, a marker type its
/// handlers' parameters name, and how it is written: `{rows:*row,all:@all}`.
fn group(g: &ArgGroup, file: &str) -> TokenStream {
    let name: TokenStream = TokenTree::Ident(Ident::new(&g.name, Span::call_site())).into();
    let marker = param_path(&Param::Group(g.name.clone()));
    let mut bounded = TokenStream::new();
    let mut plain = TokenStream::new();
    let mut fields = TokenStream::new();
    let mut writes = TokenStream::new();
    for (i, (field, param)) in g.fields.iter().enumerate() {
        let ty: TokenStream = TokenTree::Ident(Ident::new(&format!("A{i}"), Span::call_site())).into();
        let path = param_path(param);
        let ident: TokenStream = TokenTree::Ident(rust_ident(field, i)).into();
        bounded.extend(q("__A0: ::corten::script::Arg<__A1>,", &[ty.clone(), path.clone()]));
        plain.extend(q("__A0,", std::slice::from_ref(&ty)));
        fields.extend(q("pub __A0: __A1,", &[ident.clone(), ty.clone()]));
        writes.extend(q(
            "::corten::script::__private::group_field::<__A0, __A1>(__A2, __A3, &self.__A4, ctx, out);",
            &[ty, path, q(if i == 0 { "true" } else { "false" }, &[]), literal(field), ident],
        ));
    }
    let doc =
        if g.doc.is_empty() { format!(" `{}` in {file}: one argument made of several.", g.name) } else { format!(" {}", g.doc) };
    q(
        "#[doc = __A0] #[derive(Clone, Copy)] pub struct __A1<__A2> { __A3 }
        #[doc(hidden)] pub enum __A4 {}
        impl<__A2> ::corten::script::Arg<__A4> for __A1<__A5> {
            fn write_arg(&self, ctx: &::corten::script::__private::Ctx, out: &mut ::std::string::String) {
                out.push('{');
                __A6
                out.push('}');
            }
        }",
        &[literal(&doc), name, bounded, fields, marker, plain, writes],
    )
}

/// `[package.metadata.corten] scripts = "ui"` in the crate's Cargo.toml: a library's prefix for
/// the names it puts in the page, its scripts (`ui.combo_box`) and its components' roots
/// (`data-scope="ui-combo-box"`), so they never meet an app's.
pub fn prefix() -> Option<String> {
    let dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let manifest = std::fs::read_to_string(Path::new(&dir).join("Cargo.toml")).ok()?;
    let mut in_section = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == "[package.metadata.corten]";
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if key.trim() == "scripts" {
                return Some(value.trim().trim_matches('"').to_string());
            }
        }
    }
    None
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

/// True when the script imports `keep` from `corten` (`import { keep, subscribe } from "corten";`).
/// Any other import of `corten` in code (a namespace or an `import("corten")`) is an error, since it
/// would hide that.
pub fn keeps(source: &str) -> Result<bool, (usize, String)> {
    let by_name = "import corten's functions by name: import { keep } from \"corten\";";
    let lines: Vec<&str> = source.lines().collect();
    // Code without its comments: `//` to the end of the line and `/* .. */` (to the end of the line
    // when it closes on a later one), outside strings.
    let code = |line: &str| -> String {
        let mut out = String::new();
        let mut quote = None;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None if c == '"' || c == '\'' || c == '`' => quote = Some(c),
                None if c == '/' && chars.peek() == Some(&'/') => break,
                None if c == '/' && chars.peek() == Some(&'*') => {
                    chars.next();
                    let mut prev = ' ';
                    let mut closed = false;
                    for c in chars.by_ref() {
                        if prev == '*' && c == '/' {
                            closed = true;
                            break;
                        }
                        prev = c;
                    }
                    if !closed {
                        break;
                    }
                    out.push(' ');
                    continue;
                }
                None => {}
            }
            out.push(c);
        }
        out.trim().to_string()
    };
    let mut keeps = false;
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        // A line inside a block comment (`* @param ..`).
        if trimmed.starts_with('*') {
            i += 1;
            continue;
        }
        let line = code(trimmed);
        let named = |text: &str| text.contains("\"corten\"") || text.contains("'corten'");
        if line.starts_with("import ") || line.starts_with("import{") {
            let start = i;
            let mut statement = line;
            // Until it ends with its module's name: `import {` and a name a line run on.
            while !statement.ends_with([';', '"', '\'']) && i + 1 < lines.len() {
                i += 1;
                statement.push(' ');
                statement.push_str(&code(lines[i]));
            }
            let from = statement.rsplit_once("from").map(|(_, from)| from.trim().trim_end_matches(';').trim());
            if matches!(from, Some("\"corten\"" | "'corten'")) {
                let Some((_, rest)) = statement.split_once('{') else {
                    return Err((start + 1, by_name.into()));
                };
                let names = rest.split_once('}').map_or(rest, |(names, _)| names);
                keeps |= names.split(',').any(|name| name.split(" as ").next().is_some_and(|n| n.trim() == "keep"));
            }
        } else if line.contains("import(") && named(&line) {
            return Err((i + 1, by_name.into()));
        }
        i += 1;
    }
    Ok(keeps)
}

/// The handlers of a script, every top-level `export function` with its JSDoc, and its groups.
/// Errors carry the 1-based line.
#[cfg(test)]
pub fn parse(source: &str) -> Result<(Vec<Handler>, Vec<ArgGroup>), (usize, String)> {
    parse_all(source).map(|(handlers, groups, ..)| (handlers, groups))
}

/// A script's handlers, groups and the classes it imports from `corten/events` when it runs.
type Parsed = (Vec<Handler>, Vec<ArgGroup>, Vec<String>, Vec<String>);

/// A line of an import without its comments (`// ..` to the end, `/* .. */`), which may sit
/// between the names.
fn without_comments(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find('/') {
        match rest[at + 1..].chars().next() {
            Some('/') => {
                out.push_str(&rest[..at]);
                return out.trim_end().to_string();
            }
            Some('*') => {
                out.push_str(&rest[..at]);
                rest = rest[at + 2..].split_once("*/").map_or("", |(_, after)| after);
            }
            _ => {
                out.push_str(&rest[..=at]);
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    out.trim_end().to_string()
}

/// The module a static import names when it is a script of the same directory:
/// `import { a } from "./stripe.js";` and `import "./stripe.js";` are `stripe`. Another path (a
/// static file, `../app.js`) or a bare name (`corten`) is not one.
fn sibling(statement: &str) -> Option<String> {
    let rest = statement.rsplit_once(" from ").map_or_else(|| statement.trim_start_matches("import"), |(_, r)| r).trim();
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let specifier = &rest[1..1 + rest[1..].find(quote)?];
    let stem = specifier.strip_prefix("./")?.strip_suffix(".js")?;
    let plain = !stem.is_empty() && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
    plain.then(|| stem.to_string())
}

/// [`parse`], the classes the script imports from `corten/events` when it runs, and the script
/// modules of its directory it imports ([`sibling`]).
fn parse_all(source: &str) -> Result<Parsed, (usize, String)> {
    let lines: Vec<&str> = source.lines().collect();
    // Raw JSDoc types first: a typedef may come after the handler that uses it.
    /// An export: its name, its parameters' names and JSDoc types, its description.
    struct Raw {
        name: String,
        params: Vec<(String, String)>,
        doc: String,
        line: usize,
    }
    /// A `@typedef`: its name, type, `@property` lines, description and line.
    struct Typedef {
        name: String,
        ty: String,
        props: Vec<(String, String)>,
        doc: String,
        line: usize,
    }
    let mut raw: Vec<Raw> = Vec::new();
    let mut typedefs: Vec<Typedef> = Vec::new();
    let mut events: Vec<(String, String)> = Vec::new();
    // `@typedef {import("../types/app").SortBy} SortBy` and `@typedef {"a" | "b"} Mode`: a
    // handler parameter of that name is the type it names.
    let mut aliases: Vec<(String, String)> = Vec::new();
    let mut imports: Vec<String> = Vec::new();
    let mut siblings: Vec<String> = Vec::new();
    let mut doc: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        // A plain comment: an import in it is not one.
        if trimmed.starts_with("/*") && !trimmed.starts_with("/**") {
            while !lines[i].contains("*/") && i + 1 < lines.len() {
                i += 1;
            }
            doc = None;
            i += 1;
            continue;
        }
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
            if let Some((name, ty)) = tag_type(&block, "@typedef") {
                match ty.strip_prefix("import(\"corten/events\").") {
                    Some(class) => events.push((name, class.trim().to_string())),
                    // A name for another type (a generated declaration, a set of names, or another
                    // typedef): parameters typed by it are typed by what it names.
                    // Any type but an object's (`{ a: T }` or `object` with `@property`s) is one: `Tab | null`
                    // and `Tab[]` are checked where a parameter names them, as they are written there.
                    // A generic one (`@template`) is a definition of its own (`Holds<Row, El>`), not
                    // a name for another type.
                    None if object_literal(&ty).is_none()
                        && !matches!(ty.trim(), "object" | "Object")
                        && !block.contains("@template") =>
                    {
                        aliases.push((name, ty))
                    }
                    None => typedefs.push(Typedef {
                        name,
                        ty,
                        props: tag_types(&block, "@property"),
                        doc: description(&block),
                        line: start + 1,
                    }),
                }
                doc = None;
            } else {
                doc = Some(block);
            }
            i += 1;
            continue;
        }
        if trimmed.starts_with("import ") || trimmed.starts_with("import{") {
            let start = i;
            let mut statement = without_comments(trimmed);
            // Ends at its `from`, a `;`, or the closing quote of an import with neither.
            while !statement.contains(" from ") && !statement.ends_with([';', '"', '\'']) && i + 1 < lines.len() {
                i += 1;
                statement.push(' ');
                statement.push_str(&without_comments(lines[i].trim()));
            }
            if let Some(name) = sibling(&statement).filter(|n| !siblings.contains(n)) {
                siblings.push(name);
            }
            if statement.contains("\"corten/events\"") || statement.contains("'corten/events'") {
                let names = statement.split_once('{').and_then(|(_, rest)| rest.split_once('}')).map(|(names, _)| names);
                let Some(names) = names else {
                    return Err((start + 1, "import typed events by name: import { RowToggled } from \"corten/events\";".into()));
                };
                for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                    let (class, local) = name.split_once(" as ").map_or((name, name), |(c, l)| (c.trim(), l.trim()));
                    events.push((local.to_string(), class.to_string()));
                    if !imports.iter().any(|c| c == class) {
                        imports.push(class.to_string());
                    }
                }
            }
            doc = None;
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
            if raw.iter().any(|h| h.name == name) {
                return Err((start + 1, format!("{name} is exported twice")));
            }
            let jsdoc = doc.take().unwrap_or_default();
            let types = tag_types(&jsdoc, "@param");
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
                list.push((param.to_string(), ty.clone()));
            }
            raw.push(Raw { name: name.to_string(), params: list, doc: description(&jsdoc), line: start + 1 });
            i += 1;
            continue;
        }
        if !trimmed.is_empty() && !trimmed.starts_with("//") {
            doc = None;
        }
        i += 1;
    }

    // A typedef is a group when one of its properties is not JSON: something only the browser
    // has (an element, an event, a signal) or a catalog message; one of plain values stays a
    // JSON value.
    let event_of = |ty: &str| events.iter().find(|(local, _)| local == ty).map(|(_, class)| class.clone());
    // What a type names once every typedef of a typedef is followed (at most one step per alias,
    // so a cycle ends).
    let resolve = |ty: &str| {
        let mut ty = ty.trim().to_string();
        for _ in 0..=aliases.len() {
            match aliases.iter().find(|(local, _)| *local == ty) {
                Some((_, aliased)) => ty = aliased.trim().to_string(),
                None => break,
            }
        }
        ty
    };
    // A type with each alias inside it written out (`Tab[]` as `import("../types/app").Tab[]`), so
    // a generated declaration or a set of names is seen wherever it is.
    let expand = |ty: &str| expand_aliases(&aliases, &resolve(ty), 0);
    let plain_type = |ty: &str| {
        let ty = expand(ty);
        match param_type(&ty) {
            Param::Any => event_of(&ty).map_or(Param::Any, Param::Event),
            other => other,
        }
    };
    let union_error = |what: &str, ty: &str| {
        format!("{what}: a value that is one of several names ({ty}) is a Rust enum the markup passes: derive ToJson and JsonSchema on it, add it with Types::add, and type it with its generated declaration, import(\"../types/app\").Name")
    };
    let mut groups = Vec::new();
    let mut unions: Vec<(String, (usize, String))> = Vec::new();
    // Typedefs of plain values, each with its properties' types joined, so a handler that takes
    // one with a generated declaration in it is refused as one written inline is.
    let mut plain: Vec<(String, String)> = Vec::new();
    for Typedef { name, ty, mut props, doc, line } in typedefs {
        if let Some(fields) = object_literal(&ty) {
            props = fields;
        } else if !matches!(ty.as_str(), "object" | "Object") {
            continue;
        }
        let fields: Vec<(String, Param)> = props.iter().map(|(n, t)| (n.clone(), plain_type(t))).collect();
        // Only a group is checked: a typedef of plain values (one the script keeps for itself, or
        // a JSON value a handler takes) is the script's to type.
        // (A generated declaration the call could not be checked against does not make one: an
        // unreadable `FeedUrl` does, as any feed's URL.)
        let group = fields
            .iter()
            .zip(&props)
            .any(|((_, p), (_, t))| not_json(p) && !(matches!(p, Param::Unreadable(_)) && mentions_generated(&expand(t))));
        if !group {
            plain.push((name.clone(), props.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(", ")));
            continue;
        }
        // A set of names, or a type a call cannot be checked against, in a group is refused only
        // when a handler takes the group: a typedef the script keeps for itself (its own state,
        // with an element in it) is the script's to type.
        if let Some((field, ty)) = props.iter().find(|(_, t)| holds_names_union(&expand(t))) {
            unions.push((name.clone(), (line, union_error(&format!("{name}: {field}"), ty))));
        }
        if let Some((_, Param::Unreadable(why))) = fields.iter().find(|(_, p)| matches!(p, Param::Unreadable(_))) {
            unions.push((name.clone(), (line, why.clone())));
        }
        for (field, _) in &fields {
            if !field.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                || !field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err((line, format!("{name}: a group's properties are plain names, and {field} is not")));
            }
        }
        groups.push(ArgGroup { name, fields, doc });
    }
    let mut handlers = Vec::new();
    for Raw { name, params, doc, line } in raw {
        let mut list = Vec::new();
        for (param, ty) in params {
            let ty = ty.trim().to_string();
            // A name from a fixed set, passed by the markup, is a Rust enum whose declaration
            // `corten::types` generates: the call is then checked, and a renamed variant fails to
            // build. A string literal union would take any value.
            let named = resolve(&ty);
            if holds_names_union(&expand(&ty)) {
                return Err((line, union_error(&format!("{name}: {param}"), &ty)));
            }
            let resolved = if groups.iter().any(|g| g.name == named) {
                if let Some((_, (at, why))) = unions.iter().find(|(group, _)| *group == named) {
                    return Err((*at, why.clone()));
                }
                Param::Group(named.clone())
            } else {
                plain_type(&ty)
            };
            if let Param::Unreadable(why) = resolved {
                return Err((line, format!("{name}: {param}: {why}")));
            }
            // A generated declaration in a shape the call is not checked against (inside a list
            // with `null`, a tuple, a typedef of plain values) is refused rather than taking any
            // value.
            if matches!(resolved, Param::Any)
                && (mentions_generated(&expand(&ty)) || plain.iter().any(|(n, t)| *n == named && mentions_generated(&expand(t))))
            {
                return Err((
                    line,
                    format!("{name}: {param}: {ty} holds a generated declaration in a shape the call is not checked against; take the generated type itself (import(\"../types/app\").Name), a list of it, or a group typedef"),
                ));
            }
            list.push((param, resolved));
        }
        handlers.push(Handler { name, params: list, doc });
    }
    Ok((handlers, groups, imports, siblings))
}

/// `ty` with each alias inside it written out, and the aliases inside those (at most one level per
/// alias, so a cycle ends): `Tabs | null` with `Tabs = Tab[]` is `(import("../types/app").Tab[]) | null`.
fn expand_aliases(aliases: &[(String, String)], ty: &str, depth: usize) -> String {
    let mut out = String::with_capacity(ty.len());
    let (mut word, mut quote, mut after_dot) = (String::new(), None, false);
    let flush = |word: &mut String, out: &mut String, after_dot: bool| {
        match aliases.iter().find(|(local, _)| *local == *word).filter(|_| !after_dot && depth <= aliases.len()) {
            // In parentheses, so `Mode | null` stays one set and `null`, and `Tabs[]` a list of it.
            Some((_, target)) => {
                let target = expand_aliases(aliases, target.trim(), depth + 1);
                if names_union(&target) || target.contains(['|', '[', '<']) {
                    out.push('(');
                    out.push_str(&target);
                    out.push(')');
                } else {
                    out.push_str(&target);
                }
            }
            None => out.push_str(word),
        }
        word.clear();
    };
    for c in ty.chars() {
        match (c, quote) {
            (c, Some(q)) => {
                out.push(c);
                if c == q {
                    quote = None;
                }
            }
            (c, None) if c.is_ascii_alphanumeric() || c == '_' || c == '$' => word.push(c),
            (c, None) => {
                flush(&mut word, &mut out, after_dot);
                if c == '"' || c == '\'' {
                    quote = Some(c);
                }
                after_dot = c == '.';
                out.push(c);
            }
        }
    }
    flush(&mut word, &mut out, after_dot);
    out
}

/// True for a type that is not written as JSON: one only the browser can supply (an element or
/// several, an event, a signal), a catalog message (`Message<..>`, its own `%[..]` form) or a
/// format (`NumberFormat`, `MoneyFormat`, `DateFormat`: `%{..}`).
fn not_json(param: &Param) -> bool {
    match param {
        Param::Checked(path) => {
            path.starts_with("ts::Message")
                || path.ends_with("Format")
                || path.starts_with("el::")
                || path.starts_with("ts::Holds")
                || path.starts_with("ev::")
                || path.starts_with("ts::Array")
                || path.starts_with("ts::Map")
                || path == "ts::AbortSignal"
        }
        Param::Event(_) | Param::Group(_) | Param::Feed(_) | Param::Unreadable(_) => true,
        Param::Generated(_) | Param::Any => false,
    }
}

/// The type `import("../types/app").SortBy` names, from the declarations `corten::types` writes
/// (a path ending in `types/app`): `SortBy`.
fn generated_name(ty: &str) -> Option<String> {
    let rest = ty.trim().strip_prefix("import(")?;
    let (path, name) = rest.split_once(").")?;
    let path = path.trim().trim_matches(['"', '\'']);
    let path = path.strip_suffix(".js").or_else(|| path.strip_suffix(".d.ts")).unwrap_or(path);
    let name = name.trim();
    let plain = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    (generated_path(path) && plain).then(|| name.to_string())
}

/// The path of the declarations `corten::types` writes: `types/app` as whole segments.
fn generated_path(path: &str) -> bool {
    path == "types/app" || path.ends_with("/types/app")
}

/// True for a type that names a generated declaration inside another (`Tab[]`, `Tab | null`, an
/// optional `Tab=`, `Page<Tab>`): the call could not be checked against it, so it is refused
/// rather than taking any value.
fn mentions_generated(ty: &str) -> bool {
    ty.match_indices("import(").any(|(at, _)| {
        let rest = &ty[at + "import(".len()..];
        let path = rest.split(')').next().unwrap_or("").trim().trim_matches(['"', '\'']);
        let path = path.strip_suffix(".js").or_else(|| path.strip_suffix(".d.ts")).unwrap_or(path);
        generated_path(path)
    })
}

/// True for a type with a set of names anywhere in it: a string literal outside a generated
/// declaration's path (`import("../types/app")`) and a catalog message's argument names
/// (`Message<"count">`), so `"a" | "b"`, `Mode[] | null`, `[Mode, number]`, `{ m: Mode }` and
/// `Record<Mode, number>` are all refused, however they are written: such a value takes any text.
fn holds_names_union(ty: &str) -> bool {
    let mut rest = ty;
    let mut kept = String::with_capacity(ty.len());
    // Drop `import("..")` and `Message<..>` with their contents.
    loop {
        let next = [rest.find("import("), rest.find("Message<")].into_iter().flatten().min();
        let Some(at) = next else {
            kept.push_str(rest);
            break;
        };
        kept.push_str(&rest[..at]);
        let (open, close) = if rest[at..].starts_with("import(") { ('(', ')') } else { ('<', '>') };
        let body = &rest[at..];
        let start = body.find(open).map_or(body.len(), |i| i + 1);
        let mut depth = 1;
        let mut end = body.len();
        for (i, c) in body[start..].char_indices() {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    end = start + i + 1;
                    break;
                }
            }
        }
        rest = &body[end..];
    }
    kept.contains(['"', '\'', '`'])
}

/// True for a set of names, `"name" | "set"` (or one), also with `null` or `undefined` among
/// them or optional (`=`). A `|` inside a literal is part of it.
fn names_union(ty: &str) -> bool {
    let ty = ty.trim().trim_end_matches('=').trim();
    let ty = ty.strip_prefix('?').unwrap_or(ty).trim();
    let mut parts = Vec::new();
    let (mut start, mut quote, mut depth) = (0, None, 0i32);
    for (i, c) in ty.char_indices() {
        match (c, quote) {
            ('"' | '\'', None) => quote = Some(c),
            (c, Some(q)) if c == q => quote = None,
            (_, Some(_)) => {}
            ('(' | '<' | '[' | '{', None) => depth += 1,
            (')' | '>' | ']' | '}', None) => depth -= 1,
            ('|', None) if depth == 0 => {
                parts.push(&ty[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&ty[start..]);
    let mut names = 0;
    for part in parts.iter().map(|p| p.trim()) {
        let literal = part.len() >= 2
            && ((part.starts_with('"') && part.ends_with('"')) || (part.starts_with('\'') && part.ends_with('\'')));
        match part {
            "null" | "undefined" => {}
            _ if literal => names += 1,
            // A set written in parentheses, or an alias of one written out: `("a" | "b") | null`.
            _ if part.starts_with('(') && part.ends_with(')') && names_union(&part[1..part.len() - 1]) => names += 1,
            _ => return false,
        }
    }
    names > 0
}

/// `{ rows: HTMLTableRowElement[], all: HTMLInputElement }` as (name, type) pairs.
fn object_literal(ty: &str) -> Option<Vec<(String, String)>> {
    let inner = ty.trim().strip_prefix('{')?.strip_suffix('}')?;
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let bytes: Vec<char> = inner.chars().collect();
    let mut parts = Vec::new();
    for (j, c) in bytes.iter().enumerate() {
        match c {
            '{' | '<' | '(' | '[' => depth += 1,
            '}' | '>' | ')' | ']' => depth -= 1,
            ',' | ';' if depth == 0 => {
                parts.push(bytes[start..j].iter().collect::<String>());
                start = j + 1;
            }
            _ => {}
        }
    }
    parts.push(bytes[start..].iter().collect::<String>());
    for part in parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        let (name, ty) = part.split_once(':')?;
        out.push((name.trim().trim_end_matches('?').to_string(), ty.trim().to_string()));
    }
    Some(out)
}

/// The `{Type} name` after `tag` in a JSDoc block, once: `@typedef {object} Selection`.
fn tag_type(jsdoc: &str, tag: &str) -> Option<(String, String)> {
    tag_types(jsdoc, tag).into_iter().next()
}

/// `@param {Type} name` (or another `tag`'s) pairs of a JSDoc block, as (name, type).
fn tag_types(jsdoc: &str, tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = jsdoc;
    while let Some(at) = rest.find(tag) {
        rest = &rest[at + tag.len()..];
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
    // `Holds<Row, HTMLTableSectionElement>` (corten/views): an element that holds Row views.
    if let Some(inner) = ty.strip_prefix("Holds<").and_then(|t| t.strip_suffix('>')) {
        let (view, el) = inner.split_once(',').map_or((inner.trim(), "HTMLElement"), |(v, e)| (v.trim(), e.trim()));
        let plain = !view.is_empty() && view.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
        return match (plain, common(el, element)) {
            (true, Some(e)) => Param::Checked(format!("ts::Holds<::corten::script::el::{e}, {}u64>", view_id(view))),
            _ => Param::Any,
        };
    }
    // `FeedUrl<Level>` (corten): the URL of a feed of `Level` rows, a route with `rows = Level`.
    // The rows are the Rust type of that name in the component's module, written plain or as the
    // generated declaration it is (`import("../types/app").Level`).
    if let Some(inner) = ty.strip_prefix("FeedUrl<").and_then(|t| t.strip_suffix('>')) {
        let inner = inner.trim();
        let rows = match inner.strip_prefix("import(") {
            Some(rest) => rest.split_once(").").map_or("", |(_, name)| name.trim()),
            None => inner,
        };
        let plain = rows.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && rows.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        return if plain {
            Param::Feed(rows.to_string())
        } else {
            Param::Unreadable(format!(
                "FeedUrl<{inner}>: a feed's rows are one named type, FeedUrl<Level>, the Rust type Level in the component's module"
            ))
        };
    }
    // `import("../types/app").SortBy`: a declaration `corten::types` generated from the Rust type
    // of that name.
    if let Some(name) = generated_name(ty) {
        return Param::Generated(name);
    }
    if mentions_generated(ty) {
        return Param::Unreadable(format!(
            "{ty}: a generated declaration types a value alone, import(\"../types/app\").Name, so the call can be checked against its Rust type; for a list, an optional value or a generic, declare a Rust type for the whole value (derive ToJson and JsonSchema, add it with Types::add) and use its declaration"
        ));
    }
    // `Message<"count">` (corten): a message of the app's catalog with exactly these arguments.
    if let Some(inner) = ty.strip_prefix("Message<").and_then(|t| t.strip_suffix('>')) {
        return match message_names(inner) {
            Some(names) => Param::Checked(format!("ts::Message<{}u64>", message_id(&names))),
            None => Param::Any,
        };
    }
    if let Some(value) = ty.strip_prefix("Map<").and_then(|t| t.strip_suffix('>')).and_then(|t| t.trim().strip_prefix("string,"))
    {
        return match common(value.trim(), element) {
            Some(e) => Param::Checked(format!("ts::Map<::corten::script::el::{e}>")),
            None => Param::Any,
        };
    }
    let item = ty.strip_suffix("[]").or_else(|| ty.strip_prefix("Array<").and_then(|t| t.strip_suffix('>')));
    if let Some(item) = item {
        let item = item.trim().trim_start_matches('(').trim_end_matches(')').trim();
        // A list of strings, from a slice, an array or a `Vec` of them.
        if item == "string" {
            return Param::Checked("ts::Array<::corten::script::ts::String>".into());
        }
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
        // From `corten`: the page's formats, `t.js().number()`, `.money(USD)`, `.date(..)`,
        // `.percent(2)`.
        "NumberFormat" | "MoneyFormat" | "DateFormat" | "PercentFormat" => Param::Checked(format!("ts::{ty}")),
        "string" => Param::Checked("ts::String".into()),
        "number" => Param::Checked("ts::Number".into()),
        "boolean" => Param::Checked("ts::Boolean".into()),
        _ => Param::Any,
    }
}

/// What a `Message<"a" | "b">` parameter and a message with the arguments `a` and `b` are both
/// checked by: FNV-1a of the names, sorted and joined by commas.
pub(crate) fn message_id(names: &[&str]) -> u64 {
    let mut names = names.to_vec();
    names.sort_unstable();
    names.dedup();
    view_id(&names.join(","))
}

/// The argument names of `"a" | "b"`, or `None` when it is not a union of string literals.
fn message_names(union: &str) -> Option<Vec<&str>> {
    union
        .split('|')
        .map(|part| {
            let part = part.trim();
            let name = part.strip_prefix('"').and_then(|p| p.strip_suffix('"'))?;
            (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then_some(name)
        })
        .collect()
}

/// `corten::script::view_id`: FNV-1a of a view's name.
pub(crate) fn view_id(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, b| (hash ^ b as u64).wrapping_mul(0x0100_0000_01b3))
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
    if module.keeps {
        return err(
            path.span(),
            format!(
                "{}: keep() is the state of the component whose handler runs, and a script! module is shared by \
                 components: keep the state in each component's own script (#[component(js)]) or in the DOM",
                module.file
            ),
        );
    }
    let mut items = q(
        "#[doc = \" The module, for `App::components`.\"] pub const COMPONENT: ::corten::Component = ::corten::Component {
            name: __A0, script: ::core::option::Option::Some(__A1), style: ::core::option::Option::None, event: ::core::option::Option::None,
            view: ::core::option::Option::None,
        };",
        &[literal(&module.name), module.literal("super::")],
    );
    // Served at `scripts/{name}.js`, as a component's script is: two with one name fail to build.
    items.extend(crate::id::one_per_program("two_scripts_are", &module.name, true));
    Ok(module.module(vis, name.clone(), q("::core::option::Option::Some(&COMPONENT)", &[]), None, items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_that_imports_keep_keeps_state() {
        assert_eq!(keeps("import { keep } from \"corten\";"), Ok(true));
        assert_eq!(keeps("import { subscribe, keep as k } from 'corten';"), Ok(true));
        assert_eq!(keeps("import {\n  subscribe,\n  keep,\n} from \"corten\";"), Ok(true));
        assert_eq!(keeps("import { subscribe } from \"corten\";\n/** @import { Message } from \"corten\" */"), Ok(false));
        assert_eq!(keeps("import { keep } from \"./keep.js\";"), Ok(false));
        assert_eq!(keeps("import { Row } from \"corten/views\";"), Ok(false));
        assert_eq!(keeps("import { keep } from \"corten\"; // state per instance"), Ok(true));
        assert_eq!(keeps("import {keep} from\"corten\""), Ok(true));
        assert_eq!(keeps("import {\n  fromCents,\n  keep\n} from \"corten\";"), Ok(true));
        assert_eq!(keeps("/** @typedef {import(\"corten\").Message} M */\n// import(\"corten\")"), Ok(false));
        assert_eq!(keeps("const m = /** @type {import(\"corten\").Message<\"count\">} */ (x);"), Ok(false));
        assert_eq!(keeps("/* eslint-disable */ import { keep } from \"corten\";"), Ok(true));
        assert_eq!(keeps("/**\n * @param {import(\"corten\").Message} m\n */"), Ok(false));
        assert!(keeps("import * as corten from \"corten\";").unwrap_err().1.contains("by name"));
        assert!(keeps("const { keep } = await import(\"corten\");").unwrap_err().1.contains("by name"));
    }

    #[test]
    fn exports_and_jsdoc_types_become_handlers() {
        let source = r#"// @ts-check
import { keep } from "corten";

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
        let (handlers, groups) = parse(source).unwrap();
        assert!(groups.is_empty());
        assert_eq!(handlers.len(), 2);
        assert_eq!(handlers[0].name, "showAll");
        assert_eq!(handlers[0].doc, "Moves the rest into the table.");
        let kinds: Vec<String> = handlers
            .iter()
            .flat_map(|h| h.params.iter())
            .map(|(n, p)| match p {
                Param::Checked(t) => format!("{n}:{t}"),
                other => format!("{n}:{other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "rows:el::HTMLTableSectionElement",
                "rest:el::HTMLTemplateElement",
                "event:ev::MouseEvent",
                "cells:ts::Array<::corten::script::el::HTMLTableCellElement>",
                "stock:Any",
                "signal:ts::AbortSignal",
                "url:ts::String",
            ]
        );
    }

    #[test]
    fn a_feed_url_names_its_rows_by_the_rust_type_or_does_not_compile() {
        let (handlers, _) = parse(
            "/** @param {FeedUrl<Level>} a @param {FeedUrl<import(\"../types/app\").Level>} b */\nexport function f(a, b) {}",
        )
        .unwrap();
        assert_eq!(handlers[0].params[0].1, Param::Feed("Level".into()));
        assert_eq!(handlers[0].params[1].1, Param::Feed("Level".into()), "the generated declaration, imported where it is used");
        for ty in ["FeedUrl<Level[]>", "FeedUrl<Level | Stock>", "FeedUrl<>"] {
            let err = parse(&format!("/** @param {{{ty}}} a */\nexport function f(a) {{}}")).unwrap_err();
            assert!(err.1.contains("one named type"), "{ty}: {}", err.1);
        }
    }

    #[test]
    fn a_generated_type_is_the_rust_type_and_a_literal_union_does_not_compile() {
        let (handlers, _) = parse(
            "/** @typedef {import(\"../types/app\").SortBy} SortBy */\n/** @param {SortBy} a @param {import(\"../../types/app.js\").Step} b @param {import(\"./other\").Thing} c */\nexport function f(a, b, c) {}",
        )
        .unwrap();
        assert_eq!(handlers[0].params[0].1, Param::Generated("SortBy".into()), "through a typedef");
        assert_eq!(handlers[0].params[1].1, Param::Generated("Step".into()), "written in place");
        assert_eq!(handlers[0].params[2].1, Param::Any, "not the generated declarations");
        for (source, what) in [
            ("/** @param {\"name\" | \"price\"} by */\nexport function f(by) {}", "a union"),
            ("/** @param {'grid'} by */\nexport function f(by) {}", "one literal"),
            ("/** @typedef {\"a\" | \"b\"} Mode */\n/** @param {Mode} by */\nexport function f(by) {}", "a typedef of one"),
            ("/** @param {\"a\" | \"b\" | null} by */\nexport function f(by) {}", "with null"),
            ("/** @param {\"a\" | \"b\"=} by */\nexport function f(by) {}", "optional"),
            ("/** @param {\"a|b\"} by */\nexport function f(by) {}", "a literal with a bar"),
            ("/** @typedef {\"a\" | \"b\"} Mode */\n/** @typedef {Mode} Shown */\n/** @param {Shown} by */\nexport function f(by) {}", "a typedef of a typedef"),
            ("/** @typedef {{ el: HTMLElement, mode: \"a\" | \"b\" }} Sel */\n/** @param {Sel} s */\nexport function f(s) {}", "a group's field"),
        ] {
            let err = parse(source).unwrap_err();
            assert!(err.1.contains("is a Rust enum the markup passes"), "{what}: {}", err.1);
        }
        let (deep, _) = parse(
            "/** @typedef {import(\"../types/app\").Tab} Tab */\n/** @typedef {Tab} Shown */\n/** @param {Shown} a @param {import(\"./mytypes/app\").Tab} b */\nexport function f(a, b) {}",
        )
        .unwrap();
        assert_eq!(deep[0].params[0].1, Param::Generated("Tab".into()), "through a typedef of a typedef");
        assert_eq!(deep[0].params[1].1, Param::Any, "types/app as whole segments only");
        for ty in [
            "import(\"../types/app\").Tab | null",
            "import(\"../types/app\").Tab[]",
            "import(\"../types/app\").Tab=",
            "import(\"../types/app\").Page<Row>",
        ] {
            let err = parse(&format!("/** @param {{{ty}}} a */\nexport function f(a) {{}}")).unwrap_err();
            assert!(err.1.contains("types a value alone"), "{ty}: {}", err.1);
        }
        for alias in ["Tab[]", "Tab | null", "Tab=", "Array<Tab>", "?Tab"] {
            let source = format!(
                "/** @typedef {{import(\"../types/app\").Tab}} Tab */\n/** @param {{{alias}}} a */\nexport function f(a) {{}}"
            );
            assert!(parse(&source).unwrap_err().1.contains("types a value alone"), "{alias}");
        }
        let (feed, _) = parse(
            "/** @typedef {import(\"../types/app\").Level} Level */\n/** @param {FeedUrl<Level>} a */\nexport function f(a) {}",
        )
        .unwrap();
        assert_eq!(feed[0].params[0].1, Param::Feed("Level".into()), "a feed of an alias");
        // A script's own types of plain values are its own, whatever they hold.
        for typedef in [
            "/** @typedef {{ id: string, mode: \"a\" | \"b\" }} State */",
            "/** @typedef {object} Saved\n * @property {\"a\" | \"b\"} mode */",
            "/** @typedef {{ id: string, rows: import(\"./types/app\").Tab[] }} Payload */",
        ] {
            parse(&format!("{typedef}\n/** @param {{HTMLElement}} el */\nexport function f(el) {{}}")).unwrap();
        }
        let grouped = "/** @typedef {import(\"../types/app\").Tab} Tab */\n/** @typedef {{ el: HTMLElement, tabs: Tab[] }} Sel */\n/** @param {Sel} s */\nexport function f(s) {}";
        assert!(parse(grouped).unwrap_err().1.contains("types a value alone"), "a group's field");
        // A group no handler takes is the script's own, whatever it holds.
        let own = "/** @typedef {import(\"../types/app\").Tab} Tab */\n/** @typedef {{ el: HTMLElement, tabs: Tab[], tab: Tab | null }} Kept */\n/** @param {HTMLElement} el */\nexport function f(el) {}";
        parse(own).unwrap();
        // A typedef of a compound type is checked as the type it names.
        for (typedef, what) in [
            ("import(\"../types/app\").Tab | null", "types a value alone"),
            ("import(\"../types/app\").Tab[]", "types a value alone"),
            ("\"a\" | \"b\" | null", "is a Rust enum the markup passes"),
        ] {
            let source = format!("/** @typedef {{{typedef}}} Shown */\n/** @param {{Shown}} a */\nexport function f(a) {{}}");
            assert!(parse(&source).unwrap_err().1.contains(what), "{typedef}");
        }
        // A list or a generic of a set of names is refused as the set is.
        for ty in ["(\"a\" | \"b\")[]", "Array<\"a\" | \"b\">", "Readonly<Mode>", "Record<Mode, number>"] {
            let source = format!("/** @typedef {{\"a\" | \"b\"}} Mode */\n/** @param {{{ty}}} a */\nexport function f(a) {{}}");
            assert!(parse(&source).unwrap_err().1.contains("is a Rust enum the markup passes"), "{ty}");
        }
        // However a set of names is wrapped.
        for ty in [
            "Mode[]=",
            "Mode[] | null",
            "Array<Mode> | null",
            "(\"a\"|\"b\")[]=",
            "[Mode, number]",
            "{ m: Mode }",
            "Readonly<Modes>",
        ] {
            let source = format!(
                "/** @typedef {{\"a\" | \"b\"}} Mode */\n/** @typedef {{Mode[]}} Modes */\n/** @param {{{ty}}} a */\nexport function f(a) {{}}"
            );
            assert!(parse(&source).unwrap_err().1.contains("is a Rust enum the markup passes"), "{ty}");
        }
        // A generated declaration through two typedefs, or in a typedef of plain values, is refused
        // as it is written in place.
        for (typedefs, ty) in [
            ("/** @typedef {Tab[]} Tabs */", "Tabs | null"),
            ("/** @typedef {Tab[]} Tabs */", "Readonly<Tabs>"),
            ("/** @typedef {Tab[]} Tabs */\n/** @typedef {Tabs | null} MaybeTabs */", "MaybeTabs"),
            ("/** @typedef {{ id: string, tab: Tab }} Picked */", "Picked"),
        ] {
            let source = format!(
                "/** @typedef {{import(\"../types/app\").Tab}} Tab */\n{typedefs}\n/** @param {{{ty}}} a */\nexport function f(a) {{}}"
            );
            let err = parse(&source).unwrap_err().1;
            assert!(err.contains("types a value alone") || err.contains("shape the call is not checked against"), "{ty}: {err}");
        }
        // A generic typedef of the script's own (`@template`) is a definition, not a name for
        // another type, so `Holds` declared in the script keeps its check.
        let generic = "/**\n * @template Row, El\n * @typedef {El & { readonly \"corten.holds\": Row }} Holds\n */\n/** @param {Holds<Row, HTMLTableSectionElement>} body */\nexport function f(body) {}";
        let (held, _) = parse(generic).unwrap();
        assert!(matches!(&held[0].params[0].1, Param::Checked(p) if p.starts_with("ts::Holds")), "{:?}", held[0].params[0].1);
        let mode = "/** @typedef {\"a\" | \"b\"} Mode */\n/** @param {Mode | null} m */\nexport function f(m) {}";
        assert!(parse(mode).unwrap_err().1.contains("is a Rust enum the markup passes"), "an alias of a set, or null");
        let (fine, _) = parse("/** @typedef {\"a\" | \"b\"} Mode */\n/** @param {string} s */\nexport function f(s) {}").unwrap();
        assert_eq!(fine[0].params[0].1, Param::Checked("ts::String".into()), "a typedef no handler takes is the script's");
        // A script's own state with an element in it is a group only when a handler takes it.
        let kept = "/** @typedef {{ from: \"a\" | \"b\", back: HTMLButtonElement }} Moving */\n/** @param {HTMLElement} el */\nexport function f(el) {}";
        parse(kept).unwrap();
        let taken = format!("{kept}\n/** @param {{Moving}} m */\nexport function g(m) {{}}");
        assert!(parse(&taken).unwrap_err().1.contains("Moving: from"), "a group a handler takes");
    }

    #[test]
    fn a_message_parameter_is_checked_by_its_argument_names() {
        let (handlers, _) = parse(
            "/** @param {Message<\"total\" | \"count\">} label @param {Message<count>} odd */\nexport function f(label, odd) {}",
        )
        .unwrap();
        assert_eq!(handlers[0].params[0].1, Param::Checked(format!("ts::Message<{}u64>", message_id(&["count", "total"]))));
        assert_eq!(message_id(&["total", "count"]), message_id(&["count", "total"]), "the order does not matter");
        assert_eq!(handlers[0].params[1].1, Param::Any, "not a union of names");
        let (camel, _) = parse("/** @param {Message<\"updatedAt\">} label */\nexport function f(label) {}").unwrap();
        assert_eq!(
            camel[0].params[0].1,
            Param::Checked(format!("ts::Message<{}u64>", message_id(&["updatedAt"]))),
            "the catalog's name"
        );
    }

    #[test]
    fn holds_names_the_view_its_rows_are() {
        let source =
            "/** @param {Holds<Row, HTMLTableSectionElement>} body @param {Holds<Row>} any @param {Holds<Row, Thing>} odd */
export function f(body, any, odd) {}";
        let (handlers, _) = parse(source).unwrap();
        let row = view_id("Row");
        // FNV-1a's published values, as corten::script::view_id computes them.
        assert_eq!((view_id(""), view_id("a")), (0xcbf2_9ce4_8422_2325, 0xaf63_dc4c_8601_ec8c));
        assert_eq!(
            handlers[0].params,
            [
                (
                    "body".to_string(),
                    Param::Checked(format!("ts::Holds<::corten::script::el::HTMLTableSectionElement, {row}u64>"))
                ),
                ("any".to_string(), Param::Checked(format!("ts::Holds<::corten::script::el::HTMLElement, {row}u64>"))),
                ("odd".to_string(), Param::Any),
            ]
        );
    }

    #[test]
    fn untyped_parameters_and_other_exports_are_errors() {
        assert!(parse("export function f(a) {}").unwrap_err().1.contains("give a a type"));
        assert!(parse("export const x = 1;").unwrap_err().1.contains("exports only functions"));
        assert!(parse("/** @param {Element} a */\nexport function f({ a }) {}").unwrap_err().1.contains("plain names"));
        let (union, _) = parse("/** @param {HTMLAnchorElement | HTMLButtonElement} tab */\nexport function f(tab) {}").unwrap();
        assert!(matches!(&union[0].params[0].1, Param::Checked(t) if t == "el::HTMLElement"));
    }

    #[test]
    fn typedefs_of_elements_are_groups_and_imported_events_are_rust_types() {
        let source = r#"import { keep } from "corten";
import { RowToggled, Saved as Done } from "corten/events";
/** @typedef {import("corten/events").Cleared} Cleared */

/**
 * The grid's selection controls.
 * @typedef {object} Selection
 * @property {HTMLTableRowElement[]} rows
 * @property {HTMLInputElement} all
 */

/** @typedef {{ count: HTMLOutputElement; label: string }} Counter */
/** @typedef {{ id: string, qty: number }} Level */
/** @typedef {{ entity: string, selected: Message<"count"> }} Count */

/** @param {RowToggled} event @param {Selection} sel @param {Counter} c @param {Level} level @param {Done} done @param {Cleared} cleared @param {Count} n */
export function toggled(event, sel, c, level, done, cleared, n) {}
"#;
        let (handlers, groups) = parse(source).unwrap();
        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            ["Selection", "Counter", "Count"],
            "a typedef of plain values stays JSON; one with a message is a group"
        );
        assert_eq!(groups[0].doc, "The grid's selection controls.");
        assert_eq!(
            groups[0].fields[0],
            ("rows".into(), Param::Checked("ts::Array<::corten::script::el::HTMLTableRowElement>".into()))
        );
        assert_eq!(groups[1].fields[1], ("label".into(), Param::Checked("ts::String".into())));
        let params: Vec<&Param> = handlers[0].params.iter().map(|(_, p)| p).collect();
        assert_eq!(
            params,
            [
                &Param::Event("RowToggled".into()),
                &Param::Group("Selection".into()),
                &Param::Group("Counter".into()),
                &Param::Any,
                &Param::Event("Saved".into()),
                &Param::Event("Cleared".into()),
                &Param::Group("Count".into()),
            ]
        );
        // Served with the script: the classes it imports when it runs, not a JSDoc type's.
        let (_, _, imports, _) = parse_all(source).unwrap();
        assert_eq!(imports, ["RowToggled", "Saved"]);
        // Comments between the names, and an import in a comment, are not imports.
        let commented = "/* import { Old } from \"corten/events\"; */\nimport {\n  Added, // the row's\n  /* kept */ Removed,\n} from \"corten/events\";\nexport function f() {}\n";
        assert_eq!(parse_all(commented).unwrap().2, ["Added", "Removed"]);
    }

    #[test]
    fn a_script_of_the_same_directory_is_a_sibling() {
        let source = "import { loadStripe } from \"./stripe.js\";\nimport \"./theme.js\";\nimport {\n  a,\n} from './stripe.js';\nimport { keep } from \"corten\";\nimport { b } from \"../app.js\";\nimport { c } from \"./sub/x.js\";\nimport d, { e } from \"./default.js\";\nimport * as all from \"./all.js\";\nexport function f() {}\n";
        assert_eq!(parse_all(source).unwrap().3, ["stripe", "theme", "default", "all"]);
        // Without semicolons, each import ends at its closing quote: nothing after it is lost.
        let bare = "import \"./theme.js\"\nimport { a } from \"./stripe.js\"\nexport function f() {}\nexport function g() {}\n";
        let (handlers, _, _, siblings) = parse_all(bare).unwrap();
        assert_eq!(siblings, ["theme", "stripe"]);
        assert_eq!(handlers.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), ["f", "g"]);
    }
}
