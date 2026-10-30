//! Corten's macros, re-exported by the `corten` crate: `html!` and `#[component]` (see
//! `corten::view`), `#[derive(Fields)]` (see `corten::Fields`), `#[derive(ToJson)]` and `#[derive(FromJson)]` (see `corten::json`), `#[derive(Route)]` (see
//! `corten::route`), `#[derive(Id)]` (see `corten::view::Id`), `#[derive(FromRef)]` (see `corten::FromRef`), `#[event]` and `script!` (see `corten::script`), `messages!` (see `corten::i18n`) and
//! `tokens!` and component stylesheets (see `corten::style`), and `embed!` (see `corten::assets`).
//!
//! Written against `proc_macro` alone (no `syn` or `quote`), so the crate has no dependencies
//! and builds in about a second.

mod assets;
mod component;
mod derive;
mod dom;
mod event;
mod from_json;
mod from_ref;
mod html;
mod id;
mod json;
mod locale_data;
mod messages;
mod names;
mod route;
mod routes;
mod script;
mod style;
mod theme;
mod to_json;
mod tokens;
mod url_value;

use proc_macro::{Delimiter, Group, Spacing, TokenStream, TokenTree};

/// JSX-like markup compiled to string pushes. Documented at `corten::view`.
#[proc_macro]
pub fn html(input: TokenStream) -> TokenStream {
    html::expand(input)
}

