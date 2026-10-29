//! Reading the item a derive is on, with `proc_macro` alone.

use proc_macro::{Delimiter, Group, Ident, Literal, Span, TokenStream, TokenTree};

use crate::{
    component::{split, split_generics},
    tokens::{err, is_ident, is_punct, string_value, Error},
};

/// A struct or enum: its attributes, name, generics and body.
pub struct Item {
    /// The inside of each `#[...]`.
    pub attrs: Vec<Group>,
    /// Its visibility, as written (`pub`, `pub(crate)`, or nothing).
    pub vis: TokenStream,
    pub is_enum: bool,
    pub name: Ident,
    /// The impl's generics (without defaults) and the type's arguments; empty when none.
    pub impl_generics: TokenStream,
    pub type_generics: TokenStream,
    pub where_clause: TokenStream,
    /// The braces of a struct with named fields or of an enum; `None` for a unit struct.
    pub body: Option<Group>,
    /// The fields of a tuple struct, for derives that accept one.
    pub tuple: Option<Group>,
}

pub fn parse(input: TokenStream, derive: &str) -> Result<Item, Error> {
    parse_item(input, derive, false)
}

/// [`parse`], also accepting a tuple struct (its fields in [`Item::tuple`]).
pub fn parse_item(input: TokenStream, derive: &str, tuple_ok: bool) -> Result<Item, Error> {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut attrs = Vec::new();
    let mut vis = TokenStream::new();
    let mut i = 0;
    let is_enum = loop {
        match tokens.get(i) {
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                if let Some(TokenTree::Group(g)) = tokens.get(i + 1) {
                    attrs.push(g.clone());
                }
                i += 2;
            }
            Some(TokenTree::Ident(id)) if id.to_string() == "struct" => break false,
            Some(TokenTree::Ident(id)) if id.to_string() == "enum" => break true,
            Some(TokenTree::Ident(id)) if id.to_string() == "union" => {
                return err(id.span(), format!("{derive} cannot be derived for a union"))
            }
            Some(token) => {
                vis.extend([token.clone()]);
                i += 1;
            }
            None => return err(Span::call_site(), format!("{derive}: expected a struct or an enum")),
        }
    };
    i += 1;
    let name = match tokens.get(i) {
        Some(TokenTree::Ident(name)) => name.clone(),
        _ => return err(Span::call_site(), format!("{derive}: expected a name")),
    };
    i += 1;
    let mut generics = Vec::new();
    if is_punct(tokens.get(i), '<') {
        let mut depth = 0;
        // The `>` of an arrow in a bound (`F: Fn() -> u8`) closes nothing.
        let mut arrow = false;
        loop {
            let Some(token) = tokens.get(i) else { return err(name.span(), "unclosed generics") };
            i += 1;
            if let TokenTree::Punct(p) = token {
                match p.as_char() {
                    '<' => depth += 1,
                    '>' if !arrow => depth -= 1,
                    _ => {}
                }
                arrow = p.as_char() == '-' && p.spacing() == proc_macro::Spacing::Joint;
            } else {
                arrow = false;
            }
            generics.push(token.clone());
            if depth == 0 {
                break;
            }
        }
    }
    let (impl_generics, type_generics) = split_generics(&generics);
    let mut where_clause = TokenStream::new();
    let mut body = None;
    let mut tuple = None;
    while let Some(token) = tokens.get(i) {
        match token {
            TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => {
                body = Some(g.clone());
                break;
            }
            TokenTree::Group(g) if g.delimiter() == Delimiter::Parenthesis && where_clause.is_empty() => {
                if !tuple_ok {
                    return err(g.span(), format!("{derive} needs named fields (`struct X {{ a: T }}`) or a unit struct"));
                }
                tuple = Some(g.clone());
                i += 1;
                continue;
            }
            TokenTree::Punct(p) if p.as_char() == ';' => break,
            other => where_clause.extend([other.clone()]),
        }
        i += 1;
    }
    Ok(Item { attrs, vis, is_enum, name, impl_generics, type_generics, where_clause, body, tuple })
}

impl Item {
    /// The string in `#[name("...")]`, with the literal's span.
    pub fn attr_string(&self, name: &str) -> Result<Option<(String, Span)>, Error> {
        for attr in &self.attrs {
            let tokens: Vec<TokenTree> = attr.stream().into_iter().collect();
            if !is_ident(tokens.first(), name) {
                continue;
            }
            let literal = match tokens.get(1) {
                Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
                    let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                    match inner.as_slice() {
                        [TokenTree::Literal(l)] => l.clone(),
                        _ => return err(g.span(), format!("expected #[{name}(\"...\")]")),
                    }
                }
                _ => return err(attr.span(), format!("expected #[{name}(\"...\")]")),
            };
            return match string_value(&literal) {
                Some(value) => Ok(Some((value, literal.span()))),
                None => err(literal.span(), "expected a string"),
            };
        }
        Ok(None)
    }

    /// `impl<..> Trait for Name<..> where ..` followed by `body` in braces.
    pub fn impl_block(&self, trait_path: &str, body: TokenStream) -> TokenStream {
        let mut out: TokenStream = "#[automatically_derived] impl".parse().expect("parses");
        out.extend(self.impl_generics.clone());
        out.extend(trait_path.parse::<TokenStream>().expect("trait path parses"));
        out.extend("for".parse::<TokenStream>().expect("parses"));
        out.extend([TokenTree::Ident(self.name.clone())]);
        out.extend(self.type_generics.clone());
        out.extend(self.where_clause.clone());
        out.extend([TokenTree::Group(Group::new(Delimiter::Brace, body))]);
        out
    }
}

/// A named field: its name (without `r#`), the identifier to access it, and its span.
pub struct Field {
    pub name: String,
    pub ident: Ident,
    /// The field's attributes, `#[..]`'s brackets.
    pub attrs: Vec<Group>,
    /// Its name in a URL's query: `name`, or the name serde reads it by (`Route` sets it).
    pub key: String,
    /// Its type, as written.
    pub ty: Vec<TokenTree>,
    /// Its visibility, as written (`pub`, `pub(crate)`, or nothing).
    pub vis: Vec<TokenTree>,
}

pub fn named_fields(body: &Group) -> Result<Vec<Field>, Error> {
    let mut fields = Vec::new();
    for field in split(body.stream(), ',') {
        let mut j = 0;
        let mut attrs = Vec::new();
        let mut vis = Vec::new();
        loop {
            match field.get(j) {
                Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                    if let Some(TokenTree::Group(g)) = field.get(j + 1) {
                        attrs.push(g.clone());
                    }
                    j += 2;
                }
                Some(TokenTree::Ident(id)) if id.to_string() == "pub" => {
                    vis.push(field[j].clone());
                    j += 1;
                    if matches!(field.get(j), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis) {
                        vis.push(field[j].clone());
                        j += 1;
                    }
                }
                Some(TokenTree::Ident(id)) if is_punct(field.get(j + 1), ':') => {
                    let text = id.to_string();
                    let name = text.strip_prefix("r#").unwrap_or(&text).to_string();
                    let ty = field.get(j + 2..).map(<[TokenTree]>::to_vec).unwrap_or_default();
                    fields.push(Field { key: name.clone(), name, ident: id.clone(), attrs, ty, vis });
                    break;
                }
                Some(other) => return err(other.span(), "expected a named field"),
                None => break,
            }
        }
    }
    Ok(fields)
}

pub fn string(text: &str) -> TokenTree {
    TokenTree::Literal(Literal::string(text))
}
