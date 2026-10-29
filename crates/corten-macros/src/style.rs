//! Component stylesheets. `#[component(css)]` reads `{name}.css` next to the component, scopes
//! every class in it to the component (`.title` becomes `.offer-list__title`), and gives the
//! markup one constant per class, so `class={Title}` is checked and printed as static markup.
//!
//! The scoping runs here, at compile time: the server only concatenates finished stylesheets.

use std::path::{Path, PathBuf};

use proc_macro::{Delimiter, Group, Ident, Literal, Span, TokenStream, TokenTree};

use crate::tokens::{err, is_ident, is_punct, literal, q, string_value, Error};

/// A component's stylesheet, scoped.
#[derive(Debug)]
pub struct Sheet {
    /// The CSS with every class prefixed, comments and extra whitespace removed.
    pub css: String,
    /// The classes, as written in the file, first use first.
    pub classes: Vec<String>,
}

/// Prefixes every class in `src` with `{prefix}__`. A rule outside any at-rule (or in `@media`,
/// `@supports`, `@container`, `@layer`) must name one of the file's classes, so a component's
/// styles cannot reach elements it does not render.
pub fn scope(src: &str, prefix: &str) -> Result<Sheet, String> {
    let mut s =
        Scoper { text: src, bytes: src.as_bytes(), i: 0, out: String::with_capacity(src.len()), classes: Vec::new(), prefix };
    s.block(Ctx::Top, true)?;
    Ok(Sheet { css: s.out, classes: s.classes })
}

#[derive(Clone, Copy, PartialEq)]
enum Ctx {
    /// Rules, which must be scoped.
    Top,
    /// Inside a style rule: declarations and nested rules.
    Style,
    /// `@keyframes`, `@font-face` and the like: copied as written.
    Raw,
}

struct Scoper<'a> {
    text: &'a str,
    bytes: &'a [u8],
    i: usize,
    out: String,
    classes: Vec<String>,
    prefix: &'a str,
}

