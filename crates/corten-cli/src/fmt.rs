//! `corten fmt`: lays out the markup inside `html! { .. }` (rustfmt leaves macro bodies alone).
//!
//! Elements that fit on a line (rustfmt's `max_width`) stay on one; otherwise each child gets its
//! own line, except text and expressions with the inline elements next to them (`<a>`, `<strong>`,
//! `<input>` ..), which fill lines like a sentence. A start tag that does not fit puts each
//! attribute on its own line.
//!
//! It never joins lines the author split: a body written with a line break after its start tag or
//! `{` stays expanded, a start tag written with a line break before its first attribute keeps one
//! attribute per line, and a line break between two inline siblings stays. A space between two
//! inline siblings on one line stays, and so do blank lines between nodes (at most one). Rust inside
//! braces is kept as written (continuation lines move with it).
//!
//! Only whitespace changes: the tokens of every rewritten body are compared with the original's,
//! and a body that would differ (or that this parser does not understand) is left as it is, with
//! a warning.

use std::path::{Path, PathBuf};

/// Elements laid out inline: they fill lines with the text around them.
const INLINE: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "br", "button", "cite", "code", "data", "dfn", "em", "i", "img", "input", "kbd", "label",
    "mark", "meter", "output", "progress", "q", "s", "samp", "select", "small", "span", "strong", "sub", "sup", "textarea",
    "time", "u", "var", "wbr",
];
const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];

/// `corten fmt [--check] [path ..]`.
pub fn run(args: &[String]) -> Result<(), String> {
    let check = args.iter().any(|a| a == "--check");
    let mut paths: Vec<PathBuf> = args.iter().filter(|a| !a.starts_with("--")).map(PathBuf::from).collect();
    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }
    let mut files = Vec::new();
    for path in &paths {
        collect(path, &mut files).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let mut unformatted = Vec::new();
    for file in &files {
        let source = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let width = max_width(file);
        let (formatted, warnings) = format_source(&source, width);
        for warning in warnings {
            eprintln!("corten fmt: {}:{warning}", file.display());
        }
        if formatted != source {
            if check {
                unformatted.push(file.display().to_string());
            } else {
                std::fs::write(file, formatted).map_err(|e| format!("{}: {e}", file.display()))?;
            }
        }
    }
    if unformatted.is_empty() {
        Ok(())
    } else {
        Err(format!("html! markup is not formatted in {}; run corten fmt", unformatted.join(", ")))
    }
}

/// The Rust files under `path`, skipping build output and dependencies.
fn collect(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        if path.extension().is_some_and(|e| e == "rs") {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(path)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        collect(&entry.path(), files)?;
    }
    Ok(())
}

/// rustfmt's `max_width` for `file`: from the nearest rustfmt.toml, else 100.
fn max_width(file: &Path) -> usize {
    let start = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    for dir in start.ancestors().skip(1) {
        for name in ["rustfmt.toml", ".rustfmt.toml"] {
            if let Ok(text) = std::fs::read_to_string(dir.join(name)) {
                for line in text.lines() {
                    if let Some((key, value)) = line.split_once('=')
                        && key.trim() == "max_width"
                        && let Ok(width) = value.trim().parse()
                    {
                        return width;
                    }
                }
                return 100;
            }
        }
    }
    100
}

/// Formats every `html! { .. }` in `source`. Returns the new source and a warning (`line: why`)
/// for each body left as it was.
pub fn format_source(source: &str, width: usize) -> (String, Vec<String>) {
    let Ok(tokens) = lex(source) else {
        return (source.to_string(), Vec::new());
    };
    let mut out = String::with_capacity(source.len());
    let mut warnings = Vec::new();
    let mut copied = 0;
    let mut i = 0;
    while i + 2 < tokens.len() {
        let (a, b, c) = (&tokens[i], &tokens[i + 1], &tokens[i + 2]);
        let invocation = a.kind == Kind::Ident
            && &source[a.start..a.end] == "html"
            && b.kind == Kind::Punct
            && &source[b.start..b.end] == "!"
            && c.kind == Kind::Open
            && &source[c.start..c.end] == "{";
        if !invocation {
            i += 1;
            continue;
        }
        let Some(close) = matching(&tokens, i + 2) else { break };
        let line_start = source[..a.start].rfind('\n').map_or(0, |p| p + 1);
        let base = indentation(&source[line_start..]);
        let column = a.start - line_start;
        let after = source[tokens[close].end..].split('\n').next().unwrap_or_default().trim_end().len();
        match format_body(source, &tokens, i + 3, close, base, column + after, width) {
            Ok(text) => {
                let original = &source[c.start..tokens[close].end];
                if same_tokens(original, &text) {
                    out.push_str(&source[copied..c.start]);
                    out.push_str(&text);
                    copied = tokens[close].end;
                } else {
                    warnings.push(format!("{}: left as written (formatting would change its tokens)", line_of(source, a.start)));
                }
            }
            Err(why) => warnings.push(format!("{}: left as written ({why})", line_of(source, a.start))),
        }
        i = close + 1;
    }
    out.push_str(&source[copied..]);
    (out, warnings)
}

