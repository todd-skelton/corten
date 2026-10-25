//! `#[derive(FromJson)]`: a struct, or an enum of unit variants, read from JSON by Corten's reader,
//! with serde_json reading the text instead wherever the reader gives up.
//!
//! The generated `__read` matches each key against the field names (after `rename`, `rename_all`
//! and `alias`), reads a known field's value straight into its type and skips the rest. A
//! duplicate or missing field without a default gives up, so serde_json's answer (an error, or
//! `None` for an `Option`) is the one returned.
//!
//! `#[serde(...)]` is read so the value is what serde's `Deserialize` makes; an attribute that changes
//! how serde reads in a way this does not follow is a compile error.

use proc_macro::{Delimiter, Group, Ident, Literal, Span, TokenStream, TokenTree};

use crate::{
    component::split,
    derive::parse_item,
    to_json::{field_rule, serde_args_for, type_params, variant_rule, Arg, Value, RULES},
    tokens::{err, is_ident, is_punct, q, Error},
};

const NAME: &str = "FromJson";

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let item = parse_item(input, NAME, true)?;
    let container = Container::read(&item.attrs, item.is_enum)?;
    let body = if item.is_enum {
        let Some(body) = &item.body else { return err(item.name.span(), "FromJson: expected the enum's variants") };
        enumeration(&container, body)?
    } else if let Some(fields) = &item.tuple {
        let fields = parse_fields(fields, false)?;
        if container.transparent {
            transparent(&fields, item.name.span())?
        } else {
            tuple(&fields, item.name.span())?
        }
    } else if let Some(fields) = &item.body {
        let fields = parse_fields(fields, true)?;
        if container.transparent {
            transparent(&fields, item.name.span())?
        } else {
            object(&container, &fields)
        }
    } else {
        q("if __r.null() { ::core::result::Result::Ok(Self) } else { ::core::result::Result::Err(::corten::json::__read::Fallback) }", &[])
    };

    // Each type parameter must be FromJson itself.
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
            where_clause.extend(q(": ::corten::json::FromJson,", &[]));
        }
    }
    let mut out = q("#[automatically_derived] impl", &[]);
    out.extend(item.impl_generics.clone());
    out.extend(q("::corten::json::FromJson for", &[]));
    out.extend([TokenTree::Ident(item.name.clone())]);
    out.extend(item.type_generics.clone());
    out.extend(where_clause);
    out.extend([TokenTree::Group(Group::new(
        Delimiter::Brace,
        q(
            "#[allow(unused_mut, unused_variables, unused_imports, clippy::all)]
            fn __read(__r: &mut ::corten::json::__read::Reader<'_>)
                -> ::core::result::Result<Self, ::corten::json::__read::Fallback> {
                use ::corten::json::__read::{ViaDeserialize as _, ViaFromJson as _};
                __A0
            }",
            &[body],
        ),
    ))]);
    Ok(out)
}

fn unsupported<T>(arg: &Arg) -> Result<T, Error> {
    err(
        arg.span,
        format!(
            "FromJson: #[serde({})] changes how serde reads in a way FromJson does not follow; derive Deserialize alone for this type (as a field it is then read by serde_json)",
            arg.key
        ),
    )
}

fn string(arg: &Arg) -> Result<String, Error> {
    match &arg.value {
        Value::Str(s) => Ok(s.clone()),
        _ => err(arg.span, format!("FromJson: use #[serde({} = \"...\")]", arg.key)),
    }
}

fn rule(arg: &Arg) -> Result<String, Error> {
    let rule = string(arg)?;
    if !RULES.contains(&rule.as_str()) {
        return err(arg.span, format!("FromJson: unknown rename_all rule {rule:?}; serde's are {}", RULES.join(", ")));
    }
    Ok(rule)
}

/// `#[serde(default)]` or `#[serde(default = "path")]`.
enum DefaultValue {
    Trait,
    Path(TokenStream),
}

fn default(arg: &Arg) -> Result<DefaultValue, Error> {
    match &arg.value {
        Value::Bare => Ok(DefaultValue::Trait),
        Value::Str(path) => match path.parse::<TokenStream>() {
            Ok(path) => Ok(DefaultValue::Path(path)),
            Err(_) => err(arg.span, "FromJson: default takes a function's path"),
        },
        Value::Group => err(arg.span, "FromJson: use #[serde(default)] or #[serde(default = \"path\")]"),
    }
}