impl Scoper<'_> {
    fn error(&self, at: usize, message: impl std::fmt::Display) -> String {
        let line = self.text[..at.min(self.text.len())].matches('\n').count() + 1;
        format!("line {line}: {message}")
    }

    fn close(&mut self) {
        if self.out.ends_with(';') {
            self.out.pop();
        }
        self.out.push('}');
    }

    fn block(&mut self, ctx: Ctx, top: bool) -> Result<(), String> {
        loop {
            self.skip_space();
            match self.bytes.get(self.i) {
                None if top => return Ok(()),
                None => return Err(self.error(self.i, "a block is never closed: add }")),
                Some(b'}') if top => return Err(self.error(self.i, "unexpected }")),
                Some(b'}') => {
                    self.i += 1;
                    return Ok(());
                }
                _ => {}
            }
            let start = self.i;
            let end = self.chunk()?;
            let text = squash(&self.text[start..self.i]);
            if end == Some(b'{') {
                self.i += 1;
                let inner = if let Some(rule) = text.strip_prefix('@') {
                    let name = rule.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).next().unwrap_or("");
                    match (ctx, name) {
                        (Ctx::Raw, _) => Ctx::Raw,
                        (_, "media" | "supports" | "container" | "layer" | "scope" | "starting-style") => ctx,
                        (
                            _,
                            "keyframes"
                            | "font-face"
                            | "property"
                            | "page"
                            | "counter-style"
                            | "font-palette-values"
                            | "font-feature-values",
                        ) => Ctx::Raw,
                        _ => return Err(self.error(start, format!("@{name} is not supported in a component's stylesheet"))),
                    }
                } else {
                    Ctx::Style
                };
                if text.starts_with('@') || ctx == Ctx::Raw {
                    self.out.push_str(&text);
                } else {
                    let selector = self.selector(&text, ctx == Ctx::Top, start)?;
                    self.out.push_str(&selector);
                }
                self.out.push('{');
                self.block(inner, false)?;
                self.close();
                continue;
            }
            if end == Some(b';') {
                self.i += 1;
            }
            if text.is_empty() {
                continue;
            }
            if let Some(rule) = text.strip_prefix('@') {
                let name = rule.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).next().unwrap_or("");
                match name {
                    "import" => {
                        return Err(self.error(
                            start,
                            "@import loads a stylesheet the component does not own; put it in the app's stylesheet",
                        ))
                    }
                    "charset" => {}
                    "layer" => {
                        self.out.push_str(&text);
                        self.out.push(';');
                    }
                    _ => return Err(self.error(start, format!("@{name} is not supported in a component's stylesheet"))),
                }
                continue;
            }
            if ctx == Ctx::Top {
                return Err(self.error(start, format!("`{text}` is outside any rule")));
            }
            let Some((name, value)) = split_declaration(&text) else {
                return Err(self.error(start, format!("expected `property: value`, found `{text}`")));
            };
            self.out.push_str(name);
            self.out.push(':');
            self.out.push_str(value);
            self.out.push(';');
        }
    }

    fn skip_space(&mut self) {
        loop {
            while self.bytes.get(self.i).is_some_and(|b| b.is_ascii_whitespace()) {
                self.i += 1;
            }
            if self.bytes[self.i.min(self.bytes.len())..].starts_with(b"/*") {
                match self.text[self.i + 2..].find("*/") {
                    Some(end) => self.i += end + 4,
                    None => self.i = self.bytes.len(),
                }
                continue;
            }
            return;
        }
    }

    /// Moves to the next `{`, `;` or `}` outside strings, comments and brackets.
    fn chunk(&mut self) -> Result<Option<u8>, String> {
        let mut depth = 0usize;
        while let Some(&b) = self.bytes.get(self.i) {
            match b {
                b'"' | b'\'' => {
                    let start = self.i;
                    self.i += 1;
                    loop {
                        match self.bytes.get(self.i) {
                            None | Some(b'\n') => return Err(self.error(start, "a string is never closed")),
                            Some(b'\\') => self.i += 2,
                            Some(&c) if c == b => break,
                            Some(_) => self.i += 1,
                        }
                    }
                }
                b'/' if self.bytes.get(self.i + 1) == Some(&b'*') => match self.text[self.i + 2..].find("*/") {
                    Some(end) => self.i += end + 3,
                    None => self.i = self.bytes.len() - 1,
                },
                b'\\' => self.i += 1,
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth = depth.saturating_sub(1),
                b'{' | b';' | b'}' if depth == 0 => return Ok(Some(b)),
                _ => {}
            }
            self.i += 1;
        }
        Ok(None)
    }

    /// The selector with its classes prefixed. With `check`, every selector in the list must
    /// name a class of the file.
    fn selector(&mut self, text: &str, check: bool, at: usize) -> Result<String, String> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len() + 16);
        let (mut i, mut brackets, mut parens) = (0, 0usize, 0usize);
        let mut quote = None;
        let mut part_start = 0;
        let mut part_scoped = false;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = quote {
                out.push(c);
                if c == '\\' && i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                    i += 1;
                } else if c == q {
                    quote = None;
                }
                i += 1;
                continue;
            }
            match c {
                '"' | '\'' => quote = Some(c),
                '[' => brackets += 1,
                ']' => brackets = brackets.saturating_sub(1),
                '(' => parens += 1,
                ')' => parens = parens.saturating_sub(1),
                ',' if brackets == 0 && parens == 0 => {
                    if check {
                        self.owned(&chars[part_start..i], part_scoped, at)?;
                    }
                    part_start = i + 1;
                    part_scoped = false;
                }
                '.' if brackets == 0 && ident_start(&chars[i + 1..]) => {
                    let mut end = i + 1;
                    while end < chars.len() && (chars[end].is_ascii_alphanumeric() || matches!(chars[end], '-' | '_')) {
                        end += 1;
                    }
                    if chars.get(end) == Some(&'\\') {
                        return Err(
                            self.error(at, "escaped characters in a class name are not supported in a component's stylesheet")
                        );
                    }
                    let class: String = chars[i + 1..end].iter().collect();
                    if class.contains("__") {
                        return Err(
                            self.error(at, format!(".{class}: __ separates the component's name from the class; use - or _"))
                        );
                    }
                    out.push('.');
                    out.push_str(self.prefix);
                    out.push_str("__");
                    out.push_str(&class);
                    if !self.classes.contains(&class) {
                        self.classes.push(class);
                    }
                    part_scoped = true;
                    i = end;
                    continue;
                }
                _ => {}
            }
            out.push(c);
            i += 1;
        }
        if check {
            self.owned(&chars[part_start..], part_scoped, at)?;
        }
        Ok(out)
    }

    /// An error unless the selector names a class of the file.
    fn owned(&self, part: &[char], scoped: bool, at: usize) -> Result<(), String> {
        let part: String = part.iter().collect();
        let part = part.trim();
        if scoped {
            return Ok(());
        }
        Err(self.error(
            at,
            format!("`{part}` styles elements this component does not own: start the rule with one of the stylesheet's classes, like `.list {part}`"),
        ))
    }
}

