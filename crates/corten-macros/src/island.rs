//! `#[island]`: a component with behavior in a JavaScript file next to it.
//!
//! The component's markup names the elements and events its script uses: `ref={Rows}` and
//! `on:click={ShowAll}`. This macro collects those names from the component's own `html!`, turns
//! them into enums (so `html!` can only print names the island has), and describes the island in
//! the component's constant (`OfferList.island`): its tag, its script (`include_str!` of the `.js` file with
//! the component's snake_case name), and each name with the elements and events it is used on.
//! `corten::types` turns that description into the declarations `island("tag", ...)` is checked
//! against.

use proc_macro::{Delimiter, Ident, TokenStream, TokenTree};

use crate::tokens::{err, is_ident, is_punct, literal, q, Error};

/// A name the markup gives the script: a ref (`ref={Rows}`) or an action (`on:click={ShowAll}`).
pub struct Name {
    /// The Rust name, `ShowAll`, with the span of its first use.
    pub ident: Ident,
    /// The script's name, `showAll`.
    pub js: String,
    /// The elements it is used on (`button`), first use first.
    pub tags: Vec<String>,
    /// For actions, the events (`click`).
    pub events: Vec<String>,
}

#[derive(Default)]
pub struct Names {
    pub refs: Vec<Name>,
    pub actions: Vec<Name>,
}

/// Everything the component macro adds for an island.
pub struct Parts {
    /// Statements at the start of `render`: the `use` that puts the names in scope.
    pub scope: TokenStream,
    /// Printed before the body: the start tag and the props script.
    pub open: TokenStream,
    /// Printed after the body: the end tag.
    pub close: TokenStream,
    /// Items next to the component: the name enums.
    pub items: TokenStream,
    /// `::corten::island::Island { .. }`, for the component's `Component` constant.
    pub island: TokenStream,
    /// The Rust names of the refs and actions.
    pub names: Vec<String>,
}

/// A prop: a parameter marked `#[prop]`, sent to the script as JSON.
pub struct Prop {
    pub field: Ident,
    pub ty: Vec<TokenTree>,
}

pub fn parts(name: &Ident, body: &TokenStream, props: &[Prop]) -> Result<Parts, Error> {
    let component = name.to_string();
    let tag = kebab(&component);
    if !tag.contains('-') {
        return err(
            name.span(),
            format!("an island's name has two or more words, so its tag is a custom element name: {component}Island would be <{tag}-island>"),
        );
    }
    let mut names = Names::default();
    scan(body.clone(), false, &mut String::new(), &mut names)?;
    for action in &names.actions {
        if names.refs.iter().any(|r| r.js == action.js) {
            return err(action.ident.span(), format!("{} is both a ref and an action; give them different names", action.ident));
        }
    }
    let module = Ident::new(&format!("__corten_island_{component}"), name.span());
    let mut enums = TokenStream::new();
    let mut uses = TokenStream::new();
    for (kind, trait_name, list) in [("Ref", "IslandRef", &names.refs), ("Action", "IslandAction", &names.actions)] {
        if list.is_empty() {
            continue;
        }
        let mut variants = TokenStream::new();
        let mut arms = TokenStream::new();
        for n in list {
            variants.extend(q("__A0,", &[TokenTree::Ident(n.ident.clone()).into()]));
            arms.extend(q("Self::__A0 => __A1,", &[TokenTree::Ident(n.ident.clone()).into(), literal(&n.js)]));
        }
        let kind_ident: TokenStream = TokenTree::Ident(Ident::new(kind, name.span())).into();
        let trait_ident: TokenStream = TokenTree::Ident(Ident::new(trait_name, name.span())).into();
        enums.extend(q(
            "#[derive(Clone, Copy)] pub enum __A0 { __A1 }
            impl ::corten::island::__private::__A2 for __A0 { fn name(&self) -> &'static str { match self { __A3 } } }",
            &[kind_ident.clone(), variants, trait_ident, arms],
        ));
        uses.extend(q(
            // No `self::`: an island declared inside a function body (a test) has its module there.
            "#[allow(unused_imports)] use __A0::__A1::*;",
            &[TokenTree::Ident(module.clone()).into(), kind_ident],
        ));
    }
    let mut items = TokenStream::new();
    if !enums.is_empty() {
        items.extend(q(
            "#[doc(hidden)] #[allow(non_snake_case, dead_code)] mod __A0 { __A1 }",
            &[TokenTree::Ident(module).into(), enums],
        ));
    }

    let describe = |list: &[Name]| {
        let mut out = TokenStream::new();
        for n in list {
            out.extend(q(
                "::corten::island::Name { name: __A0, tags: &[__A1], events: &[__A2] },",
                &[literal(&n.js), strings(&n.tags), strings(&n.events)],
            ));
        }
        out
    };
    let mut prop_list = TokenStream::new();
    let mut prop_values = TokenStream::new();
    for prop in props {
        let key = prop.field.to_string();
        let key = key.strip_prefix("r#").unwrap_or(&key).to_string();
        prop_list.extend(q(
            "::corten::island::Prop { name: __A0, ts: ::corten::island::__private::ts::<__A1>() },",
            &[literal(&key), static_type(&prop.ty)],
        ));
        prop_values.extend(q(
            "(__A0, &self.__A1 as &dyn ::corten::island::__private::Json),",
            &[literal(&key), TokenTree::Ident(prop.field.clone()).into()],
        ));
    }
    let script = format!("{}.js", snake(&component));
    let island = q(
        "::corten::island::Island {
            tag: __A0,
            script: ::core::include_str!(__A1),
            file: __A1,
            refs: &[__A2],
            actions: &[__A3],
            props: &[__A4],
        }",
        &[literal(&tag), literal(&script), describe(&names.refs), describe(&names.actions), prop_list],
    );

    let mut open = q("__corten_component_out.push_str(__A0);", &[literal(&format!("<{tag}>"))]);
    if !props.is_empty() {
        open.extend(q("::corten::island::__private::props(__corten_component_out, &[__A0]);", &[prop_values]));
    }
    let close = q("__corten_component_out.push_str(__A0);", &[literal(&format!("</{tag}>"))]);
    let names = names.refs.iter().chain(&names.actions).map(|n| n.ident.to_string()).collect();
    Ok(Parts { scope: uses, open, close, items, island, names })
}