fn line_of(source: &str, at: usize) -> usize {
    source[..at].matches('\n').count() + 1
}

fn indentation(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

// ---- Tokens ------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Ident,
    Lifetime,
    Literal,
    Punct,
    Open,
    Close,
    LineComment,
    BlockComment,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

/// Rust tokens, comments included, by byte range.
fn lex(src: &str) -> Result<Vec<Token>, String> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        let kind = if c.is_ascii_whitespace() {
            i += 1;
            continue;
        } else if src[i..].starts_with("//") {
            i = src[i..].find('\n').map_or(bytes.len(), |p| i + p);
            Kind::LineComment
        } else if src[i..].starts_with("/*") {
            let mut depth = 0;
            loop {
                if src[i..].starts_with("/*") {
                    depth += 1;
                    i += 2;
                } else if src[i..].starts_with("*/") {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else if i >= bytes.len() {
                    return Err("unterminated comment".into());
                } else {
                    i += 1;
                }
            }
            Kind::BlockComment
        } else if let Some(end) = string_literal(src, i) {
            i = end;
            Kind::Literal
        } else if c == b'\'' {
            let rest = &src[i + 1..];
            let mut chars = rest.char_indices();
            match (chars.next(), chars.next()) {
                (Some((_, '\\')), _) => {
                    let mut j = i + 2;
                    while j < bytes.len() && bytes[j] != b'\'' {
                        j += if bytes[j] == b'\\' { 2 } else { 1 };
                    }
                    i = j + 1;
                    Kind::Literal
                }
                (Some(_), Some((k, '\''))) => {
                    i += 1 + k + 1;
                    Kind::Literal
                }
                _ => {
                    i += 1;
                    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                        i += 1;
                    }
                    Kind::Lifetime
                }
            }
        } else if c.is_ascii_digit() {
            i += 1;
            // Digits, suffixes, a decimal point before a digit (not `1..2` or `1.max()`), and an
            // exponent's sign.
            let continues = |i: usize| {
                let Some(&b) = bytes.get(i) else { return false };
                b.is_ascii_alphanumeric()
                    || b == b'_'
                    || (b == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
                    || ((b == b'+' || b == b'-') && matches!(bytes[i - 1], b'e' | b'E') && !src[start..i].starts_with("0x"))
            };
            while continues(i) {
                i += 1;
            }
            Kind::Literal
        } else if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 {
            if src[i..].starts_with("r#") {
                i += 2;
            }
            while i < bytes.len() {
                let ch = src[i..].chars().next().expect("in bounds");
                if ch.is_alphanumeric() || ch == '_' {
                    i += ch.len_utf8();
                } else {
                    break;
                }
            }
            Kind::Ident
        } else if matches!(c, b'(' | b'[' | b'{') {
            i += 1;
            Kind::Open
        } else if matches!(c, b')' | b']' | b'}') {
            i += 1;
            Kind::Close
        } else {
            i += src[i..].chars().next().map_or(1, char::len_utf8);
            Kind::Punct
        };
        tokens.push(Token { kind, start, end: i });
    }
    Ok(tokens)
}