#[derive(Default)]
struct Container {
    rename_all: Option<String>,
    default: Option<DefaultValue>,
    deny_unknown_fields: bool,
    transparent: bool,
}

impl Container {
    fn read(attrs: &[Group], is_enum: bool) -> Result<Self, Error> {
        let mut c = Container::default();
        for arg in serde_args_for(attrs, NAME)? {
            match arg.key.as_str() {
                "rename_all" => c.rename_all = Some(rule(&arg)?),
                "default" if !is_enum => c.default = Some(default(&arg)?),
                "deny_unknown_fields" => c.deny_unknown_fields = true,
                "transparent" if !is_enum => c.transparent = true,
                // Writing only, the type's own name, serde's path, or its error text and bounds:
                // none changes what a successful read makes.
                "rename" | "rename_all_fields" | "into" | "crate" | "expecting" | "bound" => {}
                _ => return unsupported(&arg),
            }
        }
        Ok(c)
    }
}

struct Field {
    /// The name, without `r#`; `None` in a tuple.
    name: Option<String>,
    /// The identifier as written (`r#type` stays raw), or the position in a tuple.
    member: TokenTree,
    rename: Option<String>,
    aliases: Vec<String>,
    default: Option<DefaultValue>,
    skip: bool,
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
                return err(group.span(), "FromJson: expected a named field");
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
        let mut field = Field { name, member, rename: None, aliases: Vec::new(), default: None, skip: false, ty };
        for arg in serde_args_for(&attrs, NAME)? {
            match arg.key.as_str() {
                "rename" => field.rename = Some(string(&arg)?),
                "alias" => field.aliases.push(string(&arg)?),
                "default" => field.default = Some(default(&arg)?),
                "skip" | "skip_deserializing" => field.skip = true,
                // Writing only, or bounds.
                "skip_serializing" | "skip_serializing_if" | "serialize_with" | "bound" | "getter" => {}
                _ => return unsupported(&arg),
            }
        }
        fields.push(field);
    }
    Ok(fields)
}

fn ty(field: &Field) -> TokenStream {
    field.ty.iter().cloned().collect()
}

fn var(index: usize) -> TokenStream {
    TokenTree::Ident(Ident::new(&format!("__f{index}"), Span::mixed_site())).into()
}

/// Reads the field's value: by its `FromJson` impl, or by serde_json from its text.
fn read(field: &Field) -> TokenStream {
    q("(&::corten::json::__read::Read::<__A0>::new()).__read(__r)?", &[ty(field)])
}

/// What a field the object leaves out (or a skipped one) is: its own default, the container's,
/// or (not skipped) what serde makes of a missing field.
fn fallback_value(container: &Container, field: &Field) -> TokenStream {
    let member: TokenStream = field.member.clone().into();
    match (&field.default, &container.default) {
        (Some(DefaultValue::Trait), _) => q("::core::default::Default::default()", &[]),
        (Some(DefaultValue::Path(path)), _) => q("__A0()", std::slice::from_ref(path)),
        (None, Some(_)) => q("__default.__A0", &[member]),
        (None, None) if field.skip => q("::core::default::Default::default()", &[]),
        (None, None) => q("::corten::json::__read::missing::<__A0>()?", &[ty(field)]),
    }
}

fn container_default(container: &Container) -> TokenStream {
    match &container.default {
        Some(DefaultValue::Trait) => q("let __default: Self = ::core::default::Default::default();", &[]),
        Some(DefaultValue::Path(path)) => q("let __default: Self = __A0();", std::slice::from_ref(path)),
        None => TokenStream::new(),
    }
}

fn byte_string(text: &str) -> TokenStream {
    TokenTree::Literal(Literal::byte_string(text.as_bytes())).into()
}

