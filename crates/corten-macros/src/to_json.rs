//! `#[derive(ToJson)]`: a struct or an enum written as JSON, laid out when the crate compiles.
//!
//! The keys, quotes, colons and commas between two values are merged into one byte string, so the
//! generated code copies a literal, writes a value, copies the next literal: `{"id":"`, the id,
//! `","count":`, the count, `}`. A string field's quotes go in the literals around it. Commas are
//! decided here too, except after a field that may be left out (`skip_serializing_if`) before any
//! that is always written, where a flag decides them.
//!
//! `#[serde(...)]` is read so the JSON is what serde writes; an attribute that changes serde's
//! output in a way this does not follow is a compile error.

use proc_macro::{Delimiter, Group, Ident, Literal, Span, TokenStream, TokenTree};

use crate::{
    component::split,
    derive::parse_item,
    tokens::{err, is_ident, is_punct, parens, q, string_value, Error},
};

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let item = parse_item(input, "ToJson", true)?;
    let container = Container::read(&item.attrs, item.is_enum)?;
    let body = if item.is_enum {
        let Some(body) = &item.body else { return err(item.name.span(), "ToJson: expected the enum's variants") };
        enumeration(&container, body)?
    } else if let Some(fields) = &item.tuple {
        let fields = parse_fields(fields, false)?;
        if container.transparent {
            transparent(&fields, item.name.span())?
        } else {
            let mut gen = Gen::default();
            tuple(&mut gen, &fields, self_field)?;
            gen.finish()
        }
    } else if let Some(fields) = &item.body {
        let fields = parse_fields(fields, true)?;
        if container.transparent {
            transparent(&fields, item.name.span())?
        } else {
            let mut gen = Gen::default();
            object(&mut gen, b"{", Comma::First, &fields, container.rename_all.as_deref(), self_field);
            gen.finish()
        }
    } else {
        q("__OUT.extend_from_slice(b\"null\");", &[])
    };

    // Each type parameter must be ToJson itself.
    let mut where_clause = item.where_clause.clone();
    let params = type_params(&item.impl_generics);
    if !params.is_empty() {
        if where_clause.is_empty() {
            where_clause.extend(q("where", &[]));
        } else if !matches!(where_clause.clone().into_iter().last(), Some(TokenTree::Punct(p)) if p.as_char() == ',') {
            where_clause.extend(q(",", &[]));
        }
        for param in params {
            where_clause.extend([TokenTree::Ident(param)]);
            where_clause.extend(q(": ::corten::json::ToJson,", &[]));
        }
    }
    let mut out = q("#[automatically_derived] impl", &[]);
    out.extend(item.impl_generics.clone());
    out.extend(q("::corten::json::ToJson for", &[]));
    out.extend([TokenTree::Ident(item.name.clone())]);
    out.extend(item.type_generics.clone());
    out.extend(where_clause);
    out.extend([TokenTree::Group(Group::new(
        Delimiter::Brace,
        q(
            "#[allow(unused_assignments, unused_mut, unused_variables)]
            fn write_json(&self, __OUT: &mut ::std::vec::Vec<u8>) {
                use ::corten::json::__private::{ViaSerialize as _, ViaToJson as _};
                __A0
            }
            fn __json_size() -> ::core::option::Option<&'static ::core::sync::atomic::AtomicUsize> {
                static SIZE: ::core::sync::atomic::AtomicUsize = ::core::sync::atomic::AtomicUsize::new(0);
                ::core::option::Option::Some(&SIZE)
            }",
            &[body],
        ),
    ))]);
    Ok(out)
}

/// The type parameters (not lifetimes or consts) of `<'a, T: X, const N: usize>`.
pub(crate) fn type_params(impl_generics: &TokenStream) -> Vec<Ident> {
    let tokens: Vec<TokenTree> = impl_generics.clone().into_iter().collect();
    if tokens.len() < 2 {
        return Vec::new();
    }
    let inner: TokenStream = tokens[1..tokens.len() - 1].iter().cloned().collect();
    split(inner, ',')
        .into_iter()
        .filter_map(|param| match param.first() {
            Some(TokenTree::Ident(id)) if id.to_string() != "const" => Some(id.clone()),
            _ => None,
        })
        .collect()
}

