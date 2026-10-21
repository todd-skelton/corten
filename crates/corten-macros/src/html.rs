//! `html!`: JSX-like markup compiled to string pushes.
//!
//! Static markup is merged at compile time into one `push_str` per run; expressions are escaped
//! through `corten::view::Render` (or `Display`); control flow is plain Rust (`for`, `if`,
//! `match`, `let`) written inside the markup, so no intermediate strings or trees are built.

use proc_macro::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};

use crate::tokens::{
    compile_error, err, escape, is_ident, is_punct, literal, literal_text, parens, q, stream, string_value, Error,
};

/// Elements that never have children or a closing tag.
const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];
/// Elements whose text is not HTML (written as is, never escaped).
const RAW_TEXT: &[&str] = &["script", "style"];
/// Rust keywords that need `r#` as a component prop name.
const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "dyn", "else", "enum", "extern", "fn", "for", "if", "impl", "in",
    "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "static", "struct", "trait", "type", "unsafe", "use",
    "where", "while", "yield",
];

pub fn expand(input: TokenStream) -> TokenStream {
    match Cursor::new(input).nodes(None) {
        Ok(nodes) => {
            let mut gen = Gen::default();
            gen.nodes(&nodes, false);
            q(
                "::corten::view::Html::new({
                    static SIZE: ::std::sync::atomic::AtomicUsize = ::std::sync::atomic::AtomicUsize::new(0);
                    &SIZE
                }, move |__OUT: &mut ::std::string::String| {
                    #[allow(unused_imports)]
                    use ::corten::view::__private::{AttrViaDisplay as _, AttrViaValue as _, ViaDisplay as _, ViaRender as _};
                    __A0
                })",
                &[gen.finish()],
            )
        }
        Err((span, message)) => compile_error(span, &message),
    }
}

enum Node {
    Element {
        name: String,
        attrs: Vec<Attr>,
        children: Vec<Node>,
    },
    Component {
        path: TokenStream,
        props: Vec<(TokenStream, TokenStream)>,
        children: Option<Vec<Node>>,
    },
    Text(String),
    Markup(String),
    Expr(TokenStream),
    /// `for`, `if`/`else`, `match`: the Rust tokens around each body, with the bodies in order.
    Block {
        head: Vec<TokenStream>,
        bodies: Vec<Vec<Node>>,
        arms: bool,
    },
    Let(TokenStream),
}

enum Attr {
    Bare(String),
    Static(String, String),
    Expr(String, TokenStream),
    Spread(TokenStream),
    /// `ref={Rows}`: prints `data-ref="rows"`.
    Ref(TokenStream),
    /// `on:click={ShowAll}`: the event and the action. All of an element's print as one
    /// `data-on="click:showAll"`.
    On(String, TokenStream),
}

struct Cursor {
    tokens: Vec<TokenTree>,
    i: usize,
    end: Span,
}

impl Cursor {
    fn new(stream: TokenStream) -> Self {
        Cursor { tokens: stream.into_iter().collect(), i: 0, end: Span::call_site() }
    }

    fn of(group: &Group) -> Self {
        Cursor { tokens: group.stream().into_iter().collect(), i: 0, end: group.span_close() }
    }

    fn peek(&self) -> Option<&TokenTree> {
        self.tokens.get(self.i)
    }

    fn peek2(&self) -> Option<&TokenTree> {
        self.tokens.get(self.i + 1)
    }

    fn next(&mut self) -> Option<TokenTree> {
        let token = self.tokens.get(self.i).cloned();
        self.i += 1;
        token
    }

    fn span(&self) -> Span {
        self.peek().map_or(self.end, TokenTree::span)
    }

    fn at_end(&self) -> bool {
        self.i >= self.tokens.len()
    }