/// The end of a string literal starting at `i` (`".."`, `b".."`, `c".."`, `r#".."#`, `br".."`), if one does.
fn string_literal(src: &str, i: usize) -> Option<usize> {
    let rest = &src[i..];
    let prefix = ["br", "cr", "b", "c", "r", ""].into_iter().find(|p| {
        rest.strip_prefix(p).is_some_and(|after| after.starts_with('"') || (p.ends_with('r') && after.starts_with('#')))
    })?;
    let mut j = i + prefix.len();
    let bytes = src.as_bytes();
    if prefix.ends_with('r') {
        let hashes = src[j..].bytes().take_while(|&b| b == b'#').count();
        j += hashes;
        if bytes.get(j) != Some(&b'"') {
            return None;
        }
        let close = format!("\"{}", "#".repeat(hashes));
        return src[j + 1..].find(&close).map(|p| j + 1 + p + close.len());
    }
    j += 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 2,
            b'"' => return Some(j + 1),
            _ => j += 1,
        }
    }
    None
}

/// The index of the token that closes the group opened at `open`.
fn matching(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            Kind::Open => depth += 1,
            Kind::Close => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// True when `a` and `b` are the same tokens, whitespace aside (block comments compared
/// with their whitespace collapsed).
fn same_tokens(a: &str, b: &str) -> bool {
    let (Ok(x), Ok(y)) = (lex(a), lex(b)) else { return false };
    let text = |src: &str, t: &Token| {
        let s = &src[t.start..t.end];
        if t.kind == Kind::BlockComment { s.split_whitespace().collect::<Vec<_>>().join(" ") } else { s.trim_end().to_string() }
    };
    x.len() == y.len() && x.iter().zip(&y).all(|(p, q)| p.kind == q.kind && text(a, p) == text(b, q))
}

// ---- Markup ------------------------------------------------------------------------------------

#[derive(Debug)]
enum Node {
    /// A string literal, `{expr}`, the doctype or a `let`: written as it is. `inline` for text and
    /// expressions.
    Leaf {
        text: Text,
        inline: bool,
    },
    Comment(String),
    /// `expanded`: written with a line break after the start tag, so never put on one line;
    /// `tall`: written with a line break before the first attribute, so one attribute per line.
    Element {
        name: String,
        attrs: Vec<Attr>,
        children: Option<Vec<Child>>,
        self_closing: bool,
        component: bool,
        expanded: bool,
        tall: bool,
    },
    Fragment {
        children: Vec<Child>,
        expanded: bool,
    },
    /// `for .. {}`, `if .. {} else if .. {} else {}`: each head with its body; `expanded` when
    /// any body was written with a line break after its `{`.
    Block {
        branches: Vec<(Text, Vec<Child>)>,
        expanded: bool,
    },
    Match {
        head: Text,
        arms: Vec<Arm>,
    },
}

#[derive(Debug)]
struct Child {
    node: Node,
    /// A blank line before it in the source.
    gap: bool,
    /// A space (and no line break) between it and the sibling before it in the source.
    space: bool,
    /// A line break between it and the sibling before it in the source.
    newline: bool,
}

#[derive(Debug)]
struct Arm {
    pattern: Text,
    /// `Some` for `pat => { .. }`, else the one node after `=>`.
    braced: Option<Vec<Child>>,
    /// The braced body was written with a line break after its `{`.
    expanded: bool,
    single: Option<Box<Child>>,
    comma: bool,
    gap: bool,
}

#[derive(Debug)]
enum Attr {
    /// `name`, `name="x"`, `name={..}`, `{..spread}`.
    Item(Text),
    Comment(String),
}

/// Source text and the indentation of the line it starts on, so continuation lines can move.
#[derive(Debug, Clone)]
struct Text {
    text: String,
    indent: usize,
}

impl Text {
    fn multiline(&self) -> bool {
        self.text.contains('\n')
    }

    /// The text placed at a line indented `to`: continuation lines keep their offset from the line
    /// the text started on.
    fn place(&self, to: usize) -> String {
        // A string literal over several lines is its own content: leave the text as it is.
        let literal_lines = lex(&self.text).is_ok_and(|tokens| {
            tokens
                .iter()
                .any(|t| matches!(t.kind, Kind::Literal | Kind::BlockComment) && self.text[t.start..t.end].contains('\n'))
        });
        if literal_lines {
            return self.text.clone();
        }
        let mut lines = self.text.split('\n');
        let mut out = lines.next().unwrap_or_default().to_string();
        for line in lines {
            out.push('\n');
            if line.trim().is_empty() {
                continue;
            }
            let lead = indentation(line);
            out.push_str(&" ".repeat((lead + to).saturating_sub(self.indent)));
            out.push_str(line.trim_start_matches(' '));
        }
        out
    }
}

struct Parser<'a> {
    src: &'a str,
    tokens: &'a [Token],
    i: usize,
    end: usize,
}