fn ident_start(rest: &[char]) -> bool {
    match rest {
        [c, ..] if c.is_ascii_alphabetic() || *c == '_' => true,
        ['-', c, ..] => c.is_ascii_alphabetic() || *c == '_',
        _ => false,
    }
}

/// Comments removed and whitespace collapsed, outside strings.
fn squash(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut space = false;
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';
                for c in chars.by_ref() {
                    if last == '*' && c == '/' {
                        break;
                    }
                    last = c;
                }
                space = true;
            }
            '"' | '\'' => {
                if space && !out.is_empty() {
                    out.push(' ');
                }
                space = false;
                out.push(c);
                while let Some(s) = chars.next() {
                    out.push(s);
                    if s == '\\' {
                        if let Some(next) = chars.next() {
                            out.push(next);
                        }
                    } else if s == c {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => space = true,
            c => {
                if space && !out.is_empty() {
                    out.push(' ');
                }
                space = false;
                out.push(c);
            }
        }
    }
    out
}

fn split_declaration(text: &str) -> Option<(&str, &str)> {
    let (name, value) = text.split_once(':')?;
    let name = name.trim();
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).then(|| (name, value.trim()))
}

/// The constant for a class: `alert-message` is `AlertMessage`.
pub fn class_ident(class: &str) -> Result<String, String> {
    let mut out = String::new();
    for word in class.split(['-', '_']).filter(|w| !w.is_empty()) {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) || out == "Self" {
        return Err(format!(".{class} has no Rust name; start it with a letter"));
    }
    Ok(out)
}

/// The stylesheet or script `file` next to the source file of `span`. When the compiler gives no source
/// path (some editors' macro servers), the one file with that name under the crate's `src`.
pub fn locate(file: &str, span: Span) -> Result<PathBuf, Error> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
    if let Some(source) = span.local_file() {
        // A relative path is from the compiler's directory (the workspace root under cargo).
        let source = match (source.is_relative(), std::env::current_dir()) {
            (true, Ok(cwd)) if cwd.join(&source).is_file() => cwd.join(source),
            (true, _) => root.join(source),
            (false, _) => source,
        };
        if let Some(dir) = source.parent() {
            let path = dir.join(file);
            if path.is_file() {
                return Ok(path);
            }
            return err(span, format!("{file} belongs next to this source file, and {} does not exist", path.display()));
        }
    }
    let mut found = Vec::new();
    find(&root.join("src"), file, &mut found);
    match found.len() {
        1 => Ok(found.pop().expect("one")),
        0 => err(span, format!("{file} belongs next to this source file, and there is none")),
        _ => err(span, format!("several files are named {file}; the compiler gave no source path to choose one")),
    }
}

fn find(dir: &Path, file: &str, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find(&path, file, found);
        } else if path.file_name().is_some_and(|n| n == file) {
            found.push(path);
        }
    }
}