/// A struct with named fields, from an object.
fn object(container: &Container, fields: &[Field]) -> TokenStream {
    if container.default.is_some() {
        return object_over_default(container, fields);
    }
    let mut vars = TokenStream::new();
    let mut arms = TokenStream::new();
    let mut build = TokenStream::new();
    for (index, field) in fields.iter().enumerate() {
        let member: TokenStream = field.member.clone().into();
        if field.skip {
            build.extend(q("__A0: __A1,", &[member, fallback_value(container, field)]));
            continue;
        }
        let name = field.name.as_deref().unwrap_or_default();
        let key = field
            .rename
            .clone()
            .unwrap_or_else(|| container.rename_all.as_deref().map_or(name.to_string(), |rule| field_rule(name, rule)));
        let mut pattern = byte_string(&key);
        for alias in &field.aliases {
            pattern.extend(q("|", &[]));
            pattern.extend(byte_string(alias));
        }
        vars.extend(q("let mut __A0: ::core::option::Option<__A1> = ::core::option::Option::None;", &[var(index), ty(field)]));
        arms.extend(q(
            "__A0 => {
                if __A1.is_some() {
                    return ::core::result::Result::Err(::corten::json::__read::Fallback);
                }
                __A1 = ::core::option::Option::Some(__A2);
            }",
            &[pattern, var(index), read(field)],
        ));
        build.extend(q(
            "__A0: match __A1 { ::core::option::Option::Some(v) => v, ::core::option::Option::None => __A2 },",
            &[member, var(index), fallback_value(container, field)],
        ));
    }
    let unknown = if container.deny_unknown_fields {
        q("_ => return ::core::result::Result::Err(::corten::json::__read::Fallback),", &[])
    } else {
        q("_ => __r.skip()?,", &[])
    };
    arms.extend(unknown);
    q(
        "__A0
        if __r.object()? {
            loop {
                match __r.key()? { __A1 }
                if !__r.more()? {
                    break;
                }
            }
        }
        __A2
        ::core::result::Result::Ok(Self { __A3 })",
        &[vars, arms, container_default(container), build],
    )
}

/// A struct with a container default, read over that default: each field the object has is
/// written in place, and one it leaves out keeps the default's value (or takes its own default).
fn object_over_default(container: &Container, fields: &[Field]) -> TokenStream {
    let mut vars = TokenStream::new();
    let mut arms = TokenStream::new();
    let mut after = TokenStream::new();
    for (index, field) in fields.iter().enumerate() {
        let member: TokenStream = field.member.clone().into();
        let own_default = match &field.default {
            Some(DefaultValue::Trait) => Some(q("::core::default::Default::default()", &[])),
            Some(DefaultValue::Path(path)) => Some(q("__A0()", std::slice::from_ref(path))),
            None => None,
        };
        if field.skip {
            if let Some(value) = own_default {
                after.extend(q("__out.__A0 = __A1;", &[member, value]));
            }
            continue;
        }
        let name = field.name.as_deref().unwrap_or_default();
        let key = field
            .rename
            .clone()
            .unwrap_or_else(|| container.rename_all.as_deref().map_or(name.to_string(), |rule| field_rule(name, rule)));
        let mut pattern = byte_string(&key);
        for alias in &field.aliases {
            pattern.extend(q("|", &[]));
            pattern.extend(byte_string(alias));
        }
        vars.extend(q("let mut __A0 = false;", &[var(index)]));
        arms.extend(q(
            "__A0 => {
                if __A1 {
                    return ::core::result::Result::Err(::corten::json::__read::Fallback);
                }
                __A1 = true;
                __out.__A2 = __A3;
            }",
            &[pattern, var(index), member.clone(), read(field)],
        ));
        if let Some(value) = own_default {
            after.extend(q("if !__A0 { __out.__A1 = __A2; }", &[var(index), member, value]));
        }
    }
    let unknown = if container.deny_unknown_fields {
        q("_ => return ::core::result::Result::Err(::corten::json::__read::Fallback),", &[])
    } else {
        q("_ => __r.skip()?,", &[])
    };
    arms.extend(unknown);
    let start = match &container.default {
        Some(DefaultValue::Path(path)) => q("__A0()", std::slice::from_ref(path)),
        _ => q("::core::default::Default::default()", &[]),
    };
    q(
        "let mut __out: Self = __A0;
        __A1
        if __r.object()? {
            loop {
                match __r.key()? { __A2 }
                if !__r.more()? {
                    break;
                }
            }
        }
        __A3
        ::core::result::Result::Ok(__out)",
        &[start, vars, arms, after],
    )
}