/// A function as a component struct. Documented at `corten::view::component`.
#[proc_macro_attribute]
pub fn component(args: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(args, item).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A struct whose fields are a URL's parameters. Documented at `corten::route`.
#[proc_macro_derive(Route, attributes(route, serde))]
pub fn derive_route(input: TokenStream) -> TokenStream {
    route::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A module of routes and the trait of their handlers. Documented at `corten::route::routes`.
#[proc_macro_attribute]
pub fn routes(args: TokenStream, item: TokenStream) -> TokenStream {
    routes::expand(args, item).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// An enum of unit variants written in URLs as the words serde reads. Documented at
/// `corten::route::UrlValue`.
#[proc_macro_derive(UrlValue, attributes(serde))]
pub fn derive_url_value(input: TokenStream) -> TokenStream {
    url_value::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// One `FromRef` impl per field of the app's state. Documented at `corten::FromRef`.
#[proc_macro_derive(FromRef, attributes(from_ref))]
pub fn derive_from_ref(input: TokenStream) -> TokenStream {
    from_ref::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// An element id as a type. Documented at `corten::view::Id`.
#[proc_macro_derive(Id)]
pub fn derive_id(input: TokenStream) -> TokenStream {
    id::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A struct or an enum written as JSON, laid out when the crate compiles. Documented at
/// `corten::json`.
#[proc_macro_derive(ToJson, attributes(serde))]
pub fn derive_to_json(input: TokenStream) -> TokenStream {
    to_json::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A struct, or an enum of unit variants, read from JSON with serde_json as the fallback.
/// Documented at `corten::json`.
#[proc_macro_derive(FromJson, attributes(serde))]
pub fn derive_from_json(input: TokenStream) -> TokenStream {
    from_json::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A struct as a typed DOM event. Documented at `corten::script`.
#[proc_macro_attribute]
pub fn event(args: TokenStream, item: TokenStream) -> TokenStream {
    event::expand(args, item).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A handler module shared by components. Documented at `corten::script`.
#[proc_macro]
pub fn script(input: TokenStream) -> TokenStream {
    script::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A design-token file as CSS custom properties, typed scales and utility classes. Documented
/// at `corten::style`.
#[proc_macro]
pub fn tokens(input: TokenStream) -> TokenStream {
    theme::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// A static directory compiled into the program, a constant per file. Documented at
/// `corten::assets`.
#[proc_macro]
pub fn embed(input: TokenStream) -> TokenStream {
    assets::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// Message catalogs checked and compiled into a type. Documented at `corten::i18n`.
#[proc_macro]
pub fn messages(input: TokenStream) -> TokenStream {
    messages::expand(input).unwrap_or_else(|(span, message)| tokens::compile_error(span, &message))
}

/// Every locale's number, currency and date data, for corten's test against Intl. Not a public API.
#[doc(hidden)]
#[proc_macro]
pub fn __all_locales(_: TokenStream) -> TokenStream {
    messages::all_locales()
}

/// Field names of a form input struct as constants: `Input::FIELDS.email` is the
/// `corten::form::FieldName` `"email"`, with serde's `rename` and `rename_all` applied. Templates
/// print them for `name=` and handlers pass them to `Errors`, so a renamed or misspelled field is a
/// compile error.
#[proc_macro_derive(Fields, attributes(serde, fields))]
pub fn derive_fields(input: TokenStream) -> TokenStream {
    match fields(input) {
        Ok(code) => code.parse().expect("generated code parses"),
        Err(message) => format!("::core::compile_error!({message:?});").parse().expect("compile_error parses"),
    }
}

fn fields(input: TokenStream) -> Result<String, String> {
    let source = input.clone();
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut rename_all = None;
    let mut container_default = false;
    let mut visibility = String::new();
    let mut i = 0;
    let name = loop {
        match tokens.get(i) {
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                if let Some(TokenTree::Group(group)) = tokens.get(i + 1) {
                    for (key, value) in serde_args(group)? {
                        if key == "rename_all" {
                            rename_all = Some(value.ok_or("Fields: rename_all needs a value, like rename_all = \"camelCase\"")?);
                        }
                        container_default |= key == "default";
                    }
                }
                i += 2;
            }
            Some(TokenTree::Ident(id)) if id.to_string() == "pub" => {
                visibility = "pub".into();
                if let Some(TokenTree::Group(g)) = tokens.get(i + 1) {
                    if g.delimiter() == Delimiter::Parenthesis {
                        visibility = format!("pub({})", g.stream());
                        i += 1;
                    }
                }
                i += 1;
            }
            Some(TokenTree::Ident(id)) if id.to_string() == "struct" => match tokens.get(i + 1) {
                Some(TokenTree::Ident(name)) => {
                    i += 2;
                    break name.to_string();
                }
                _ => return Err("Fields: expected a struct name".into()),
            },
            Some(TokenTree::Ident(id)) if id.to_string() == "enum" || id.to_string() == "union" => {
                return Err("Fields can only be derived for a struct with named fields".into())
            }
            Some(_) => i += 1,
            None => return Err("Fields: expected a struct".into()),
        }
    };
    // A struct with generics (a query struct that borrows): the names do not depend on them.
    let generic = matches!(tokens.get(i), Some(TokenTree::Punct(p)) if p.as_char() == '<');
    let item = if generic { Some(derive::parse(source, "Fields").map_err(|(_, message)| message)?) } else { None };
    let named = "Fields can only be derived for a struct with named fields";
    let body = match (&item, tokens.get(i)) {
        (Some(item), _) => item.body.clone().ok_or(named)?,
        (None, Some(TokenTree::Group(g))) if g.delimiter() == Delimiter::Brace => g.clone(),
        _ => return Err(named.into()),
    };
    let (impl_generics, type_generics, where_clause) = match &item {
        Some(item) => (item.impl_generics.to_string(), item.type_generics.to_string(), item.where_clause.to_string()),
        None => Default::default(),
    };
    let mut names = Vec::new();
    // Whether a form can send none of its fields (each an `Option` or defaulted), and the groups
    // with no default, which must not be such a struct.
    let mut leave_out = true;
    let mut bare_groups = Vec::new();
    // The `Files` fields (file inputs): nothing reads them, so the code below does.
    let mut files = Vec::new();
    for field in split_fields(body.stream()) {
        let mut rename = None;
        let mut skip = false;
        let mut default = container_default;
        let mut ident = None;
        let mut kind = Kind::Plain;
        let mut ty: Vec<TokenTree> = Vec::new();
        // A file input is never among the text fields, so a form always leaves it out.
        let mut is_files = false;
        let mut j = 0;
        while j < field.len() {
            match &field[j] {
                TokenTree::Punct(p) if p.as_char() == '#' => {
                    if let Some(TokenTree::Group(group)) = field.get(j + 1) {
                        if let Some(found) = fields_kind(group)? {
                            kind = found;
                        }
                        for (key, value) in serde_args(group)? {
                            match key.as_str() {
                                "rename" => {
                                    rename = Some(
                                        value
                                            .ok_or("Fields: use rename = \"name\"; rename(deserialize = ..) is not supported")?,
                                    )
                                }
                                "skip" | "skip_deserializing" => skip = true,
                                "default" => default = true,
                                "flatten" => {
                                    return Err(
                                        "Fields: #[serde(flatten)] is not supported; a group of fields another struct defines is #[fields(group)], posted as group.field".into()
                                    )
                                }
                                _ => {}
                            }
                        }
                    }
                    j += 2;
                }
                TokenTree::Ident(id) if id.to_string() == "pub" => {
                    j += 1;
                    if matches!(field.get(j), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis) {
                        j += 1;
                    }
                }
                TokenTree::Ident(id) => {
                    ident = Some(id.to_string());
                    // After the name and its `:`, the type.
                    ty = field.get(j + 2..).unwrap_or_default().to_vec();
                    let ty = field.get(j + 2..).unwrap_or_default();
                    if matches!(
                        ty.iter()
                            .rev()
                            .find_map(|t| match t {
                                TokenTree::Ident(id) => Some(id.to_string()),
                                _ => None,
                            })
                            .as_deref(),
                        Some("Files")
                    ) {
                        files.push(id.to_string());
                        is_files = true;
                    }
                    if matches!(kind, Kind::Rows | Kind::Map) && !default {
                        return Err(format!(
                            "Fields: `{id}` is {}, and a form with none sends nothing for it; add #[serde(default)] so the form reads it as empty",
                            if kind == Kind::Rows { "rows" } else { "a map" }
                        ));
                    }
                    if !default && kind == Kind::Plain {
                        if let Some(what) = left_out_when_empty(field.get(j + 2..).unwrap_or_default()) {
                            return Err(format!(
                                "Fields: `{id}` is {what}; add #[serde(default)] so the form reads it as {}",
                                if what.starts_with("a bool") { "false" } else { "empty" }
                            ));
                        }
                    }
                    break;
                }
                _ => j += 1,
            }
        }
        let Some(ident) = ident else { continue };
        if skip {
            continue;
        }
        let bare = ident.strip_prefix("r#").unwrap_or(&ident).to_string();
        let wire = match (rename, &rename_all) {
            (Some(name), _) => name,
            (None, Some(rule)) => apply_rule(&bare, rule)?,
            (None, None) => bare.clone(),
        };
        if wire.contains(['.', '[', ']']) {
            return Err(format!(
                "Fields: `{bare}` is named {wire:?} in the form, and . [ ] name a group's or a row's fields: rename it without them"
            ));
        }
        leave_out &= default || is_files || is_option(&ty);
        let shape = match kind {
            Kind::Plain => Shape::Plain,
            Kind::Group => {
                let group = nested_type(&ident, &ty, "group")?;
                if !default {
                    bare_groups.push((ident.clone(), group.clone()));
                }
                Shape::Group(group)
            }
            Kind::Rows => {
                let (key, row) = map_types(&ident, &ty)?;
                Shape::Rows(key, row)
            }
            Kind::Map => Shape::Map(map_types(&ident, &ty)?.0),
        };
        names.push((ident, wire, shape));
    }
    let fields_type = format!("{name}Fields");
    // One struct of names per place the struct can be: the form's top, a group, a row, a group in
    // a row. Each field's name is a `FieldName` of that place, so a form's own field is a name alone.
    let places: [(&str, &str, &str, &str); 4] = [
        ("", "", "::corten::form::Root", "FIELDS"),
        ("Group", "", "::corten::form::Group", "group"),
        ("Row", "<'k>", "::corten::form::Row<'k>", "row"),
        ("RowGroup", "<'k>", "::corten::form::RowGroup<'k>", "row_group"),
    ];
    let mut out = String::new();
    let mut shape = String::from("let shape = 0u8;");
    for (ident, _, kind) in &names {
        match kind {
            Shape::Plain => {}
            Shape::Group(ty) => {
                shape += &format!(
                    " if <{ty} as ::corten::form::Fields>::SHAPE != 0 {{ ::core::panic!({:?}) }} let shape = if shape < 1 {{ 1 }} else {{ shape }};",
                    format!("corten: `{ident}` is a group, and a group's fields are plain: its type has a group or rows of its own")
                );
            }
            Shape::Rows(_, row) => {
                shape += &format!(
                    " if <{row} as ::corten::form::Fields>::SHAPE > 1 {{ ::core::panic!({:?}) }} let shape = 2;",
                    format!("corten: `{ident}` is rows, and a row's fields are plain fields and groups, not rows")
                );
            }
            // Keyed like rows, so only at a form's top.
            Shape::Map(_) => shape += " let shape = 2;",
        }
    }
    for (ident, ty) in &bare_groups {
        shape += &format!(
            " if <{ty} as ::corten::form::Fields>::LEAVE_OUT {{ ::core::panic!({:?}) }}",
            format!("corten: `{ident}` is a group whose fields a form can all leave out (an unchecked checkbox sends nothing, and a file input is never a text field), so it can send none of them; add #[serde(default)] so the form reads it as its default")
        );
    }
    for (suffix, lifetime, place, _) in places {
        let ty = format!("{name}{suffix}Fields");
        let place_doc = match suffix {
            "" => "",
            "Group" => " as a group of another form (`#[fields(group)]`), what a component that draws the group takes",
            "Row" => " in a row of another form (`#[fields(rows)]`)",
            _ => " as a group in a row of another form, what a component that draws the group in a row takes",
        };
        out += &format!(
            "/// Form field names of [`{name}`]{place_doc}, from `#[derive(Fields)]`.\n#[derive(Clone, Copy, Debug)]\n#[allow(dead_code)]\n{visibility} struct {ty}{lifetime} {{\n"
        );
        for (ident, _, kind) in &names {
            let field_ty = match (kind, suffix) {
                (Shape::Plain, _) => format!("::corten::form::FieldName<{place}>"),
                (Shape::Group(g), "") => format!("<{g} as ::corten::form::Fields>::InGroup"),
                (Shape::Group(g), "Row") => format!("<{g} as ::corten::form::Fields>::InRowGroup<'k>"),
                // A group in a group or in a row's group: its names exist, but taking them is refused.
                (Shape::Group(_), _) => "::corten::form::Refused".to_string(),
                (Shape::Rows(k, r), "") => format!("::corten::form::Rows<{k}, {r}>"),
                (Shape::Map(k), "") => format!("::corten::form::Keyed<{k}>"),
                (Shape::Rows(..) | Shape::Map(_), _) => "::corten::form::Refused".to_string(),
            };
            out += &format!("    pub {ident}: {field_ty},\n");
        }
        if !lifetime.is_empty() {
            out += "    #[doc(hidden)]\n    pub __k: ::core::marker::PhantomData<&'k ()>,\n";
        }
        out += "}\n";
    }
    let build = |suffix: &str, at: &str| {
        let mut fields = String::new();
        for (ident, wire, kind) in &names {
            let value = match (kind, suffix) {
                (Shape::Plain, "") => format!("::corten::form::FieldName::__new({wire:?})"),
                (Shape::Plain, _) => format!("::corten::form::FieldName::__at({at}, {wire:?})"),
                (Shape::Group(g), "") => format!("<{g}>::__in_group(::corten::form::__private::group({wire:?}))"),
                (Shape::Group(g), "Row") => {
                    format!("<{g}>::__in_row_group(::corten::form::__private::row_group({at}, {wire:?}))")
                }
                (Shape::Group(_), _) | (Shape::Rows(..) | Shape::Map(_), "Group" | "Row" | "RowGroup") => {
                    "::corten::form::Refused".to_string()
                }
                (Shape::Rows(..), _) => format!("::corten::form::Rows::__new({wire:?})"),
                (Shape::Map(_), _) => format!("::corten::form::Keyed::__new({wire:?})"),
            };
            fields += &format!("{ident}: {value}, ");
        }
        fields
    };
    let root_fields = build("", "");
    let group_fields = build("Group", "at");
    let row_fields = build("Row", "at");
    let row_group_fields = build("RowGroup", "at");
    out += &format!(
        "#[allow(dead_code)]\nimpl {impl_generics} {name} {type_generics} {where_clause} {{
    /// Each field's name as the form posts it.
    pub const FIELDS: {fields_type} = {{ let _ = <Self as ::corten::form::Fields>::SHAPE; {fields_type} {{ {root_fields} }} }};
    #[doc(hidden)]
    pub const fn __in_group(at: ::corten::form::Group) -> {name}GroupFields {{
        {name}GroupFields {{ {group_fields} }}
    }}
    #[doc(hidden)]
    pub const fn __in_row_group(at: ::corten::form::RowGroup<'_>) -> {name}RowGroupFields<'_> {{
        {name}RowGroupFields {{ {row_group_fields} __k: ::core::marker::PhantomData }}
    }}
}}
impl {impl_generics} ::corten::form::Fields for {name} {type_generics} {where_clause} {{
    type Names = {fields_type};
    type InGroup = {name}GroupFields;
    type InRow<'k> = {name}RowFields<'k>;
    type InRowGroup<'k> = {name}RowGroupFields<'k>;
    const SHAPE: u8 = {{ {shape} shape }};
    const LEAVE_OUT: bool = {leave_out};
    fn in_row<'k>(rows: &'static str, key: &'k (dyn ::corten::form::RowKey + ::core::marker::Sync)) -> {name}RowFields<'k> {{
        let _ = <Self as ::corten::form::Fields>::SHAPE;
        let at = ::corten::form::__private::row(rows, key);
        {name}RowFields {{ {row_fields} __k: ::core::marker::PhantomData }}
    }}
}}
"
    );
    if !files.is_empty() {
        let read: Vec<String> = files.iter().map(|f| format!("&self.{f}")).collect();
        out += &format!(
            "impl {impl_generics} ::corten::form::__FileInputs for {name} {type_generics} {where_clause} {{\n    fn __file_inputs(&self) -> usize {{\n        [{}].len()\n    }}\n}}\n",
            read.join(", ")
        );
    }
    Ok(out)
}

/// Whether a field's type is an `Option`, which a form may leave out.
fn is_option(ty: &[TokenTree]) -> bool {
    let name = ty.iter().take_while(|t| !matches!(t, TokenTree::Punct(p) if p.as_char() == '<')).filter_map(|t| match t {
        TokenTree::Ident(id) => Some(id.to_string()),
        _ => None,
    });
    name.last().as_deref() == Some("Option")
}

/// What a browser leaves out of the form when it has no value, for a field type that is a
/// `bool` or a `Vec`: the key is missing, so without a default the form fails to read.
fn left_out_when_empty(ty: &[TokenTree]) -> Option<&'static str> {
    let name = ty.iter().take_while(|t| !matches!(t, TokenTree::Punct(p) if p.as_char() == '<')).filter_map(|t| match t {
        TokenTree::Ident(id) => Some(id.to_string()),
        _ => None,
    });
    match name.last().as_deref() {
        Some("bool") if ty.len() == 1 => Some("a bool, and a browser sends nothing for an unchecked checkbox"),
        Some("Vec") => Some("a list, and a browser sends nothing for a list with nothing checked or selected"),
        _ => None,
    }
}

/// Splits a brace group's tokens at top-level commas. Commas inside `<..>` belong to a type.
/// What `#[fields(..)]` makes a field of a `Fields` struct.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Plain,
    /// `#[fields(group)]`: a struct of fields, named `field.name`.
    Group,
    /// `#[fields(rows)]`: a map from a key to a struct of fields, named `field[key].name`.
    Rows,
    /// `#[fields(map)]`: a map from a key to one plain value, named `field[key]`.
    Map,
}

/// A field's names: one, a group's (its type), or rows' (the key's type and the row's).
enum Shape {
    Plain,
    Group(String),
    Rows(String, String),
    /// `#[fields(map)]`: the key's type.
    Map(String),
}

/// `#[fields(group)]` or `#[fields(rows)]`; `None` for another attribute.
fn fields_kind(attribute: &Group) -> Result<Option<Kind>, String> {
    let tokens: Vec<TokenTree> = attribute.stream().into_iter().collect();
    match (tokens.first(), tokens.get(1)) {
        (Some(TokenTree::Ident(path)), Some(TokenTree::Group(args))) if path.to_string() == "fields" => {
            match args.stream().to_string().trim() {
                "group" => Ok(Some(Kind::Group)),
                "rows" => Ok(Some(Kind::Rows)),
                "map" => Ok(Some(Kind::Map)),
                other => Err(format!(
                    "Fields: #[fields({other})] is not one; a field is #[fields(group)], #[fields(rows)] or #[fields(map)]"
                )),
            }
        }
        (Some(TokenTree::Ident(path)), _) if path.to_string() == "fields" => {
            Err("Fields: write #[fields(group)], #[fields(rows)] or #[fields(map)]".into())
        }
        _ => Ok(None),
    }
}

/// A group's or a row's type, as written: one that does not borrow, since its names outlive the
/// form.
fn nested_type(field: &str, ty: &[TokenTree], what: &str) -> Result<String, String> {
    if ty.iter().any(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == '\'')) {
        return Err(format!("Fields: `{field}` is a {what}, and a {what}'s type cannot borrow (`'a`); give it owned fields"));
    }
    if is_option(ty) {
        return Err(format!(
            "Fields: `{field}` is a {what}, and a {what} is never an Option: a form sends its fields or leaves them out one by one; make the fields Options and give the {what} #[serde(default)]"
        ));
    }
    let text: String = ty.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return Err(format!("Fields: `{field}` has no type"));
    }
    Ok(text)
}

/// The key and row types of a rows field: `BTreeMap<Domain, DomainLimits>`.
fn map_types(field: &str, ty: &[TokenTree]) -> Result<(String, String), String> {
    let usage =
        format!("Fields: `{field}` is rows, a map from each row's key to its fields, like BTreeMap<Domain, DomainLimits>");
    let open = ty.iter().position(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == '<')).ok_or(usage.clone())?;
    let mut args = vec![Vec::new()];
    let mut depth = 0i32;
    for t in &ty[open + 1..] {
        match t {
            TokenTree::Punct(p) if p.as_char() == '<' => depth += 1,
            TokenTree::Punct(p) if p.as_char() == '>' && depth == 0 => break,
            TokenTree::Punct(p) if p.as_char() == '>' => depth -= 1,
            TokenTree::Punct(p) if p.as_char() == ',' && depth == 0 => {
                args.push(Vec::new());
                continue;
            }
            _ => {}
        }
        args.last_mut().expect("never empty").push(t.clone());
    }
    args.retain(|a| !a.is_empty());
    match args.as_slice() {
        [key, row] => Ok((nested_type(field, key, "row key")?, nested_type(field, row, "row")?)),
        _ => Err(usage),
    }
}

fn split_fields(stream: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut fields = vec![Vec::new()];
    let mut depth = 0i32;
    let mut previous_dash = false;
    for token in stream {
        if let TokenTree::Punct(p) = &token {
            match p.as_char() {
                '<' => depth += 1,
                // `->` in a fn pointer type is not a closing angle bracket.
                '>' if !previous_dash => depth -= 1,
                ',' if depth == 0 => {
                    fields.push(Vec::new());
                    previous_dash = false;
                    continue;
                }
                _ => {}
            }
            previous_dash = p.as_char() == '-' && p.spacing() == Spacing::Joint;
        } else {
            previous_dash = false;
        }
        fields.last_mut().expect("never empty").push(token);
    }
    fields.retain(|f| !f.is_empty());
    fields
}

/// `key` or `key = "value"` pairs of a `#[serde(...)]` attribute group; other attributes yield nothing.
pub(crate) fn serde_args(attribute: &Group) -> Result<Vec<(String, Option<String>)>, String> {
    let tokens: Vec<TokenTree> = attribute.stream().into_iter().collect();
    let (Some(TokenTree::Ident(path)), Some(TokenTree::Group(args))) = (tokens.first(), tokens.get(1)) else {
        return Ok(Vec::new());
    };
    if path.to_string() != "serde" {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let args: Vec<TokenTree> = args.stream().into_iter().collect();
    let mut k = 0;
    while k < args.len() {
        if let TokenTree::Ident(key) = &args[k] {
            let key = key.to_string();
            match (args.get(k + 1), args.get(k + 2)) {
                (Some(TokenTree::Punct(eq)), Some(TokenTree::Literal(value))) if eq.as_char() == '=' => {
                    out.push((key, Some(unquote(&value.to_string())?)));
                    k += 3;
                }
                (Some(TokenTree::Group(_)), _) => {
                    out.push((key, None));
                    k += 2;
                }
                _ => {
                    out.push((key, None));
                    k += 1;
                }
            }
        } else {
            k += 1;
        }
    }
    Ok(out)
}

fn unquote(literal: &str) -> Result<String, String> {
    literal
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .filter(|s| !s.contains('\\'))
        .map(str::to_string)
        .ok_or_else(|| format!("expected a plain string literal, found {literal}"))
}

/// serde's `rename_all` rules, applied to a snake_case field name.
pub(crate) fn apply_rule(field: &str, rule: &str) -> Result<String, String> {
    let words: Vec<&str> = field.split('_').filter(|w| !w.is_empty()).collect();
    let capitalized = |w: &str| {
        let mut c = w.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    };
    Ok(match rule {
        "lowercase" => field.to_lowercase(),
        "UPPERCASE" => field.to_uppercase(),
        "snake_case" => field.to_string(),
        "SCREAMING_SNAKE_CASE" => field.to_uppercase(),
        "kebab-case" => field.replace('_', "-"),
        "SCREAMING-KEBAB-CASE" => field.replace('_', "-").to_uppercase(),
        "PascalCase" => words.iter().map(|w| capitalized(w)).collect(),
        "camelCase" => {
            let pascal: String = words.iter().map(|w| capitalized(w)).collect();
            let mut c = pascal.chars();
            c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
        other => return Err(format!("unknown rename_all rule {other:?}")),
    })
}