type Parsed<T> = Result<T, String>;

impl<'a> Parser<'a> {
    fn peek(&self, n: usize) -> Option<&Token> {
        (self.i + n < self.end).then(|| &self.tokens[self.i + n])
    }

    fn is(&self, n: usize, kind: Kind, text: &str) -> bool {
        self.peek(n).is_some_and(|t| t.kind == kind && &self.src[t.start..t.end] == text)
    }

    fn text_of(&self, from: usize, to: usize) -> Text {
        let start = self.tokens[from].start;
        let line_start = self.src[..start].rfind('\n').map_or(0, |p| p + 1);
        Text { text: self.src[start..self.tokens[to].end].to_string(), indent: indentation(&self.src[line_start..]) }
    }

    /// A blank line between the previous token and the next.
    fn gap(&self) -> bool {
        self.i > 0
            && self.i < self.tokens.len()
            && self.src[self.tokens[self.i - 1].end..self.tokens[self.i].start].matches('\n').count() > 1
    }

    /// A line break between token `at` and the one after it.
    fn breaks_after(&self, at: usize) -> bool {
        at + 1 < self.tokens.len() && self.src[self.tokens[at].end..self.tokens[at + 1].start].contains('\n')
    }

    fn group(&self, at: usize) -> Parsed<usize> {
        matching(self.tokens, at).filter(|&c| c < self.end).ok_or_else(|| "unbalanced braces".to_string())
    }

