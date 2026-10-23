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

/// Puts `data-scope="name"` on the one root element, and `data-events` with the types of the
/// `on:` handlers in the component's markup, so a script started on the root listens for exactly
/// those without searching the markup.
fn mark_scope(nodes: &mut [Node], scope: &str) -> Result<(), Error> {
    let mut types = Vec::new();
    event_types(nodes, &mut types);
    let mut roots = nodes.iter_mut().filter(|n| !matches!(n, Node::Let(_)));
    match (roots.next(), roots.next()) {
        (Some(Node::Element { attrs, .. }), None) => {
            attrs.insert(0, Attr::Static("data-scope".into(), scope.into()));
            if !types.is_empty() {
                attrs.insert(1, Attr::Static("data-events".into(), types.join(" ")));
            }
            Ok(())
        }
        _ => err(
            Span::call_site(),
            "a component with refs renders one root element, which holds its refs: wrap its markup in one element",
        ),
    }
}

/// The event types of the `on:` attributes in `nodes` (and in markup they pass to components as
/// children), in first-use order. `on:connect` is not an event a listener gets. A handler a
/// component passes to a child as a prop (`<Confirm on:confirm={..}>`) is put on an element by
/// the child, which picks its type, so it is not here.
fn event_types(nodes: &[Node], out: &mut Vec<String>) {
    for node in nodes {
        match node {
            Node::Element { attrs, children, .. } => {
                for attr in attrs {
                    if let Attr::On(event, ..) = attr {
                        let ty = if event.starts_with(|c: char| c.is_ascii_uppercase()) {
                            crate::names::kebab(event)
                        } else {
                            event.clone()
                        };
                        if ty != "connect" && !out.contains(&ty) {
                            out.push(ty);
                        }
                    }
                }
                event_types(children, out);
            }
            Node::Component { children: Some(children), .. } => event_types(children, out),
            Node::Block { bodies, .. } => bodies.iter().for_each(|body| event_types(body, out)),
            _ => {}
        }
    }
}

/// How a view property reads back from the page, from the parameter's Rust type.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Number,
    Text,
    Bool,
}

impl Kind {
    /// `u32`, `f64`, `&'a str`, `String`, `Cow<'a, str>` or `bool`; other types are not a
    /// view's (a script could not read them back from the page as the same value).
    fn of(ty: &[TokenTree]) -> Option<Kind> {
        let mut i = 0;
        loop {
            match ty.get(i) {
                Some(TokenTree::Punct(p)) if p.as_char() == '&' => i += 1,
                Some(TokenTree::Punct(p)) if p.as_char() == '\'' => i += 2,
                Some(TokenTree::Ident(id)) if id.to_string() == "mut" => i += 1,
                _ => break,
            }
        }
        let words: Vec<String> = ty[i..].iter().map(ToString::to_string).collect();
        match words.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
            ["u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize" | "f32" | "f64"] => {
                Some(Kind::Number)
            }
            ["str" | "String"] => Some(Kind::Text),
            ["bool"] => Some(Kind::Bool),
            ["Cow", "<", .., "str", ">"] => Some(Kind::Text),
            _ => None,
        }
    }

    fn ts(self) -> &'static str {
        match self {
            Kind::Number => "number",
            Kind::Text => "string",
            Kind::Bool => "boolean",
        }
    }
}

/// Where a parameter is shown: an element's whole text, or one attribute's whole value. `path` is
/// the element's place under the root, as child-element indexes.
struct Hole {
    param: usize,
    path: Vec<usize>,
    attr: Option<String>,
}