fn strings(list: &[String]) -> TokenStream {
    let mut out = TokenStream::new();
    for s in list {
        out.extend(literal(s));
        out.extend(q(",", &[]));
    }
    out
}

/// The prop's type with every lifetime `'static`, for the type description in a constant.
fn static_type(tokens: &[TokenTree]) -> TokenStream {
    let mut out: Vec<TokenTree> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            TokenTree::Punct(p) if p.as_char() == '\'' => {
                out.extend(q("'static", &[]));
                i += 2;
                continue;
            }
            TokenTree::Punct(p) if p.as_char() == '&' => {
                out.push(tokens[i].clone());
                if !is_punct(tokens.get(i + 1), '\'') {
                    out.extend(q("'static", &[]));
                }
            }
            TokenTree::Group(g) => {
                let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                out.push(TokenTree::Group(proc_macro::Group::new(g.delimiter(), static_type(&inner))));
            }
            other => out.push(other.clone()),
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// Finds `ref={Name}` and `on:event={Name}` in the `html!` calls in `tokens`. `tag` is the last
/// element opened, which the next ref or action is on.
fn scan(tokens: TokenStream, markup: bool, tag: &mut String, names: &mut Names) -> Result<(), Error> {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            TokenTree::Ident(id) if !markup && id.to_string() == "html" && is_punct(tokens.get(i + 1), '!') => {
                if let Some(TokenTree::Group(g)) = tokens.get(i + 2) {
                    scan(g.stream(), true, &mut String::new(), names)?;
                    i += 3;
                    continue;
                }
            }
            TokenTree::Group(g) => scan(g.stream(), markup, tag, names)?,
            TokenTree::Punct(p) if markup && p.as_char() == '<' => {
                if let Some(TokenTree::Ident(first)) = tokens.get(i + 1) {
                    let mut name = first.to_string();
                    let mut j = i + 2;
                    while is_punct(tokens.get(j), '-') {
                        match tokens.get(j + 1) {
                            Some(part @ (TokenTree::Ident(_) | TokenTree::Literal(_))) => {
                                name.push('-');
                                name.push_str(&part.to_string());
                                j += 2;
                            }
                            _ => break,
                        }
                    }
                    if is_punct(tokens.get(j), ':') && is_punct(tokens.get(j + 1), ':') {
                        name = "::".into();
                    }
                    *tag = name;
                }
            }
            TokenTree::Ident(id) if markup && id.to_string() == "ref" && is_punct(tokens.get(i + 1), '=') => {
                let ident = value(&tokens, i + 2, id, "ref={Rows}")?;
                element(tag, id, "ref")?;
                add(&mut names.refs, ident, tag, None)?;
                i += 3;
                continue;
            }
            TokenTree::Ident(id)
                if markup && id.to_string() == "on" && is_punct(tokens.get(i + 1), ':') && !is_punct(tokens.get(i + 2), ':') =>
            {
                let mut event = String::new();
                let mut j = i + 2;
                loop {
                    match tokens.get(j) {
                        Some(part @ (TokenTree::Ident(_) | TokenTree::Literal(_))) => event.push_str(&part.to_string()),
                        _ => return err(id.span(), "expected an event name after on:, like on:click={Save}"),
                    }
                    j += 1;
                    if is_punct(tokens.get(j), '-') {
                        event.push('-');
                        j += 1;
                    } else {
                        break;
                    }
                }
                if !is_punct(tokens.get(j), '=') {
                    return err(id.span(), format!("on:{event} needs an action: on:{event}={{Save}}"));
                }
                let ident = value(&tokens, j + 1, id, "on:click={Save}")?;
                element(tag, id, "on:")?;
                add(&mut names.actions, ident, tag, Some(event))?;
                i = j + 2;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    Ok(())
}

/// The name in `{Name}` at `tokens[at]`.
fn value(tokens: &[TokenTree], at: usize, attr: &Ident, example: &str) -> Result<Ident, Error> {
    if let Some(TokenTree::Group(g)) = tokens.get(at) {
        let inner: Vec<TokenTree> = g.stream().into_iter().collect();
        if let (Delimiter::Brace, [TokenTree::Ident(name)]) = (g.delimiter(), inner.as_slice()) {
            let text = name.to_string();
            let camel = text.starts_with(|c: char| c.is_ascii_uppercase()) && text.chars().all(|c| c.is_ascii_alphanumeric());
            if !camel {
                return err(name.span(), format!("{text}: island names are UpperCamelCase, like {example}"));
            }
            return Ok(name.clone());
        }
    }
    err(attr.span(), format!("this takes a name the island's script uses, in braces: {example}"))
}

fn element(tag: &str, attr: &Ident, what: &str) -> Result<(), Error> {
    if tag.is_empty() || tag == "::" || tag.starts_with(|c: char| c.is_ascii_uppercase()) {
        return err(attr.span(), format!("{what} goes on an HTML element, not on a component"));
    }
    Ok(())
}

fn add(list: &mut Vec<Name>, ident: Ident, tag: &str, event: Option<String>) -> Result<(), Error> {
    let js = camel(&ident.to_string());
    if matches!(js.as_str(), "constructor" | "prototype" | "toString" | "valueOf" | "hasOwnProperty") {
        return err(ident.span(), format!("{ident} is a name every JavaScript object has; pick another"));
    }
    let entry = match list.iter_mut().position(|n| n.js == js) {
        Some(i) => &mut list[i],
        None => {
            list.push(Name { ident, js, tags: Vec::new(), events: Vec::new() });
            list.last_mut().expect("just pushed")
        }
    };
    if !entry.tags.iter().any(|t| t == tag) {
        entry.tags.push(tag.to_string());
    }
    if let Some(event) = event {
        if !entry.events.contains(&event) {
            entry.events.push(event);
        }
    }
    Ok(())
}

/// `OfferList` to `offer-list`.
pub fn kebab(name: &str) -> String {
    words(name).join("-")
}

/// `OfferList` to `offer_list`.
pub fn snake(name: &str) -> String {
    words(name).join("_")
}

/// `ShowAll` to `showAll`, `URLField` to `urlField`.
pub fn camel(name: &str) -> String {
    let mut out = String::new();
    for (i, word) in words(name).iter().enumerate() {
        if i == 0 {
            out.push_str(word);
        } else {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                out.push(first.to_ascii_uppercase());
                out.push_str(chars.as_str());
            }
        }
    }
    out
}

/// Lowercase words of an UpperCamelCase name; a run of capitals is one word (`URLField` is
/// `url`, `field`).
fn words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut words: Vec<String> = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        let starts = i == 0
            || (c.is_ascii_uppercase()
                && (chars[i - 1].is_ascii_lowercase()
                    || chars[i - 1].is_ascii_digit()
                    || chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase())));
        if starts {
            words.push(String::new());
        }
        words.last_mut().expect("a word started").push(c.to_ascii_lowercase());
    }
    words
}

/// True for the parameter attribute `#[prop]`.
pub fn is_prop(attr: &[TokenTree]) -> bool {
    matches!(attr, [TokenTree::Punct(p), TokenTree::Group(g)] if p.as_char() == '#' && {
        let inner: Vec<TokenTree> = g.stream().into_iter().collect();
        inner.len() == 1 && is_ident(inner.first(), "prop")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_convert_between_rust_and_html_and_javascript() {
        assert_eq!(kebab("OfferList"), "offer-list");
        assert_eq!(snake("OfferList"), "offer_list");
        assert_eq!(kebab("Counter"), "counter");
        assert_eq!(camel("ShowAll"), "showAll");
        assert_eq!(camel("Rows"), "rows");
        assert_eq!(camel("URLField"), "urlField");
        assert_eq!(camel("URL"), "url");
        assert_eq!(kebab("Grid2Rows"), "grid2-rows");
    }
}