    /// Nodes until the end, or until `</` when inside the element named `closing`.
    fn nodes(&mut self, closing: Option<&str>) -> Result<Vec<Node>, Error> {
        let mut nodes = Vec::new();
        while !self.at_end() {
            if is_punct(self.peek(), '<') && is_punct(self.peek2(), '/') {
                if closing.is_some() {
                    break;
                }
                let span = self.span();
                self.next();
                self.next();
                let name = self.name().map(|n| n.0).unwrap_or_default();
                let hint = if VOID.contains(&name.as_str()) {
                    format!("; <{name}> is a void element and has no closing tag")
                } else {
                    String::new()
                };
                return err(span, format!("unexpected closing tag </{name}>{hint}"));
            }
            nodes.push(self.node()?);
        }
        Ok(nodes)
    }

    fn node(&mut self) -> Result<Node, Error> {
        let span = self.span();
        match self.peek().cloned() {
            Some(TokenTree::Punct(p)) if p.as_char() == '<' => self.tag(),
            Some(TokenTree::Literal(lit)) => {
                self.next();
                match string_value(&lit).or_else(|| literal_text(&lit)) {
                    Some(text) => Ok(Node::Text(text)),
                    None => err(span, "write text as a string literal: \"...\""),
                }
            }
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                self.next();
                let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                if inner.is_empty() {
                    return err(span, "empty braces: put a Rust expression inside, like {name}");
                }
                if is_punct(inner.first(), '.') && is_punct(inner.get(1), '.') {
                    return err(span, "{..attributes} goes inside a start tag, like <input {..attrs}>");
                }
                Ok(Node::Expr(g.stream()))
            }
            Some(TokenTree::Ident(id)) => match id.to_string().as_str() {
                "for" => self.block_for(),
                "if" => self.block_if(),
                "match" => self.block_match(),
                "let" => {
                    let tokens = self.until_semicolon()?;
                    Ok(Node::Let(tokens))
                }
                word => err(
                    span,
                    format!("unexpected `{word}`: write text in quotes (\"{word}\") and Rust expressions in braces ({{{word}}})"),
                ),
            },
            Some(other) => err(
                other.span(),
                format!("unexpected `{other}`: expected <element>, \"text\", {{expression}}, for, if, match or let"),
            ),
            None => err(span, "unexpected end of markup"),
        }
    }

    /// A tag name: `div`, `listing-sort`, `Card` or `ui::Card`. Returns the name, its tokens and
    /// whether it names a component (a path, or a name starting with an uppercase letter).
    fn name(&mut self) -> Result<(String, TokenStream, Span), Error> {
        let span = self.span();
        let Some(TokenTree::Ident(first)) = self.next() else {
            return err(span, "expected a tag name");
        };
        let mut name = first.to_string();
        let mut tokens = vec![TokenTree::Ident(first)];
        loop {
            match (self.peek(), self.peek2()) {
                (Some(TokenTree::Punct(p)), Some(TokenTree::Ident(_) | TokenTree::Literal(_))) if p.as_char() == '-' => {
                    let dash = self.next().unwrap();
                    let part = self.next().unwrap();
                    name.push('-');
                    name.push_str(&part.to_string());
                    tokens.extend([dash, part]);
                }
                (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b))) if a.as_char() == ':' && b.as_char() == ':' => {
                    tokens.extend([self.next().unwrap(), self.next().unwrap()]);
                    let span = self.span();
                    match self.next() {
                        Some(TokenTree::Ident(part)) => {
                            name.push_str("::");
                            name.push_str(&part.to_string());
                            tokens.push(TokenTree::Ident(part));
                        }
                        _ => return err(span, "expected a name after ::"),
                    }
                }
                _ => break,
            }
        }
        Ok((name, stream(tokens), span))
    }

    fn tag(&mut self) -> Result<Node, Error> {
        let open = self.span();
        self.next();
        if is_punct(self.peek(), '!') {
            self.next();
            let doctype = self.next();
            let html = self.next();
            if !(matches!(&doctype, Some(TokenTree::Ident(d)) if d.to_string().eq_ignore_ascii_case("doctype"))
                && matches!(&html, Some(TokenTree::Ident(h)) if h.to_string() == "html")
                && is_punct(self.peek(), '>'))
            {
                return err(open, "the only declaration is <!DOCTYPE html>");
            }
            self.next();
            return Ok(Node::Markup("<!DOCTYPE html>".into()));
        }
        if is_punct(self.peek(), '>') {
            self.next();
            let children = self.nodes(Some(""))?;
            self.closing("", open)?;
            return Ok(Node::Block { head: vec![TokenStream::new()], bodies: vec![children], arms: false });
        }
        let (name, path, name_span) = self.name()?;
        let component = name.contains("::") || name.starts_with(|c: char| c.is_ascii_uppercase());
        if component && name.contains('-') {
            return err(name_span, "a component name is a Rust path; element names with - are lowercase");
        }
        let mut attrs = Vec::new();
        let mut props = Vec::new();
        let self_closing = loop {
            let span = self.span();
            match self.peek().cloned() {
                Some(TokenTree::Punct(p)) if p.as_char() == '>' => {
                    self.next();
                    break false;
                }
                Some(TokenTree::Punct(p)) if p.as_char() == '/' && is_punct(self.peek2(), '>') => {
                    self.next();
                    self.next();
                    break true;
                }
                Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                    self.next();
                    let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                    if component || !(is_punct(inner.first(), '.') && is_punct(inner.get(1), '.')) {
                        return err(span, "inside a start tag, braces spread attributes: {..attrs}");
                    }
                    attrs.push(Attr::Spread(stream(inner.into_iter().skip(2))));
                }
                Some(TokenTree::Ident(_)) if component => {
                    let Some(TokenTree::Ident(prop)) = self.next() else { unreachable!() };
                    if is_punct(self.peek(), '-') {
                        return err(self.span(), "component props are Rust field names (no -)");
                    }
                    let text = prop.to_string();
                    let field = if KEYWORDS.contains(&text.as_str()) { Ident::new_raw(&text, prop.span()) } else { prop };
                    let value = if is_punct(self.peek(), '=') {
                        self.next();
                        self.value(true)?.1
                    } else {
                        q("true", &[])
                    };
                    props.push((stream([TokenTree::Ident(field)]), value));
                }
                Some(TokenTree::Ident(_)) => {
                    let name = self.attr_name()?;
                    if name == "data-ref" || name == "data-on" {
                        return err(span, "data-ref and data-on are printed from names: write ref={Rows} or on:click={ShowAll} in an #[island] component");
                    }
                    if name == "ref" || name.starts_with("on:") {
                        let braced = is_punct(self.peek(), '=')
                            && matches!(self.peek2(), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace);
                        if !braced {
                            return err(
                                span,
                                format!(
                                    "{name} takes a name of the island's script in braces: ref={{Rows}}, on:click={{ShowAll}}"
                                ),
                            );
                        }
                        self.next();
                        let (_, expr) = self.value(false)?;
                        attrs.push(match name.strip_prefix("on:") {
                            Some(event) => Attr::On(event.to_string(), expr),
                            None => Attr::Ref(expr),
                        });
                        continue;
                    }
                    if is_punct(self.peek(), '=') {
                        self.next();
                        match self.value(false)? {
                            (Some(text), _) => attrs.push(Attr::Static(name, text)),
                            (None, expr) => attrs.push(Attr::Expr(name, expr)),
                        }
                    } else {
                        attrs.push(Attr::Bare(name));
                    }
                }
                _ => return err(span, format!("expected an attribute, > or /> in <{name}>")),
            }
        };
        let void = !component && VOID.contains(&name.as_str());
        let children = if self_closing || void {
            None
        } else {
            let children = self.nodes(Some(&name))?;
            self.closing(&name, open)?;
            Some(children)
        };
        if component {
            return Ok(Node::Component { path, props, children });
        }
        Ok(Node::Element { name, attrs, children: children.unwrap_or_default() })
    }

    /// `</name>` after the children of the element opened at `open`.
    fn closing(&mut self, name: &str, open: Span) -> Result<(), Error> {
        if self.at_end() {
            return err(open, format!("<{name}> is never closed; add </{name}> or write <{name} />"));
        }
        self.next();
        self.next();
        let found = if is_punct(self.peek(), '>') { String::new() } else { self.name()?.0 };
        let span = self.span();
        if found != name {
            return err(span, format!("expected </{name}>, found </{found}>"));
        }
        if !is_punct(self.peek(), '>') {
            return err(span, format!("expected > to end </{name}"));
        }
        self.next();
        Ok(())
    }

    /// `data-ref`, `aria-label`, `xlink:href`.
    fn attr_name(&mut self) -> Result<String, Error> {
        let mut name = self.next().map(|t| t.to_string()).unwrap_or_default();
        while let (Some(TokenTree::Punct(p)), Some(TokenTree::Ident(_) | TokenTree::Literal(_))) = (self.peek(), self.peek2()) {
            if p.as_char() != '-' && p.as_char() != ':' {
                break;
            }
            name.push(p.as_char());
            self.next();
            name.push_str(&self.next().unwrap().to_string());
        }
        Ok(name)
    }

    /// An attribute or prop value: `"text"`, a literal, `{expression}` or a single identifier.
    /// Returns the static text when the value is known at compile time (elements only).
    fn value(&mut self, component: bool) -> Result<(Option<String>, TokenStream), Error> {
        let span = self.span();
        match self.next() {
            Some(TokenTree::Literal(lit)) if !component => match string_value(&lit).or_else(|| literal_text(&lit)) {
                Some(text) => Ok((Some(text), TokenStream::new())),
                None => err(span, "unsupported literal"),
            },
            Some(TokenTree::Literal(lit)) => Ok((None, TokenTree::Literal(lit).into())),
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                if g.stream().is_empty() {
                    return err(span, "empty braces: put a Rust expression inside");
                }
                Ok((None, parens(g.stream())))
            }
            Some(TokenTree::Ident(id)) => Ok((None, TokenTree::Ident(id).into())),
            _ => err(span, "a value is \"text\" or {expression}"),
        }
    }

    /// Tokens up to the next top-level `{...}` group, and the group.
    fn until_brace(&mut self, what: &str) -> Result<(TokenStream, Group), Error> {
        let start = self.span();
        let mut head = Vec::new();
        while let Some(token) = self.next() {
            match token {
                TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => return Ok((stream(head), g)),
                other => head.push(other),
            }
        }
        err(start, format!("expected {{ ... }} after {what}"))
    }

    fn until_semicolon(&mut self) -> Result<TokenStream, Error> {
        let start = self.span();
        let mut tokens = Vec::new();
        while let Some(token) = self.next() {
            let end = matches!(&token, TokenTree::Punct(p) if p.as_char() == ';');
            tokens.push(token);
            if end {
                return Ok(stream(tokens));
            }
        }
        err(start, "a let in markup ends with ;")
    }

    fn block_for(&mut self) -> Result<Node, Error> {
        let keyword = self.next().unwrap();
        let (head, body) = self.until_brace("for ... in ...")?;
        let mut tokens = stream([keyword]);
        tokens.extend(head);
        Ok(Node::Block { head: vec![tokens], bodies: vec![Cursor::of(&body).nodes(None)?], arms: false })
    }

    fn block_if(&mut self) -> Result<Node, Error> {
        let mut heads = Vec::new();
        let mut bodies = Vec::new();
        let mut keyword = stream([self.next().unwrap()]);
        loop {
            let (cond, body) = self.until_brace("if")?;
            keyword.extend(cond);
            heads.push(keyword);
            bodies.push(Cursor::of(&body).nodes(None)?);
            if !is_ident(self.peek(), "else") {
                break;
            }
            keyword = stream([self.next().unwrap()]);
            if is_ident(self.peek(), "if") {
                keyword.extend([self.next().unwrap()]);
                continue;
            }
            let span = self.span();
            match self.next() {
                Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                    heads.push(keyword);
                    bodies.push(Cursor::of(&g).nodes(None)?);
                    break;
                }
                _ => return err(span, "expected { ... } or if after else"),
            }
        }
        Ok(Node::Block { head: heads, bodies, arms: false })
    }

    fn block_match(&mut self) -> Result<Node, Error> {
        let keyword = self.next().unwrap();
        let (scrutinee, arms) = self.until_brace("match")?;
        let mut cursor = Cursor::of(&arms);
        let mut head = stream([keyword]);
        head.extend(scrutinee);
        let mut heads = vec![head];
        let mut bodies = Vec::new();
        while !cursor.at_end() {
            let start = cursor.span();
            let mut pattern = Vec::new();
            loop {
                match (cursor.peek(), cursor.peek2()) {
                    (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b))) if a.as_char() == '=' && b.as_char() == '>' => {
                        pattern.extend([cursor.next().unwrap(), cursor.next().unwrap()]);
                        break;
                    }
                    (Some(_), _) => pattern.push(cursor.next().unwrap()),
                    (None, _) => return err(start, "expected => in match arm"),
                }
            }
            heads.push(stream(pattern));
            let body = match cursor.peek() {
                Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                    let g = g.clone();
                    cursor.next();
                    Cursor::of(&g).nodes(None)?
                }
                _ => vec![cursor.node()?],
            };
            bodies.push(body);
            if is_punct(cursor.peek(), ',') {
                cursor.next();
            }
        }
        Ok(Node::Block { head: heads, bodies, arms: true })
    }
}