/// The view class `#[component(view)]` generates for the component `class`, from the `html!` its
/// `body` ends with: the JavaScript (`export class Row { .. }`) and its TypeScript declaration.
///
/// A parameter is a property when every use of it in the markup is a hole: `{label}` as the only
/// child of an element, or an attribute's whole value (`data-selected={selected}`), on elements
/// whose place is fixed (no loop, condition, expression or component before them among their
/// siblings). The property's getter reads its first hole and its setter writes all of them. A root
/// with `key={id}` keys the view, for `rows()`.
pub fn view_of(body: &Group, class: &str, params: &[(String, Vec<TokenTree>)], span: Span) -> Result<(String, String), Error> {
    let tokens: Vec<TokenTree> = body.stream().into_iter().collect();
    let n = tokens.len();
    let markup = match tokens.last() {
        Some(TokenTree::Group(g))
            if n >= 3
                && g.delimiter() == Delimiter::Brace
                && is_ident(tokens.get(n - 3), "html")
                && is_punct(tokens.get(n - 2), '!') =>
        {
            g.clone()
        }
        _ => {
            return err(
                body.span(),
                "a view's component ends with html! { <root> .. </root> }: its holes are found in that markup",
            )
        }
    };
    let nodes = Cursor::of(&markup).nodes(None)?;
    let mut roots = nodes.iter().filter(|n| !matches!(n, Node::Let(_)));
    let root = match (roots.next(), roots.next()) {
        (Some(root @ Node::Element { .. }), None) => root,
        _ => return err(markup.span(), "a view renders one root element: wrap its markup in one element"),
    };
    let names: Vec<&str> = params.iter().map(|(name, _)| name.as_str()).collect();
    let mut holes = Vec::new();
    find_holes(root, &mut Vec::new(), &names, &mut holes);
    let mut uses = vec![0; params.len()];
    count_uses(&nodes, &names, &mut uses);
    // The properties: parameters shown only in holes, of a type the page can hold.
    let mut props: Vec<(usize, Kind)> = Vec::new();
    for (i, (_, ty)) in params.iter().enumerate() {
        let shown: Vec<&Hole> = holes.iter().filter(|h| h.param == i).collect();
        let Some(kind) = Kind::of(ty) else { continue };
        let fits = shown.iter().all(|h| h.attr.is_some() || kind != Kind::Bool);
        if !shown.is_empty() && shown.len() == uses[i] && fits {
            props.push((i, kind));
        }
    }
    if let Some(&(i, _)) = props.iter().find(|&&(i, _)| matches!(params[i].0.as_str(), "el" | "constructor")) {
        return err(
            span,
            format!("a view's property cannot be named {}: its root element is el, and a class has a constructor", params[i].0),
        );
    }
    if props.is_empty() {
        return err(
            span,
            "a view needs a hole a script can write: a parameter that is the whole text of an element (<a>{label}</a>) or an attribute's whole value (data-selected={selected})",
        );
    }
    let Node::Element { attrs: root_attrs, .. } = root else { unreachable!() };
    let key = root_attrs.iter().find_map(|a| match a {
        Attr::Expr(name, expr) if name == "data-key" => Some(param_of(expr, &names)),
        _ => None,
    });
    let key = match key {
        None => None,
        Some(Some(i)) if props.iter().any(|(p, _)| *p == i) => Some(i),
        Some(_) => {
            return err(
                span,
                "a view's key={..} is one of its parameters, and shown only where a script can write it (an element's whole text or an attribute's whole value)",
            )
        }
    };

    let access = |base: &str, path: &[usize]| {
        let mut s = base.to_string();
        for &k in path {
            s.push_str(".firstElementChild");
            for _ in 0..k {
                s.push_str(".nextElementSibling");
            }
        }
        s
    };
    let mut js = format!("export class {class} {{\n  constructor(el) {{ this.el = el; }}\n");
    let mut ts = format!(
        "  /** The view of #[component(view)] {class}: its holes, read from and written to the page. */\n  export class {class} {{\n    constructor(el: Element);\n    readonly el: Element;\n"
    );
    let mut data = Vec::new();
    // `fill` writes the holes itself: going through a view and its setters cost about 1.5 µs a
    // row when making 10,000 rows.
    let mut fill = Vec::new();
    for &(i, kind) in &props {
        let name = &params[i].0;
        let shown: Vec<&Hole> = holes.iter().filter(|h| h.param == i).collect();
        let first = shown[0];
        let at = access("this.el", &first.path);
        let get = match (&first.attr, kind) {
            (None, Kind::Number) => format!("Number({at}.textContent)"),
            (None, _) => format!("{at}.textContent"),
            (Some(attr), Kind::Number) => format!("Number({at}.getAttribute({attr:?}))"),
            (Some(attr), Kind::Text) => format!("{at}.getAttribute({attr:?}) ?? \"\""),
            (Some(attr), Kind::Bool) => format!("{at}.hasAttribute({attr:?})"),
        };
        // Writes every hole of the element `base` from `value`.
        let write = |base: &str, value: &str| -> Vec<String> {
            shown
                .iter()
                .map(|h| {
                    let at = access(base, &h.path);
                    match (&h.attr, kind) {
                        (None, _) => format!("text({at}, {value});"),
                        (Some(attr), Kind::Bool) => format!("{at}.toggleAttribute({attr:?}, {value});"),
                        (Some(attr), _) => format!("{at}.setAttribute({attr:?}, {value});"),
                    }
                })
                .collect()
        };
        let (set, filled) = if kind == Kind::Number {
            // A numbered local: a parameter may be named like anything in this function.
            let local = format!("v{i}");
            let mut filled = vec![format!("const {local} = String(data.{name});")];
            filled.extend(write("el", &local));
            (write("this.el", "String(v)"), filled)
        } else {
            (write("this.el", "v"), write("el", &format!("data.{name}")))
        };
        fill.extend(filled);
        js.push_str(&format!("  get {name}() {{ return {get}; }}\n  set {name}(v) {{ {} }}\n", set.join(" ")));
        ts.push_str(&format!("    {name}: {};\n", kind.ts()));
        data.push(format!("{name}: {}", kind.ts()));
    }
    js.push_str(&format!("  static fill(el, data) {{ {} }}\n", fill.join(" ")));
    let data = format!("{{ {} }}", data.join("; "));
    ts.push_str(&format!("    /** Writes every hole of `el` (a {class}'s root) from `data`. */\n    static fill(el: Element, data: {data}): void;\n"));
    if let Some(i) = key {
        let key = &params[i].0;
        js.push_str(&format!(
            "  static key(el) {{ return el.getAttribute(\"data-key\") ?? \"\"; }}\n  static keyOf(data) {{ return String(data.{key}); }}\n"
        ));
        ts.push_str(&format!(
            "    /** The key of a {class}'s root (its key={{{key}}}). */\n    static key(el: Element): string;\n    static keyOf(data: {data}): string;\n"
        ));
    }
    js.push_str("}\n");
    ts.push_str("  }\n");
    Ok((js, ts))
}