    fn sub(&self, from: usize, to: usize) -> Parser<'a> {
        Parser { src: self.src, tokens: self.tokens, i: from, end: to }
    }

    fn nodes(&mut self, closing: bool) -> Parsed<Vec<Child>> {
        let mut nodes = Vec::new();
        while self.i < self.end {
            if closing && self.is(0, Kind::Punct, "<") && self.is(1, Kind::Punct, "/") {
                break;
            }
            let gap = self.gap() && !nodes.is_empty();
            let between = if nodes.is_empty() { "" } else { &self.src[self.tokens[self.i - 1].end..self.tokens[self.i].start] };
            let (space, newline) = (!between.is_empty() && !between.contains('\n'), between.contains('\n'));
            nodes.push(Child { node: self.node()?, gap, space, newline });
        }
        Ok(nodes)
    }

    fn node(&mut self) -> Parsed<Node> {
        let token = *self.peek(0).ok_or("unexpected end")?;
        let word = &self.src[token.start..token.end];
        match token.kind {
            Kind::LineComment | Kind::BlockComment => {
                self.i += 1;
                Ok(Node::Comment(word.trim_end().to_string()))
            }
            Kind::Literal => {
                self.i += 1;
                Ok(Node::Leaf { text: self.text_of(self.i - 1, self.i - 1), inline: true })
            }
            Kind::Open if word == "{" => {
                let close = self.group(self.i)?;
                let text = self.text_of(self.i, close);
                self.i = close + 1;
                Ok(Node::Leaf { text, inline: true })
            }
            Kind::Punct if word == "<" => self.tag(),
            Kind::Ident if word == "for" => {
                let (head, body, expanded) = self.until_brace()?;
                Ok(Node::Block { branches: vec![(head, body)], expanded })
            }
            Kind::Ident if word == "if" => {
                let (head, body, mut expanded) = self.until_brace()?;
                let mut branches = vec![(head, body)];
                while self.is(0, Kind::Ident, "else") {
                    if self.is(1, Kind::Ident, "if") {
                        let (head, body, broken) = self.until_brace()?;
                        branches.push((head, body));
                        expanded |= broken;
                    } else {
                        let start = self.i;
                        self.i += 1;
                        if !self.is(0, Kind::Open, "{") {
                            return Err("expected { after else".into());
                        }
                        let close = self.group(self.i)?;
                        let body = self.sub(self.i + 1, close).nodes(false)?;
                        expanded |= !body.is_empty() && self.breaks_after(self.i);
                        branches.push((self.text_of(start, start), body));
                        self.i = close + 1;
                    }
                }
                Ok(Node::Block { branches, expanded })
            }
            Kind::Ident if word == "match" => {
                let start = self.i;
                while !self.is(0, Kind::Open, "{") {
                    self.skip_tree()?;
                }
                let head = self.text_of(start, self.i - 1);
                let close = self.group(self.i)?;
                let mut arms = Vec::new();
                let mut p = self.sub(self.i + 1, close);
                while p.i < p.end {
                    let gap = p.gap() && !arms.is_empty();
                    let from = p.i;
                    while !(p.is(0, Kind::Punct, "=") && p.is(1, Kind::Punct, ">")) {
                        if p.i >= p.end {
                            return Err("expected => in a match arm".into());
                        }
                        p.skip_tree()?;
                    }
                    p.i += 2;
                    let pattern = p.text_of(from, p.i - 1);
                    let (braced, expanded, single) = if p.is(0, Kind::Open, "{") {
                        let end = p.group(p.i)?;
                        let body = p.sub(p.i + 1, end).nodes(false)?;
                        let expanded = !body.is_empty() && p.breaks_after(p.i);
                        p.i = end + 1;
                        (Some(body), expanded, None)
                    } else {
                        (None, false, Some(Box::new(Child { node: p.node()?, gap: false, space: false, newline: false })))
                    };
                    let comma = p.is(0, Kind::Punct, ",");
                    if comma {
                        p.i += 1;
                    }
                    arms.push(Arm { pattern, braced, expanded, single, comma, gap });
                }
                self.i = close + 1;
                Ok(Node::Match { head, arms })
            }
            Kind::Ident if word == "let" => {
                let start = self.i;
                while !self.is(0, Kind::Punct, ";") {
                    if self.i >= self.end {
                        return Err("a let without ;".into());
                    }
                    self.skip_tree()?;
                }
                self.i += 1;
                Ok(Node::Leaf { text: self.text_of(start, self.i - 1), inline: false })
            }
            _ => Err(format!("unexpected {word:?}")),
        }
    }

    /// Steps over one token, or a whole group.
    fn skip_tree(&mut self) -> Parsed<()> {
        let token = self.peek(0).ok_or("unexpected end")?;
        self.i = if token.kind == Kind::Open { self.group(self.i)? + 1 } else { self.i + 1 };
        Ok(())
    }

    /// `keyword head { body }`: the text up to the brace, the body's nodes, and whether the body
    /// was written with a line break after its `{`.
    fn until_brace(&mut self) -> Parsed<(Text, Vec<Child>, bool)> {
        let start = self.i;
        self.i += 1;
        if self.is(0, Kind::Ident, "if") {
            self.i += 1;
        }
        while !self.is(0, Kind::Open, "{") {
            if self.i >= self.end {
                return Err("expected { .. }".into());
            }
            self.skip_tree()?;
        }
        let head = self.text_of(start, self.i - 1);
        let close = self.group(self.i)?;
        let body = self.sub(self.i + 1, close).nodes(false)?;
        let expanded = !body.is_empty() && self.breaks_after(self.i);
        self.i = close + 1;
        Ok((head, body, expanded))
    }

    /// A name joined from `a-b`, `a::b` or (attributes) `a:b`.
    fn name(&mut self, colons: bool) -> Parsed<String> {
        let token = *self.peek(0).ok_or("expected a name")?;
        if token.kind != Kind::Ident {
            return Err("expected a name".into());
        }
        self.i += 1;
        let mut name = self.src[token.start..token.end].to_string();
        loop {
            let joined = |p: &Parser, n: usize| p.peek(n).is_some_and(|t| matches!(t.kind, Kind::Ident | Kind::Literal));
            if self.is(0, Kind::Punct, "-") && joined(self, 1) {
                name.push('-');
            } else if self.is(0, Kind::Punct, ":") && self.is(1, Kind::Punct, ":") && joined(self, 2) {
                name.push_str("::");
                self.i += 1;
            } else if colons && self.is(0, Kind::Punct, ":") && joined(self, 1) {
                name.push(':');
            } else {
                return Ok(name);
            }
            self.i += 1;
            let part = self.tokens[self.i];
            name.push_str(&self.src[part.start..part.end]);
            self.i += 1;
        }
    }

    fn tag(&mut self) -> Parsed<Node> {
        self.i += 1;
        if self.is(0, Kind::Punct, "!") {
            let start = self.i - 1;
            while !self.is(0, Kind::Punct, ">") {
                if self.i >= self.end {
                    return Err("unclosed doctype".into());
                }
                self.i += 1;
            }
            self.i += 1;
            let text = self.text_of(start, self.i - 1).text.split_whitespace().collect::<Vec<_>>().join(" ").replace("< !", "<!");
            return Ok(Node::Leaf { text: Text { text: text.replace(" >", ">"), indent: 0 }, inline: false });
        }
        if self.is(0, Kind::Punct, ">") {
            self.i += 1;
            let broken = self.breaks_after(self.i - 1);
            let children = self.nodes(true)?;
            self.closing("")?;
            return Ok(Node::Fragment { expanded: broken && !children.is_empty(), children });
        }
        let name = self.name(false)?;
        let tall = self.breaks_after(self.i - 1) && !self.is(0, Kind::Punct, ">") && !self.is(0, Kind::Punct, "/");
        let component = name.contains("::") || name.starts_with(|c: char| c.is_ascii_uppercase());
        let mut attrs = Vec::new();
        let self_closing = loop {
            let token = *self.peek(0).ok_or_else(|| format!("<{name}> is not closed"))?;
            let word = &self.src[token.start..token.end];
            match token.kind {
                Kind::Punct if word == ">" => {
                    self.i += 1;
                    break false;
                }
                Kind::Punct if word == "/" && self.is(1, Kind::Punct, ">") => {
                    self.i += 2;
                    break true;
                }
                Kind::LineComment | Kind::BlockComment => {
                    attrs.push(Attr::Comment(word.trim_end().to_string()));
                    self.i += 1;
                }
                Kind::Open if word == "{" => {
                    let close = self.group(self.i)?;
                    attrs.push(Attr::Item(self.text_of(self.i, close)));
                    self.i = close + 1;
                }
                Kind::Ident => {
                    let start = self.i;
                    let attr = self.name(true)?;
                    let mut text = Text { text: attr, indent: 0 };
                    if self.is(0, Kind::Punct, "=") {
                        self.i += 1;
                        let value = *self.peek(0).ok_or("expected a value")?;
                        let end = if value.kind == Kind::Open { self.group(self.i)? } else { self.i };
                        let v = self.text_of(self.i, end);
                        text = Text { text: format!("{}={}", text.text, v.text), indent: v.indent };
                        self.i = end + 1;
                    } else {
                        text.indent = self.text_of(start, start).indent;
                    }
                    attrs.push(Attr::Item(text));
                }
                _ => return Err(format!("unexpected {word:?} in <{name}>")),
            }
        };
        let mut expanded = false;
        let children = if self_closing || (!component && VOID.contains(&name.as_str())) {
            None
        } else {
            let broken = self.breaks_after(self.i - 1);
            let children = self.nodes(true)?;
            expanded = broken && !children.is_empty();
            self.closing(&name)?;
            Some(children)
        };
        Ok(Node::Element { name, attrs, children, self_closing, component, expanded, tall })
    }

    fn closing(&mut self, name: &str) -> Parsed<()> {
        if !(self.is(0, Kind::Punct, "<") && self.is(1, Kind::Punct, "/")) {
            return Err(format!("<{name}> is not closed"));
        }
        self.i += 2;
        let found = if self.is(0, Kind::Punct, ">") { String::new() } else { self.name(false)? };
        if found != name || !self.is(0, Kind::Punct, ">") {
            return Err(format!("expected </{name}>"));
        }
        self.i += 1;
        Ok(())
    }
}