/// Generated statements, with adjacent static markup merged into one `push_str`.
#[derive(Default)]
struct Gen {
    out: TokenStream,
    text: String,
    /// Type checks that print nothing (`ref={Rows}` is a ref of this island), emitted before the
    /// writes so they do not split runs of static markup.
    checks: TokenStream,
}

impl Gen {
    fn flush(&mut self) {
        if !self.text.is_empty() {
            let text = std::mem::take(&mut self.text);
            self.out.extend(q("__OUT.push_str(__A0);", &[literal(&text)]));
        }
    }

    fn code(&mut self, tokens: TokenStream) {
        self.flush();
        self.out.extend(tokens);
    }

    fn finish(mut self) -> TokenStream {
        self.flush();
        let mut out = self.checks;
        out.extend(self.out);
        out
    }

    /// Prints an island name (`ref={Rows}`, `on:click={ShowAll}`). A bare name is known now, so
    /// it is static markup, and its type is checked apart; anything else is written at run time.
    fn island_name(&mut self, expr: &TokenStream, check: &str, write: &str) {
        let mut tokens: Vec<TokenTree> = expr.clone().into_iter().collect();
        if let [TokenTree::Group(g)] = tokens.as_slice() {
            if g.delimiter() == Delimiter::Parenthesis {
                tokens = g.stream().into_iter().collect();
            }
        }
        match tokens.as_slice() {
            [TokenTree::Ident(name)] if name.to_string().starts_with(|c: char| c.is_ascii_uppercase()) => {
                self.text.push_str(&crate::island::camel(&name.to_string()));
                self.checks.extend(q(check, std::slice::from_ref(expr)));
            }
            _ => self.code(q(write, std::slice::from_ref(expr))),
        }
    }