/// A component's stylesheet, read and scoped, and what the macro generates from it.
pub struct Css {
    /// `offer-list`.
    pub prefix: String,
    /// `offer_list.css`.
    pub file: String,
    pub path: PathBuf,
    pub sheet: Sheet,
    /// The constant for each class, in the order of `sheet.classes`.
    pub idents: Vec<String>,
}

impl Css {
    pub fn read(owner: &Ident, span: Span) -> Result<Css, Error> {
        let name = owner.to_string();
        let prefix = crate::names::kebab(&name);
        let file = format!("{}.css", crate::names::snake(&name));
        let path = locate(&file, span)?;
        let text = std::fs::read_to_string(&path).map_err(|e| (span, format!("cannot read {}: {e}", path.display())))?;
        let sheet = scope(&text, &prefix).map_err(|e| (span, format!("{file}, {e}")))?;
        let mut idents: Vec<String> = Vec::with_capacity(sheet.classes.len());
        for class in &sheet.classes {
            let ident = class_ident(class).map_err(|e| (span, format!("{file}: {e}")))?;
            if let Some(i) = idents.iter().position(|other| *other == ident) {
                return err(span, format!("{file}: .{} and .{class} are both {ident} in Rust; rename one", sheet.classes[i]));
            }
            idents.push(ident);
        }
        Ok(Css { prefix, file, path, sheet, idents })
    }

    fn scoped(&self, class: &str) -> String {
        format!("{}__{class}", self.prefix)
    }

    fn class_for(&self, ident: &str) -> Option<&str> {
        self.idents.iter().position(|i| i == ident).map(|i| self.sheet.classes[i].as_str())
    }

    fn unknown(&self, ident: &Ident) -> Error {
        let list = self.idents.join(", ");
        (ident.span(), format!("{} has no class {ident}; its classes are {list}", self.file))
    }

    /// The module of class constants, named after the stylesheet's owner. Its constants are
    /// not `allow(dead_code)`: a class no component of the file uses is a warning.
    pub fn items(&self, owner: &Ident) -> TokenStream {
        let mut consts = q("const _: &str = ::core::include_str!(__A0);", &[literal(&self.path.display().to_string())]);
        for (class, ident) in self.sheet.classes.iter().zip(&self.idents) {
            // With the owner's span (not the macro's), rustc reports an unused class as dead code.
            consts.extend(respan(
                q(
                    "pub const __A0: ::corten::style::Class = ::corten::style::Class::__new(__A1);",
                    &[TokenTree::Ident(Ident::new(ident, owner.span())).into(), literal(&self.scoped(class))],
                ),
                owner.span(),
            ));
        }
        q(
            "#[doc(hidden)] #[allow(non_snake_case, non_upper_case_globals)] mod __A0 { __A1 }",
            &[TokenTree::Ident(module(owner)).into(), consts],
        )
    }

    /// Statements at the start of `render`: the owner's constants in scope, and a use of each
    /// class the markup printed as static text (so they count as used).
    pub fn scope(&self, owner: &Ident, used: &[Ident]) -> TokenStream {
        let mut out = q("#[allow(unused_imports)] use __A0::*;", &[TokenTree::Ident(module(owner)).into()]);
        if !used.is_empty() {
            let mut list = TokenStream::new();
            for ident in used {
                list.extend([TokenTree::Ident(ident.clone())]);
                list.extend(q(",", &[]));
            }
            out.extend(q("let _ = (__A0);", &[list]));
        }
        out
    }

    /// `::corten::style::Style { .. }`.
    pub fn style(&self) -> TokenStream {
        let mut classes = TokenStream::new();
        for class in &self.sheet.classes {
            classes.extend(literal(class));
            classes.extend(q(",", &[]));
        }
        q(
            "::corten::style::Style { name: __A0, file: __A1, css: __A2, classes: &[__A3] }",
            &[literal(&self.prefix), literal(&self.file), literal(&self.sheet.css), classes],
        )
    }