/// The holes under `node` (an element at `path`): its attributes whose value is a parameter, its
/// text when that is a parameter alone, and those of its child elements whose place is fixed.
fn find_holes(node: &Node, path: &mut Vec<usize>, names: &[&str], holes: &mut Vec<Hole>) {
    let Node::Element { name, attrs, children } = node else { return };
    for attr in attrs {
        if let Attr::Expr(attr, expr) = attr {
            if let (false, Some(param)) = (attr == "class", param_of(expr, names)) {
                holes.push(Hole { param, path: path.clone(), attr: Some(attr.clone()) });
            }
        }
    }
    let content: Vec<&Node> = children.iter().filter(|c| !matches!(c, Node::Let(_))).collect();
    if let ([Node::Expr(expr)], false) = (content.as_slice(), VOID.contains(&name.as_str()) || RAW_TEXT.contains(&name.as_str()))
    {
        if let Some(param) = param_of(expr, names) {
            holes.push(Hole { param, path: path.clone(), attr: None });
        }
    }
    let mut index = 0;
    for child in children {
        match child {
            Node::Element { .. } => {
                path.push(index);
                find_holes(child, path, names, holes);
                path.pop();
                index += 1;
            }
            Node::Text(_) | Node::Let(_) => {}
            // Anything else may print elements, so the ones after it have no fixed place.
            _ => break,
        }
    }
}

/// The parameter an expression is, when it is one alone (`label`, or `(label)` as an attribute
/// value is wrapped).
fn param_of(expr: &TokenStream, names: &[&str]) -> Option<usize> {
    let mut tokens: Vec<TokenTree> = expr.clone().into_iter().collect();
    if let [TokenTree::Group(g)] = tokens.as_slice() {
        if g.delimiter() == Delimiter::Parenthesis {
            tokens = g.stream().into_iter().collect();
        }
    }
    match tokens.as_slice() {
        [TokenTree::Ident(id)] => names.iter().position(|n| id.to_string() == *n),
        _ => None,
    }
}

/// Counts every mention of each parameter in the markup's Rust code (a field `x.label` is not one).
fn count_uses(nodes: &[Node], names: &[&str], uses: &mut [usize]) {
    fn count(tokens: &TokenStream, names: &[&str], uses: &mut [usize]) {
        let tokens: Vec<TokenTree> = tokens.clone().into_iter().collect();
        for (i, token) in tokens.iter().enumerate() {
            match token {
                TokenTree::Ident(id) if !(i > 0 && is_punct(tokens.get(i - 1), '.')) => {
                    if let Some(p) = names.iter().position(|n| id.to_string() == *n) {
                        uses[p] += 1;
                    }
                }
                TokenTree::Group(g) => count(&g.stream(), names, uses),
                _ => {}
            }
        }
    }
    for node in nodes {
        match node {
            Node::Element { attrs, children, .. } => {
                for attr in attrs {
                    match attr {
                        Attr::Expr(_, expr) | Attr::Spread(expr) | Attr::Ref(expr, _) | Attr::On(_, _, expr) => {
                            count(expr, names, uses)
                        }
                        Attr::Bare(_) | Attr::Static(..) => {}
                    }
                }
                count_uses(children, names, uses);
            }
            Node::Component { props, children, .. } => {
                for (_, value) in props {
                    count(value, names, uses);
                }
                if let Some(children) = children {
                    count_uses(children, names, uses);
                }
            }
            Node::Expr(tokens) | Node::Let(tokens) => count(tokens, names, uses),
            Node::Block { head, bodies, .. } => {
                for h in head {
                    count(h, names, uses);
                }
                for body in bodies {
                    count_uses(body, names, uses);
                }
            }
            Node::Text(_) | Node::Markup(_) => {}
        }
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
                    if name == "data-ref" || name == "data-on" || name == "data-scope" || name == "data-events" {
                        return err(span, "data-ref, data-on, data-scope and data-events are printed by Corten: write ref={rows} (with let (rows) = refs();) or on:click={js::show(rows)}");
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