// ---- #[serde(...)] ----

pub(crate) enum Value {
    Bare,
    Str(String),
    Group,
}

pub(crate) struct Arg {
    pub(crate) key: String,
    pub(crate) value: Value,
    pub(crate) span: Span,
}

impl Arg {
    fn string(&self) -> Result<String, Error> {
        match &self.value {
            Value::Str(s) => Ok(s.clone()),
            _ => err(self.span, format!("ToJson: use #[serde({} = \"...\")]", self.key)),
        }
    }

    fn unsupported<T>(&self) -> Result<T, Error> {
        err(
            self.span,
            format!(
                "ToJson: #[serde({})] changes what serde writes in a way ToJson does not follow; implement ToJson by hand for this type",
                self.key
            ),
        )
    }
}

fn serde_args(attrs: &[Group]) -> Result<Vec<Arg>, Error> {
    serde_args_for(attrs, "ToJson")
}

/// The arguments of every `#[serde(...)]` in `attrs`; `derive` names the derive in errors.
pub(crate) fn serde_args_for(attrs: &[Group], derive: &str) -> Result<Vec<Arg>, Error> {
    let mut out = Vec::new();
    for attr in attrs {
        let tokens: Vec<TokenTree> = attr.stream().into_iter().collect();
        let (Some(TokenTree::Ident(path)), Some(TokenTree::Group(args))) = (tokens.first(), tokens.get(1)) else { continue };
        if path.to_string() != "serde" {
            continue;
        }
        for arg in split(args.stream(), ',') {
            let Some(TokenTree::Ident(key)) = arg.first() else {
                return err(args.span(), format!("{derive}: expected #[serde(key)] or #[serde(key = \"value\")]"));
            };
            let value = match (arg.get(1), arg.get(2)) {
                (None, _) => Value::Bare,
                (Some(TokenTree::Punct(p)), Some(TokenTree::Literal(l))) if p.as_char() == '=' => match string_value(l) {
                    Some(s) => Value::Str(s),
                    None => return err(l.span(), format!("{derive}: expected a string")),
                },
                (Some(TokenTree::Group(_)), _) => Value::Group,
                _ => return err(key.span(), format!("{derive}: expected #[serde(key)] or #[serde(key = \"value\")]")),
            };
            out.push(Arg { key: key.to_string(), value, span: key.span() });
        }
    }
    Ok(out)
}

#[derive(Default)]
struct Container {
    rename_all: Option<String>,
    rename_all_fields: Option<String>,
    transparent: bool,
    tag: Option<String>,
    content: Option<String>,
    untagged: bool,
}

impl Container {
    fn read(attrs: &[Group], is_enum: bool) -> Result<Self, Error> {
        let mut c = Container::default();
        for arg in serde_args(attrs)? {
            match arg.key.as_str() {
                "rename_all" => c.rename_all = Some(rule(&arg)?),
                "rename_all_fields" if is_enum => c.rename_all_fields = Some(rule(&arg)?),
                "transparent" if !is_enum => c.transparent = true,
                "tag" if is_enum => c.tag = Some(arg.string()?),
                "content" if is_enum => c.content = Some(arg.string()?),
                "untagged" if is_enum => c.untagged = true,
                // What reading does, or the type's own name, which the JSON does not show.
                "rename"
                | "deny_unknown_fields"
                | "default"
                | "crate"
                | "expecting"
                | "from"
                | "try_from"
                | "variant_identifier"
                | "field_identifier" => {}
                _ => return arg.unsupported(),
            }
        }
        Ok(c)
    }
}

/// A field of a struct or of a variant.
struct Field {
    /// The name, without `r#`; `None` in a tuple.
    name: Option<String>,
    /// The identifier as written (`r#type` stays raw), or the position in a tuple.
    member: TokenTree,
    rename: Option<String>,
    skip: bool,
    skip_if: Option<TokenStream>,
    ty: Vec<TokenTree>,
}