    /// The component's markup with `class={Title}` and `class={[Title, Active]}` as static
    /// markup, after checking every class it names (and that no class of the file is written as
    /// text). The names printed as text go in `used`.
    pub fn rewrite(&self, tokens: TokenStream, markup: bool, used: &mut Vec<Ident>) -> Result<TokenStream, Error> {
        let tokens: Vec<TokenTree> = tokens.into_iter().collect();
        let mut out: Vec<TokenTree> = Vec::with_capacity(tokens.len());
        let mut tag = String::new();
        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                TokenTree::Ident(id) if !markup && id.to_string() == "html" && is_punct(tokens.get(i + 1), '!') => {
                    if let Some(TokenTree::Group(g)) = tokens.get(i + 2) {
                        out.extend(tokens[i..i + 2].iter().cloned());
                        out.push(regroup(g, self.rewrite(g.stream(), true, used)?));
                        i += 3;
                        continue;
                    }
                }
                TokenTree::Group(g) => {
                    out.push(regroup(g, self.rewrite(g.stream(), markup, used)?));
                    i += 1;
                    continue;
                }
                TokenTree::Punct(p) if markup && p.as_char() == '<' => {
                    tag = match tokens.get(i + 1) {
                        Some(TokenTree::Ident(name)) if !is_punct(tokens.get(i + 2), ':') => name.to_string(),
                        _ => String::new(),
                    };
                }
                TokenTree::Ident(id)
                    if markup
                        && id.to_string() == "class"
                        && is_punct(tokens.get(i + 1), '=')
                        && !is_punct(tokens.get(i + 2), '=')
                        && !(i > 0 && (is_ident(tokens.get(i - 1), "let") || is_ident(tokens.get(i - 1), "mut")))
                        && tag.starts_with(|c: char| c.is_ascii_lowercase()) =>
                {
                    out.extend(tokens[i..i + 2].iter().cloned());
                    if let Some(value) = tokens.get(i + 2) {
                        out.push(self.value(value, used)?);
                    }
                    i += 3;
                    continue;
                }
                _ => {}
            }
            out.push(tokens[i].clone());
            i += 1;
        }
        Ok(out.into_iter().collect())
    }

    fn value(&self, value: &TokenTree, used: &mut Vec<Ident>) -> Result<TokenTree, Error> {
        let mut names = |idents: &[&Ident]| -> Result<TokenTree, Error> {
            let mut text = String::new();
            for ident in idents {
                let class = self.class_for(&ident.to_string()).ok_or_else(|| self.unknown(ident))?;
                used.push((*ident).clone());
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(&self.scoped(class));
            }
            let mut lit = Literal::string(&text);
            lit.set_span(idents[0].span());
            Ok(TokenTree::Literal(lit))
        };
        let camel =
            |t: &TokenTree| matches!(t, TokenTree::Ident(id) if id.to_string().starts_with(|c: char| c.is_ascii_uppercase()));
        match value {
            TokenTree::Literal(lit) => {
                self.not_text(lit)?;
                Ok(value.clone())
            }
            TokenTree::Ident(id) if camel(value) => names(&[id]),
            TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => {
                let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                match inner.as_slice() {
                    [TokenTree::Ident(id)] if camel(&inner[0]) => names(&[id]),
                    [TokenTree::Literal(lit)] => {
                        self.not_text(lit)?;
                        Ok(value.clone())
                    }
                    [TokenTree::Group(list)] if list.delimiter() == Delimiter::Bracket => {
                        let items: Vec<TokenTree> = list.stream().into_iter().collect();
                        let idents: Vec<&Ident> = items
                            .iter()
                            .enumerate()
                            .filter_map(|(n, t)| match t {
                                TokenTree::Ident(id) if n % 2 == 0 => Some(id),
                                _ => None,
                            })
                            .collect();
                        let plain = !items.is_empty()
                            && items.iter().enumerate().all(|(n, t)| if n % 2 == 0 { camel(t) } else { is_punct(Some(t), ',') });
                        if plain {
                            names(&idents)
                        } else {
                            Ok(regroup(g, self.rewrite(g.stream(), false, used)?))
                        }
                    }
                    _ => Ok(regroup(g, self.rewrite(g.stream(), false, used)?)),
                }
            }
            other => Ok(other.clone()),
        }
    }

    /// An error when text names a class of the file, which the scoped rule would not match.
    fn not_text(&self, lit: &Literal) -> Result<(), Error> {
        let Some(text) = string_value(lit) else { return Ok(()) };
        for word in text.split_ascii_whitespace() {
            if let Some(i) = self.sheet.classes.iter().position(|c| c == word) {
                return err(
                    lit.span(),
                    format!(
                        "\"{word}\" is a class of {}: write class={{{}}}, which is scoped to this component",
                        self.file, self.idents[i]
                    ),
                );
            }
        }
        Ok(())
    }
}