    fn body(&self, nodes: &[Node], raw: bool) -> TokenTree {
        let mut gen = Gen::default();
        gen.nodes(nodes, raw);
        TokenTree::Group(Group::new(Delimiter::Brace, gen.finish()))
    }

    fn nodes(&mut self, nodes: &[Node], raw: bool) {
        for node in nodes {
            self.node(node, raw);
        }
    }

    fn node(&mut self, node: &Node, raw: bool) {
        match node {
            Node::Text(text) if raw => self.text.push_str(text),
            Node::Text(text) => self.text.push_str(&escape(text)),
            Node::Markup(markup) => self.text.push_str(markup),
            Node::Expr(expr) => {
                self.code(q("(&::corten::view::__private::Wrap(&(__A0))).__render(__OUT);", std::slice::from_ref(expr)))
            }
            Node::Let(tokens) => self.code(tokens.clone()),
            Node::Element { name, attrs, children } => {
                self.text.push('<');
                self.text.push_str(name);
                let mut actions_printed = false;
                for attr in attrs {
                    match attr {
                        Attr::Bare(attr) => {
                            self.text.push(' ');
                            self.text.push_str(attr);
                        }
                        Attr::Static(attr, value) => {
                            self.text.push_str(&format!(" {attr}=\"{}\"", escape(value)));
                        }
                        Attr::Ref(expr) => {
                            self.text.push_str(" data-ref=\"");
                            self.island_name(
                                expr,
                                "::corten::island::__private::check_ref(&__A0);",
                                "__OUT.push_str(::corten::island::__private::ref_name(&__A0));",
                            );
                            self.text.push('"');
                        }
                        Attr::On(..) if actions_printed => {}
                        Attr::On(..) => {
                            actions_printed = true;
                            self.text.push_str(" data-on=\"");
                            for (i, (event, expr)) in attrs
                                .iter()
                                .filter_map(|a| match a {
                                    Attr::On(event, expr) => Some((event, expr)),
                                    _ => None,
                                })
                                .enumerate()
                            {
                                if i > 0 {
                                    self.text.push(' ');
                                }
                                self.text.push_str(event);
                                self.text.push(':');
                                self.island_name(
                                    expr,
                                    "::corten::island::__private::check_action(&__A0);",
                                    "__OUT.push_str(::corten::island::__private::action_name(&__A0));",
                                );
                            }
                            self.text.push('"');
                        }
                        // Classes are a list (see `corten::style::ClassList`): `class={(Row, hot.then_some(Hot))}`.
                        Attr::Expr(attr, expr) if attr == "class" => {
                            self.code(q("::corten::style::__private::class_attr(&(__A0), __OUT);", std::slice::from_ref(expr)))
                        }
                        Attr::Expr(attr, expr) => self.code(q(
                            "(&::corten::view::__private::Attr(&(__A0))).__attr(__A1, __OUT);",
                            &[expr.clone(), literal(&format!(" {attr}"))],
                        )),
                        Attr::Spread(expr) => {
                            self.code(q("::corten::view::Attrs::render_attrs(&(__A0), __OUT);", std::slice::from_ref(expr)))
                        }
                    }
                }
                self.text.push('>');
                if !VOID.contains(&name.as_str()) {
                    self.nodes(children, RAW_TEXT.contains(&name.as_str()));
                    self.text.push_str(&format!("</{name}>"));
                }
            }
            Node::Component { path, props, children } => {
                let mut fields = TokenStream::new();
                for (field, value) in props {
                    fields.extend(q("__A0: __A1,", &[field.clone(), value.clone()]));
                }
                if let Some(children) = children {
                    let body = stream([self.body(children, false)]);
                    fields.extend(q("children: ::corten::view::Html::child(|__OUT: &mut ::std::string::String| __A0),", &[body]));
                }
                let literal = stream([TokenTree::Group(Group::new(Delimiter::Brace, fields))]);
                self.code(q("::corten::view::Render::render(&__A0 __A1, __OUT);", &[path.clone(), literal]));
            }
            Node::Block { head, bodies, arms } => {
                if *arms {
                    let mut inner = TokenStream::new();
                    for (pattern, body) in head[1..].iter().zip(bodies) {
                        inner.extend(pattern.clone());
                        inner.extend([
                            self.body(body, raw),
                            TokenTree::Punct(proc_macro::Punct::new(',', proc_macro::Spacing::Alone)),
                        ]);
                    }
                    let mut tokens = head[0].clone();
                    tokens.extend([TokenTree::Group(Group::new(Delimiter::Brace, inner))]);
                    self.code(tokens);
                } else {
                    let mut tokens = TokenStream::new();
                    for (h, body) in head.iter().zip(bodies) {
                        tokens.extend(h.clone());
                        tokens.extend([self.body(body, raw)]);
                    }
                    self.code(tokens);
                }
            }
        }
    }
}