/// The body between tokens `from` and `close` (its `}`), laid out for a macro on a line indented
/// `base`; `column` is where `html!` starts plus what follows the closing brace on its line.
fn format_body(
    src: &str,
    tokens: &[Token],
    from: usize,
    close: usize,
    base: usize,
    column: usize,
    width: usize,
) -> Parsed<String> {
    if from == close {
        return Ok("{}".into());
    }
    let mut parser = Parser { src, tokens, i: from, end: close };
    let nodes = parser.nodes(false)?;
    let printer = Printer { width };
    // One line, `html! { <p>..</p> }`, when it fits after the `html! ` already on the line and was
    // not written expanded.
    let expanded = parser.breaks_after(from - 1);
    if let Some(flat) = printer.flat_list(&nodes).filter(|_| !expanded)
        && column + "html! { ".len() + flat.len() + " }".len() <= width
    {
        return Ok(format!("{{ {flat} }}"));
    }
    let mut out = String::from("{\n");
    printer.list(&nodes, base + 4, &mut out);
    out.push_str(&" ".repeat(base));
    out.push('}');
    Ok(out)
}

struct Printer {
    width: usize,
}

impl Printer {
    /// A node on one line, if it can be: no comments, no multi-line Rust, and (for an element)
    /// children that are all inline or just one.
    fn flat(&self, node: &Node) -> Option<String> {
        match node {
            Node::Leaf { text, .. } => (!text.multiline()).then(|| text.text.clone()),
            Node::Comment(_) => None,
            Node::Element { expanded: true, .. }
            | Node::Element { tall: true, .. }
            | Node::Fragment { expanded: true, .. }
            | Node::Block { expanded: true, .. } => None,
            Node::Element { name, attrs, children, self_closing, .. } => {
                let mut out = format!("<{name}");
                for attr in attrs {
                    match attr {
                        Attr::Item(text) if !text.multiline() => {
                            out.push(' ');
                            out.push_str(&text.text);
                        }
                        _ => return None,
                    }
                }
                if *self_closing {
                    out.push_str(" />");
                    return Some(out);
                }
                out.push('>');
                let Some(children) = children else { return Some(out) };
                out.push_str(&self.flat_list(children)?);
                out.push_str(&format!("</{name}>"));
                Some(out)
            }
            Node::Fragment { children, .. } => Some(format!("<>{}</>", self.flat_list(children)?)),
            Node::Block { branches, .. } => {
                let mut out = String::new();
                for (i, (head, body)) in branches.iter().enumerate() {
                    if head.multiline() {
                        return None;
                    }
                    if i > 0 {
                        out.push(' ');
                    }
                    out.push_str(&head.text);
                    out.push_str(&if body.is_empty() { " {}".into() } else { format!(" {{ {} }}", self.flat_list(body)?) });
                }
                Some(out)
            }
            Node::Match { .. } => None,
        }
    }