fn parse_fields(group: &Group, named: bool) -> Result<Vec<Field>, Error> {
    let mut fields = Vec::new();
    for (index, tokens) in split(group.stream(), ',').into_iter().enumerate() {
        let mut j = 0;
        let mut attrs = Vec::new();
        while is_punct(tokens.get(j), '#') {
            if let Some(TokenTree::Group(g)) = tokens.get(j + 1) {
                attrs.push(g.clone());
            }
            j += 2;
        }
        if is_ident(tokens.get(j), "pub") {
            j += 1;
            if matches!(tokens.get(j), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis) {
                j += 1;
            }
        }
        let (name, member, ty) = if named {
            let (Some(TokenTree::Ident(ident)), true) = (tokens.get(j), is_punct(tokens.get(j + 1), ':')) else {
                return err(group.span(), "ToJson: expected a named field");
            };
            let text = ident.to_string();
            (
                Some(text.strip_prefix("r#").unwrap_or(&text).to_string()),
                TokenTree::Ident(ident.clone()),
                tokens[j + 2..].to_vec(),
            )
        } else {
            (None, TokenTree::Literal(Literal::usize_unsuffixed(index)), tokens[j..].to_vec())
        };
        let mut field = Field { name, member, rename: None, skip: false, skip_if: None, ty };
        for arg in serde_args(&attrs)? {
            match arg.key.as_str() {
                "rename" => field.rename = Some(arg.string()?),
                "skip" | "skip_serializing" => field.skip = true,
                "skip_serializing_if" if named => match arg.string()?.parse::<TokenStream>() {
                    Ok(path) => field.skip_if = Some(path),
                    Err(_) => return err(arg.span, "ToJson: skip_serializing_if takes a function's path"),
                },
                // Only reading uses these.
                "default" | "alias" | "skip_deserializing" | "deserialize_with" | "borrow" => {}
                _ => return arg.unsupported(),
            }
        }
        fields.push(field);
    }
    Ok(fields)
}

// ---- rename_all, as serde applies it ----

pub(crate) const RULES: [&str; 8] = [
    "lowercase",
    "UPPERCASE",
    "PascalCase",
    "camelCase",
    "snake_case",
    "SCREAMING_SNAKE_CASE",
    "kebab-case",
    "SCREAMING-KEBAB-CASE",
];

fn rule(arg: &Arg) -> Result<String, Error> {
    let rule = arg.string()?;
    if !RULES.contains(&rule.as_str()) {
        return err(arg.span, format!("ToJson: unknown rename_all rule {rule:?}; serde's are {}", RULES.join(", ")));
    }
    Ok(rule)
}

/// A snake_case field name under a rule (serde's `RenameRule::apply_to_field`).
pub(crate) fn field_rule(field: &str, rule: &str) -> String {
    match rule {
        "lowercase" | "snake_case" => field.to_string(),
        "UPPERCASE" | "SCREAMING_SNAKE_CASE" => field.to_ascii_uppercase(),
        "kebab-case" => field.replace('_', "-"),
        "SCREAMING-KEBAB-CASE" => field.to_ascii_uppercase().replace('_', "-"),
        "PascalCase" | "camelCase" => {
            let mut pascal = String::new();
            let mut capitalize = true;
            for ch in field.chars() {
                if ch == '_' {
                    capitalize = true;
                } else if capitalize {
                    pascal.push(ch.to_ascii_uppercase());
                    capitalize = false;
                } else {
                    pascal.push(ch);
                }
            }
            if rule == "camelCase" {
                lower_first(&pascal)
            } else {
                pascal
            }
        }
        _ => unreachable!("checked by rule()"),
    }
}

/// A PascalCase variant name under a rule (serde's `RenameRule::apply_to_variant`).
pub(crate) fn variant_rule(variant: &str, rule: &str) -> String {
    let snake = || {
        let mut snake = String::new();
        for (i, ch) in variant.char_indices() {
            if i > 0 && ch.is_uppercase() {
                snake.push('_');
            }
            snake.push(ch.to_ascii_lowercase());
        }
        snake
    };
    match rule {
        "PascalCase" => variant.to_string(),
        "lowercase" => variant.to_ascii_lowercase(),
        "UPPERCASE" => variant.to_ascii_uppercase(),
        "camelCase" => lower_first(variant),
        "snake_case" => snake(),
        "SCREAMING_SNAKE_CASE" => snake().to_ascii_uppercase(),
        "kebab-case" => snake().replace('_', "-"),
        "SCREAMING-KEBAB-CASE" => snake().to_ascii_uppercase().replace('_', "-"),
        _ => unreachable!("checked by rule()"),
    }
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_ascii_lowercase().to_string() + chars.as_str()).unwrap_or_default()
}