/// A tuple struct: one field is its value (serde's newtype), more are an array of exactly that
/// many.
fn tuple(fields: &[Field], span: Span) -> Result<TokenStream, Error> {
    if fields.iter().any(|f| f.skip || f.default.is_some()) {
        return err(span, "FromJson: skip and default in a tuple struct are not supported; derive Deserialize alone");
    }
    let mut build = TokenStream::new();
    if let [field] = fields {
        build.extend(q("0: __A0,", &[read(field)]));
        return Ok(q("::core::result::Result::Ok(Self { __A0 })", &[build]));
    }
    let mut steps = TokenStream::new();
    for (index, field) in fields.iter().enumerate() {
        let more = if index + 1 < fields.len() {
            q("if !__r.more_items()? { return ::core::result::Result::Err(::corten::json::__read::Fallback); }", &[])
        } else {
            q("if __r.more_items()? { return ::core::result::Result::Err(::corten::json::__read::Fallback); }", &[])
        };
        steps.extend(q("let __A0 = __A1; __A2", &[var(index), read(field), more]));
        build.extend(q("__A0: __A1,", &[field.member.clone().into(), var(index)]));
    }
    Ok(q(
        "if !__r.array()? {
            return ::core::result::Result::Err(::corten::json::__read::Fallback);
        }
        __A0
        ::core::result::Result::Ok(Self { __A1 })",
        &[steps, build],
    ))
}

/// `#[serde(transparent)]`: the one field that is not skipped is the value; the others are
/// their defaults.
fn transparent(fields: &[Field], span: Span) -> Result<TokenStream, Error> {
    let mut read_fields = fields.iter().filter(|f| !f.skip);
    let (Some(_), None) = (read_fields.next(), read_fields.next()) else {
        return err(span, "FromJson: #[serde(transparent)] needs exactly one field that is not skipped");
    };
    let mut build = TokenStream::new();
    for field in fields {
        let value = if field.skip {
            match &field.default {
                Some(DefaultValue::Path(path)) => q("__A0()", std::slice::from_ref(path)),
                _ => q("::core::default::Default::default()", &[]),
            }
        } else {
            read(field)
        };
        build.extend(q("__A0: __A1,", &[field.member.clone().into(), value]));
    }
    Ok(q("::core::result::Result::Ok(Self { __A0 })", &[build]))
}

/// An enum of unit variants, from its name as a string.
fn enumeration(container: &Container, body: &Group) -> Result<TokenStream, Error> {
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
        let Some(TokenTree::Ident(ident)) = tokens.get(j) else { return err(body.span(), "FromJson: expected a variant") };
        if matches!(tokens.get(j + 1), Some(TokenTree::Group(_))) {
            return err(
                ident.span(),
                "FromJson: only an enum of unit variants is read; derive Deserialize alone for this enum (as a field it is then read by serde_json)",
            );
        }
        let (mut rename, mut aliases, mut skip) = (None, Vec::new(), false);
        for arg in serde_args_for(&attrs, NAME)? {
            match arg.key.as_str() {
                "rename" => rename = Some(string(&arg)?),
                "alias" => aliases.push(string(&arg)?),
                "skip" | "skip_deserializing" => skip = true,
                "skip_serializing" | "serialize_with" | "bound" => {}
                _ => return unsupported(&arg),
            }
        }
        if skip {
            continue;
        }
        let name = rename.unwrap_or_else(|| {
            let text = ident.to_string();
            let text = text.strip_prefix("r#").unwrap_or(&text).to_string();
            container.rename_all.as_deref().map_or(text.clone(), |rule| variant_rule(&text, rule))
        });
        let mut pattern = byte_string(&name);
        for alias in &aliases {
            pattern.extend(q("|", &[]));
            pattern.extend(byte_string(alias));
        }
        arms.extend(q("__A0 => ::core::result::Result::Ok(Self::__A1),", &[pattern, TokenTree::Ident(ident.clone()).into()]));
    }
    Ok(q(
        "match __r.name()? {
            __A0
            _ => ::core::result::Result::Err(::corten::json::__read::Fallback),
        }",
        &[arms],
    ))
}