    /// Siblings on one line: inline nodes side by side, or a single node.
    fn flat_list(&self, nodes: &[Child]) -> Option<String> {
        if nodes.len() > 1 && !nodes.iter().all(|c| inline(&c.node)) {
            return None;
        }
        let mut out = String::new();
        for child in nodes {
            if child.gap || child.newline {
                return None;
            }
            if child.space && !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&self.flat(&child.node)?);
        }
        Some(out)
    }

    /// Siblings, one per line from `indent`, with inline runs filling lines.
    fn list(&self, nodes: &[Child], indent: usize, out: &mut String) {
        let pad = " ".repeat(indent);
        let mut line = String::new();
        let flush = |line: &mut String, out: &mut String| {
            if !line.is_empty() {
                out.push_str(&pad);
                out.push_str(line);
                out.push('\n');
                line.clear();
            }
        };
        let mut previous_leaf = false;
        for child in nodes {
            if child.gap {
                flush(&mut line, out);
                out.push('\n');
            }
            let leaf = matches!(child.node, Node::Leaf { .. });
            // Two elements in a row start a new line, and so does a node the author put on its own
            // line; otherwise text joins whatever it is next to.
            if (!leaf && !previous_leaf) || child.newline {
                flush(&mut line, out);
            }
            previous_leaf = leaf;
            if inline(&child.node)
                && let Some(flat) = self.flat(&child.node)
            {
                let space = if child.space && !line.is_empty() { " " } else { "" };
                if indent + line.len() + space.len() + flat.len() <= self.width {
                    line.push_str(space);
                    line.push_str(&flat);
                    continue;
                }
                flush(&mut line, out);
                if indent + flat.len() <= self.width {
                    line.push_str(&flat);
                    continue;
                }
            }
            flush(&mut line, out);
            self.block(&child.node, indent, out);
        }
        flush(&mut line, out);
    }

    /// One node on its own lines, ending with a newline.
    fn block(&self, node: &Node, indent: usize, out: &mut String) {
        let pad = " ".repeat(indent);
        if let Some(flat) = self.flat(node)
            && indent + flat.len() <= self.width
        {
            out.push_str(&pad);
            out.push_str(&flat);
            out.push('\n');
            return;
        }
        match node {
            Node::Leaf { text, .. } => {
                out.push_str(&pad);
                out.push_str(&text.place(indent));
                out.push('\n');
            }
            Node::Comment(text) => {
                out.push_str(&pad);
                out.push_str(text);
                out.push('\n');
            }
            Node::Element { name, attrs, children, self_closing, tall, .. } => {
                out.push_str(&pad);
                out.push('<');
                out.push_str(name);
                let end = if *self_closing { " />" } else { ">" };
                let one_line: Option<String> = attrs
                    .iter()
                    .map(|a| match a {
                        Attr::Item(t) if !t.multiline() => Some(format!(" {}", t.text)),
                        _ => None,
                    })
                    .collect();
                match one_line.filter(|_| !tall) {
                    Some(text) if indent + 1 + name.len() + text.len() + end.len() <= self.width => {
                        out.push_str(&text);
                        out.push_str(end);
                    }
                    _ => {
                        out.push('\n');
                        for attr in attrs {
                            out.push_str(&pad);
                            out.push_str("    ");
                            match attr {
                                Attr::Item(text) => out.push_str(&text.place(indent + 4)),
                                Attr::Comment(text) => out.push_str(text),
                            }
                            out.push('\n');
                        }
                        out.push_str(&pad);
                        out.push_str(end.trim_start());
                    }
                }
                match children {
                    None => out.push('\n'),
                    Some(children) if children.is_empty() => {
                        out.push_str(&format!("</{name}>\n"));
                    }
                    Some(children) => {
                        out.push('\n');
                        self.list(children, indent + 4, out);
                        out.push_str(&format!("{pad}</{name}>\n"));
                    }
                }
            }
            Node::Fragment { children, .. } => {
                out.push_str(&format!("{pad}<>\n"));
                self.list(children, indent + 4, out);
                out.push_str(&format!("{pad}</>\n"));
            }
            Node::Block { branches, .. } => {
                out.push_str(&pad);
                for (i, (head, body)) in branches.iter().enumerate() {
                    if i > 0 {
                        out.push_str("} ");
                    }
                    out.push_str(&head.place(indent));
                    out.push_str(" {\n");
                    self.list(body, indent + 4, out);
                    out.push_str(&pad);
                }
                out.push_str("}\n");
            }
            Node::Match { head, arms } => {
                out.push_str(&pad);
                out.push_str(&head.place(indent));
                out.push_str(" {\n");
                let arm_pad = " ".repeat(indent + 4);
                for arm in arms {
                    if arm.gap {
                        out.push('\n');
                    }
                    let comma = if arm.comma { "," } else { "" };
                    let lead = format!("{arm_pad}{} ", arm.pattern.place(indent + 4));
                    if let Some(body) = &arm.braced {
                        let flat = self.flat_list(body).filter(|_| !arm.pattern.multiline() && !arm.expanded);
                        match flat {
                            Some(_) if body.is_empty() => out.push_str(&format!("{lead}{{}}{comma}\n")),
                            Some(flat) if lead.len() + flat.len() + 4 + comma.len() <= self.width => {
                                out.push_str(&format!("{lead}{{ {flat} }}{comma}\n"));
                            }
                            _ => {
                                out.push_str(&format!("{lead}{{\n"));
                                self.list(body, indent + 8, out);
                                out.push_str(&format!("{arm_pad}}}{comma}\n"));
                            }
                        }
                    } else if let Some(single) = &arm.single {
                        let mut node = String::new();
                        self.block(&single.node, indent + 4, &mut node);
                        let node = node.trim_end_matches('\n');
                        out.push_str(&lead);
                        out.push_str(node.trim_start_matches(' '));
                        out.push_str(comma);
                        out.push('\n');
                    }
                }
                out.push_str(&pad);
                out.push_str("}\n");
            }
        }
    }
}

/// Text, an expression or an inline element: laid out side by side with its neighbors.
fn inline(node: &Node) -> bool {
    match node {
        Node::Leaf { inline, .. } => *inline,
        Node::Element { name, component, .. } => !component && INLINE.contains(&name.as_str()),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