/// `text` as a JSON string, quotes included, escaped as serde_json escapes.
fn json_string(text: &str) -> Vec<u8> {
    let mut out = vec![b'"'];
    for ch in text.chars() {
        match ch {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\r' => out.extend_from_slice(b"\\r"),
            '\t' => out.extend_from_slice(b"\\t"),
            '\u{8}' => out.extend_from_slice(b"\\b"),
            '\u{c}' => out.extend_from_slice(b"\\f"),
            c if (c as u32) < 0x20 => out.extend_from_slice(format!("\\u{:04x}", c as u32).as_bytes()),
            c => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }
    out.push(b'"');
    out
}

// ---- Output ----

enum Part {
    Literal(Vec<u8>),
    Code(TokenStream),
}

/// The write as a list of literals and code; neighbouring literals merge.
#[derive(Default)]
struct Gen {
    parts: Vec<Part>,
}

impl Gen {
    fn lit(&mut self, bytes: &[u8]) {
        match self.parts.last_mut() {
            Some(Part::Literal(last)) => last.extend_from_slice(bytes),
            _ => self.parts.push(Part::Literal(bytes.to_vec())),
        }
    }

    fn code(&mut self, code: TokenStream) {
        self.parts.push(Part::Code(code));
    }

    fn finish(self) -> TokenStream {
        let mut out = TokenStream::new();
        for part in self.parts {
            out.extend(match part {
                Part::Literal(bytes) => {
                    q("__OUT.extend_from_slice(__A0);", &[TokenTree::Literal(Literal::byte_string(&bytes)).into()])
                }
                Part::Code(code) => code,
            });
        }
        out
    }
}

/// `&self.field`.
fn self_field(field: &Field) -> TokenStream {
    let mut access = q("&self.", &[]);
    access.extend([field.member.clone()]);
    access
}

/// A `String` or `&str` (by its path's last name, without generics), whose quotes can go in the
/// literals around it. Anything else, a `str` newtype included, writes itself.
fn is_string(ty: &[TokenTree]) -> bool {
    let mut i = 0;
    let reference = is_punct(ty.first(), '&');
    if reference {
        i = 1;
        if is_punct(ty.get(1), '\'') {
            i = 3;
        }
        if is_ident(ty.get(i), "mut") {
            i += 1;
        }
    }
    let path = &ty[i.min(ty.len())..];
    let plain = !path.is_empty()
        && path.iter().all(|t| matches!(t, TokenTree::Ident(_)) || matches!(t, TokenTree::Punct(p) if p.as_char() == ':'));
    match path.last() {
        Some(TokenTree::Ident(last)) if plain => last.to_string() == "String" || (reference && last.to_string() == "str"),
        _ => false,
    }
}

/// Writes the value `access` points to.
fn value(gen: &mut Gen, ty: &[TokenTree], access: TokenStream) {
    if is_string(ty) {
        gen.lit(b"\"");
        gen.code(q("::corten::json::__private::str_body(__OUT, __A0);", &[access]));
        gen.lit(b"\"");
    } else {
        gen.code(q("(&::corten::json::__private::Field(__A0)).__write(__OUT);", &[access]));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Comma {
    /// Nothing written yet.
    First,
    /// Something written: the next field starts with a comma.
    After,
    /// Decided when it runs: only fields that may be left out have come so far.
    Runtime,
}

/// `open`, then the fields as `"key":value` pairs, then `}`.
fn object(
    gen: &mut Gen,
    open: &[u8],
    start: Comma,
    fields: &[Field],
    rule: Option<&str>,
    access: impl Fn(&Field) -> TokenStream,
) {
    gen.lit(open);
    let mut comma = start;
    let shown = fields.iter().filter(|f| !f.skip);
    if start == Comma::First && shown.clone().next().is_some_and(|f| f.skip_if.is_some()) {
        gen.code(q("let mut __corten_first = true;", &[]));
    }
    for field in shown {
        let name = field.name.as_deref().expect("named fields");
        let mut key = json_string(&match (&field.rename, rule) {
            (Some(rename), _) => rename.clone(),
            (None, Some(rule)) => field_rule(name, rule),
            (None, None) => name.to_string(),
        });
        key.push(b':');
        let mut pair = Gen::default();
        match comma {
            Comma::First => {}
            Comma::After => pair.lit(b","),
            Comma::Runtime => pair.code(q("if !__corten_first { __OUT.push(b','); }", &[])),
        }
        pair.lit(&key);
        value(&mut pair, &field.ty, access(field));
        match &field.skip_if {
            None => {
                gen.parts.extend(pair.parts);
                comma = Comma::After;
            }
            Some(path) => {
                if comma != Comma::After {
                    pair.code(q("__corten_first = false;", &[]));
                    comma = Comma::Runtime;
                }
                gen.code(q("if !__A0(__A1) { __A2 }", &[parens(path.clone()), access(field), pair.finish()]));
            }
        }
    }
    gen.lit(b"}");
}

/// A tuple: one field is its value (a newtype), more are an array.
fn tuple(gen: &mut Gen, fields: &[Field], access: impl Fn(&Field) -> TokenStream) -> Result<(), Error> {
    if let [only] = fields {
        if !only.skip {
            value(gen, &only.ty, access(only));
            return Ok(());
        }
    }
    gen.lit(b"[");
    for (i, field) in fields.iter().filter(|f| !f.skip).enumerate() {
        if i > 0 {
            gen.lit(b",");
        }
        value(gen, &field.ty, access(field));
    }
    gen.lit(b"]");
    Ok(())
}

/// `#[serde(transparent)]`: the one field that is not skipped, as itself.
fn transparent(fields: &[Field], span: Span) -> Result<TokenStream, Error> {
    let mut shown = fields.iter().filter(|f| !f.skip);
    let (Some(field), None) = (shown.next(), shown.next()) else {
        return err(span, "ToJson: #[serde(transparent)] needs exactly one field that is not skipped");
    };
    let mut gen = Gen::default();
    value(&mut gen, &field.ty, self_field(field));
    Ok(gen.finish())
}

fn enumeration(container: &Container, body: &Group) -> Result<TokenStream, Error> {
    if container.content.is_some() && container.tag.is_none() {
        return err(body.span(), "ToJson: #[serde(content = \"...\")] needs tag = \"...\" too");
    }
    if container.untagged && container.tag.is_some() {
        return err(body.span(), "ToJson: an enum is either untagged or tagged");
    }
    let mut arms = TokenStream::new();
    for tokens in split(body.stream(), ',') {
        let mut j = 0;
        let mut attrs = Vec::new();
        while is_punct(tokens.get(j), '#') {
            if let Some(TokenTree::Group(g)) = tokens.get(j + 1) {
                attrs.push(g.clone());
            }
            j += 2;
        }
        let Some(TokenTree::Ident(ident)) = tokens.get(j) else { return err(body.span(), "ToJson: expected a variant") };
        let (mut rename, mut rule) = (None, container.rename_all_fields.clone());
        for arg in serde_args(&attrs)? {
            match arg.key.as_str() {
                "rename" => rename = Some(arg.string()?),
                "rename_all" => rule = Some(self::rule(&arg)?),
                "alias" | "other" | "borrow" | "deserialize_with" => {}
                "skip" | "skip_serializing" => {
                    return err(
                        arg.span,
                        "ToJson: a variant that serde cannot write is not supported; remove the variant or the skip",
                    )
                }
                _ => return arg.unsupported(),
            }
        }
        let name = rename.unwrap_or_else(|| {
            let text = ident.to_string();
            let text = text.strip_prefix("r#").unwrap_or(&text).to_string();
            container.rename_all.as_deref().map_or(text.clone(), |rule| variant_rule(&text, rule))
        });
        let name = json_string(&name);
        let (named, fields) = match tokens.get(j + 1) {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => (true, Some(parse_fields(g, true)?)),
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => (false, Some(parse_fields(g, false)?)),
            _ => (false, None),
        };
        let binding = |field: &Field| -> TokenStream {
            let index = match &field.member {
                TokenTree::Literal(l) => l.to_string(),
                TokenTree::Ident(i) => i.to_string(),
                _ => unreachable!(),
            };
            TokenTree::Ident(Ident::new(&format!("__corten_{}", index.trim_start_matches("r#")), Span::call_site())).into()
        };
        // The pattern: `Self::A`, `Self::B(x, _)` or `Self::C { a: x, .. }`.
        let mut pattern = q("Self::", &[]);
        pattern.extend([TokenTree::Ident(ident.clone())]);
        if let Some(fields) = &fields {
            let mut inner = TokenStream::new();
            for field in fields {
                if named {
                    inner.extend([field.member.clone()]);
                    inner.extend(q(":", &[]));
                }
                inner.extend(if field.skip { q("_", &[]) } else { binding(field) });
                inner.extend(q(",", &[]));
            }
            if named {
                inner.extend(q("..", &[]));
            }
            pattern.extend([TokenTree::Group(Group::new(if named { Delimiter::Brace } else { Delimiter::Parenthesis }, inner))]);
        }
        let mut gen = Gen::default();
        let tag = |gen: &mut Gen, tag: &str| {
            gen.lit(b"{");
            gen.lit(&json_string(tag));
            gen.lit(b":");
            gen.lit(&name);
        };
        let span = ident.span();
        match (&fields, named) {
            // A unit variant.
            (None, _) => match (&container.tag, container.untagged) {
                (Some(t), _) => {
                    tag(&mut gen, t);
                    gen.lit(b"}");
                }
                (None, true) => gen.lit(b"null"),
                (None, false) => gen.lit(&name),
            },
            (Some(fields), true) => match (&container.tag, &container.content, container.untagged) {
                (Some(t), None, _) => {
                    let mut open = Vec::new();
                    open.push(b'{');
                    open.extend(json_string(t));
                    open.push(b':');
                    open.extend(&name);
                    object(&mut gen, &open, Comma::After, fields, rule.as_deref(), binding);
                }
                (Some(t), Some(c), _) => {
                    tag(&mut gen, t);
                    gen.lit(b",");
                    gen.lit(&json_string(c));
                    gen.lit(b":");
                    object(&mut gen, b"{", Comma::First, fields, rule.as_deref(), binding);
                    gen.lit(b"}");
                }
                (None, _, true) => object(&mut gen, b"{", Comma::First, fields, rule.as_deref(), binding),
                (None, _, false) => {
                    gen.lit(b"{");
                    gen.lit(&name);
                    gen.lit(b":");
                    object(&mut gen, b"{", Comma::First, fields, rule.as_deref(), binding);
                    gen.lit(b"}");
                }
            },
            (Some(fields), false) => match (&container.tag, &container.content, container.untagged) {
                (Some(_), None, _) => return err(
                    span,
                    "ToJson: an internally tagged enum (tag without content) needs struct or unit variants; use a struct variant",
                ),
                (Some(t), Some(c), _) => {
                    tag(&mut gen, t);
                    gen.lit(b",");
                    gen.lit(&json_string(c));
                    gen.lit(b":");
                    tuple(&mut gen, fields, binding)?;
                    gen.lit(b"}");
                }
                (None, _, true) => tuple(&mut gen, fields, binding)?,
                (None, _, false) => {
                    gen.lit(b"{");
                    gen.lit(&name);
                    gen.lit(b":");
                    tuple(&mut gen, fields, binding)?;
                    gen.lit(b"}");
                }
            },
        }
        arms.extend(pattern);
        arms.extend(q("=> { __A0 }", &[gen.finish()]));
    }
    if arms.is_empty() {
        return Ok(q("match *self {}", &[]));
    }
    Ok(q("match self { __A0 }", &[arms]))
}
