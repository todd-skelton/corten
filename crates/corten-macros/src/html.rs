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
    let (scope, input) = scope_marker(input);
    match Cursor::new(input).nodes(None).and_then(|mut nodes| {
        if let Some(scope) = &scope {
            mark_scope(&mut nodes, scope)?;
        }
        Ok(nodes)
    }) {
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

/// `#[component]` starts the markup of a component with refs with `@scope "name";`.
fn scope_marker(input: TokenStream) -> (Option<String>, TokenStream) {
    let tokens: Vec<TokenTree> = input.clone().into_iter().collect();
    if let (true, true, Some(TokenTree::Literal(name)), true) =
        (is_punct(tokens.first(), '@'), is_ident(tokens.get(1), "scope"), tokens.get(2), is_punct(tokens.get(3), ';'))
    {
        if let Some(name) = string_value(name) {
            return (Some(name), stream(tokens.into_iter().skip(4)));
        }
    }
    (None, input)
}

/// Puts `data-scope="name"` on the one root element.
fn mark_scope(nodes: &mut [Node], scope: &str) -> Result<(), Error> {
    let mut roots = nodes.iter_mut().filter(|n| !matches!(n, Node::Let(_)));
    match (roots.next(), roots.next()) {
        (Some(Node::Element { attrs, .. }), None) => {
            attrs.insert(0, Attr::Static("data-scope".into(), scope.into()));
            Ok(())
        }
        _ => err(
            Span::call_site(),
            "a component with refs renders one root element, which holds its refs: wrap its markup in one element",
        ),
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
    /// `ref={rows}`: prints `data-ref="rows"`, and types the ref with the element's interface, or
    /// with a wider one (`ref={trigger as HTMLElement}`) when the ref is on several kinds.
    Ref(TokenStream, Option<Ident>),
    /// `on:click={js::show(rows)}`: the event (with its span) and the handler. All of an
    /// element's print as one `data-on="click:module.show(@rows)"`.
    On(String, Span, TokenStream),
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
                Some(TokenTree::Ident(id))
                    if component
                        && id.to_string() == "on"
                        && is_punct(self.peek2(), ':')
                        && !matches!(self.tokens.get(self.i + 2), Some(TokenTree::Punct(_))) =>
                {
                    self.next();
                    self.next();
                    let span = self.span();
                    let Some(TokenTree::Ident(event)) = self.next() else {
                        return err(span, "on: on a component names one of its Handler props: on:remove={..} sets on_remove");
                    };
                    if !is_punct(self.peek(), '=') {
                        return err(span, format!("on:{event} takes a handler: on:{event}={{js::name(..)}}"));
                    }
                    self.next();
                    let (_, expr) = self.value(true)?;
                    let field = Ident::new(&format!("on_{event}"), event.span());
                    let expr =
                        keywords(expr, &q("::corten::script::ev::Event", &[]), &q("::corten::script::el::HTMLElement", &[]));
                    props.push((stream([TokenTree::Ident(field)]), q("&(__A0)", &[expr])));
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
                    let mut name = self.attr_name()?;
                    // `key={item.id}` names an element in a keyed ref (a handler's `Map`).
                    if name == "key" {
                        name = "data-key".into();
                    }
                    if name == "data-ref" || name == "data-on" || name == "data-scope" {
                        return err(span, "data-ref, data-on and data-scope are printed by Corten: write ref={rows} (with let (rows) = refs();) or on:click={js::show(rows)}");
                    }
                    if name == "ref" || name.starts_with("on:") {
                        let braced = is_punct(self.peek(), '=')
                            && matches!(self.peek2(), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace);
                        if !braced {
                            return err(
                                span,
                                format!("{name} takes a value in braces: ref={{rows}}, on:click={{js::show(rows)}}"),
                            );
                        }
                        self.next();
                        let (_, expr) = self.value(false)?;
                        attrs.push(match name.strip_prefix("on:") {
                            Some(event) => Attr::On(event.to_string(), span, expr),
                            None => {
                                // `value` wraps the braces' contents in parentheses.
                                let mut tokens: Vec<TokenTree> = expr.clone().into_iter().collect();
                                if let [TokenTree::Group(g)] = tokens.as_slice() {
                                    if g.delimiter() == Delimiter::Parenthesis {
                                        tokens = g.stream().into_iter().collect();
                                    }
                                }
                                let n = tokens.len();
                                if n >= 3 && is_ident(tokens.get(n - 2), "as") {
                                    let TokenTree::Ident(wide) = &tokens[n - 1] else {
                                        return err(tokens[n - 1].span(), "ref={trigger as HTMLElement} names a DOM interface");
                                    };
                                    Attr::Ref(stream(tokens[..n - 2].iter().cloned()), Some(wide.clone()))
                                } else {
                                    Attr::Ref(expr, None)
                                }
                            }
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
    /// Type checks that print nothing, emitted before the writes so they do not split runs of
    /// static markup.
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
                        Attr::Ref(expr, wide) => {
                            let el: TokenStream =
                                format!("::corten::script::el::{}", crate::dom::element(name)).parse().expect("a path parses");
                            match wide {
                                // `ref={trigger as HTMLElement}`: a ref on several kinds of element, typed as one they all are.
                                Some(wide) => {
                                    let ty: TokenStream = format!("::corten::script::el::{wide}").parse().expect("a path parses");
                                    let ty: TokenStream = ty
                                        .into_iter()
                                        .map(|mut t| {
                                            t.set_span(wide.span());
                                            t
                                        })
                                        .collect();
                                    self.code(q(
                                        "::corten::script::__private::ref_attr_as::<__A1, __A2>(&(__A0), __OUT);",
                                        &[expr.clone(), el, ty],
                                    ))
                                }
                                None => self.code(q(
                                    "::corten::script::__private::ref_attr::<__A1>(&(__A0), __OUT);",
                                    &[expr.clone(), el],
                                )),
                            }
                        }
                        Attr::On(..) if actions_printed => {}
                        Attr::On(..) => {
                            actions_printed = true;
                            let target: TokenStream =
                                format!("::corten::script::el::{}", crate::dom::element(name)).parse().expect("a path parses");
                            let mut binds = TokenStream::new();
                            for attr in attrs {
                                let Attr::On(event, span, expr) = attr else { continue };
                                let (printed, ty) = if event.starts_with(|c: char| c.is_ascii_uppercase()) {
                                    let ty: TokenStream = TokenTree::Ident(Ident::new(event, *span)).into();
                                    self.checks
                                        .extend(q("::corten::script::__private::is_event::<__A0>();", std::slice::from_ref(&ty)));
                                    (crate::names::kebab(event), ty)
                                } else {
                                    let ty: TokenStream = format!("::corten::script::ev::{}", crate::dom::event(event))
                                        .parse()
                                        .expect("a path parses");
                                    (event.clone(), ty)
                                };
                                let expr = keywords(expr.clone(), &ty, &target);
                                binds.extend(q("(__A0, &(__A1) as &dyn ::corten::script::Bind),", &[literal(&printed), expr]));
                            }
                            self.code(q("::corten::script::__private::data_on(__OUT, &[__A0]);", &[binds]));
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

/// `event`, `target`, `signal` and `scope` in a handler call are the browser's: typed as the
/// event `event_ty` and the element `target_ty`. A field (`x.event`), a path segment, a struct
/// field name (`event: ..`), a call (`event()`) or a macro keeps its own meaning.
fn keywords(tokens: TokenStream, event_ty: &TokenStream, target_ty: &TokenStream) -> TokenStream {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    let mut out = TokenStream::new();
    for (i, token) in tokens.iter().enumerate() {
        match token {
            TokenTree::Ident(id) if matches!(id.to_string().as_str(), "event" | "target" | "signal" | "scope") => {
                let after_dot = i > 0 && (is_punct(tokens.get(i - 1), '.') || is_punct(tokens.get(i - 1), ':'));
                let next = tokens.get(i + 1);
                let used_otherwise = is_punct(next, ':')
                    || is_punct(next, '!')
                    || matches!(next, Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis);
                if after_dot || used_otherwise {
                    out.extend([token.clone()]);
                    continue;
                }
                let replaced = match id.to_string().as_str() {
                    "event" => q("::corten::script::__private::EventKw::<__A0>::NEW", std::slice::from_ref(event_ty)),
                    "target" => q("::corten::script::__private::TargetKw::<__A0>::NEW", std::slice::from_ref(target_ty)),
                    "signal" => q("::corten::script::__private::SignalKw", &[]),
                    _ => q("::corten::script::__private::ScopeKw", &[]),
                };
                // Errors about the keyword point at it.
                out.extend(replaced.into_iter().map(|mut t| {
                    t.set_span(id.span());
                    t
                }));
            }
            TokenTree::Group(g) => {
                let mut copy = Group::new(g.delimiter(), keywords(g.stream(), event_ty, target_ty));
                copy.set_span(g.span());
                out.extend([TokenTree::Group(copy)]);
            }
            other => out.extend([other.clone()]),
        }
    }
    out
}