fn respan(tokens: TokenStream, span: Span) -> TokenStream {
    tokens
        .into_iter()
        .map(|mut token| {
            if let TokenTree::Group(g) = &token {
                let mut copy = Group::new(g.delimiter(), respan(g.stream(), span));
                copy.set_span(span);
                return TokenTree::Group(copy);
            }
            token.set_span(span);
            token
        })
        .collect()
}

/// `__corten_css_OfferList`: the class constants of `OfferList`'s stylesheet.
fn module(owner: &Ident) -> Ident {
    Ident::new(&format!("__corten_css_{owner}"), owner.span())
}

fn regroup(g: &Group, stream: TokenStream) -> TokenTree {
    let mut copy = Group::new(g.delimiter(), stream);
    copy.set_span(g.span());
    TokenTree::Group(copy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_are_prefixed_and_rules_stay_inside_the_component() {
        let css = "/* list */\n.list { gap: 8px; }\n.list > .row:hover, .row.is-on { color: var(--ink) }\n@media (min-width: 40rem) { .list { gap: 1.5rem } }\n.title { & span { font-weight: 700 } h2 & { margin: 0 } }\n@keyframes spin { from { rotate: 0 } to { rotate: 1turn } }\n.img { background: url(\"a.b.png\") }\n.x[data-tone=\".y\"] { opacity: .5 }";
        let sheet = scope(css, "offer-list").unwrap();
        assert_eq!(
            sheet.css,
            ".offer-list__list{gap:8px}.offer-list__list > .offer-list__row:hover, .offer-list__row.offer-list__is-on{color:var(--ink)}@media (min-width: 40rem){.offer-list__list{gap:1.5rem}}.offer-list__title{& span{font-weight:700}h2 &{margin:0}}@keyframes spin{from{rotate:0}to{rotate:1turn}}.offer-list__img{background:url(\"a.b.png\")}.offer-list__x[data-tone=\".y\"]{opacity:.5}"
        );
        assert_eq!(sheet.classes, ["list", "row", "is-on", "title", "img", "x"]);
        assert!(scope("h2 { margin: 0 }", "x").unwrap_err().contains("`h2` styles elements this component does not own"));
        assert!(scope(".a, h2 { margin: 0 }", "x").unwrap_err().contains("line 1"));
        assert!(scope("@media print { body { margin: 0 } }", "x").is_err(), "at-rules do not unscope");
        let print = scope(
            "@page slip-4x6 { size: 4in 6in; margin: 0 }\n.slip { page: slip-4x6 }\n@media print { .bar { display: none } }",
            "x",
        );
        assert_eq!(
            print.map(|s| s.css),
            Ok("@page slip-4x6{size:4in 6in;margin:0}.x__slip{page:slip-4x6}@media print{.x__bar{display:none}}".to_string()),
            "a named page is the app's; print rules are scoped"
        );
        assert!(scope("\n\n@import \"x.css\";", "x").unwrap_err().starts_with("line 3: @import"));
        assert!(scope(".a { color: red", "x").unwrap_err().contains("never closed"));
        assert!(scope(".a__b { color: red }", "x").is_err());
    }

    #[test]
    fn class_names_become_rust_names() {
        assert_eq!(class_ident("alert-message").unwrap(), "AlertMessage");
        assert_eq!(class_ident("img").unwrap(), "Img");
        assert_eq!(class_ident("is_on").unwrap(), "IsOn");
        assert!(class_ident("self").is_err());
    }
}
