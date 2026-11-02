//! `tokens!`: a W3C design-token file (DTCG 2025.10, read by `dtcg`) or a resolver document
//! read at compile time into CSS custom properties, the overrides of each modifier's contexts,
//! typed scales and responsive utility classes (see `corten::style`).

use std::path::Path;

use proc_macro::{Ident, Span, TokenStream, TokenTree};

use crate::{
    dtcg,
    json::Value,
    style::class_ident,
    tokens::{err, literal, q, string_value, Error},
};

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn", "for", "if",
    "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "static", "struct", "super",
    "trait", "true", "type", "unsafe", "use", "where", "while", "yield", "gen", "try", "macro", "box", "do", "final", "override",
    "priv", "typeof", "unsized", "virtual", "abstract", "become", "base", "new",
];

struct Entry {
    key: String,
    value: String,
    variant: String,
    description: Option<String>,
}

struct Scale {
    name: String,
    /// As the file writes it: a utility's `group`.
    written: String,
    ty: String,
    entries: Vec<Entry>,
}

enum Source {
    /// A scale, by index: `gap` takes `Space` and sets `var(--space-…)`.
    Scale(usize),
    /// Its own values: `direction` takes `Direction`.
    Values(Scale),
}

struct Utility {
    name: String,
    property: String,
    source: Source,
}

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let usage = "tokens!(\"tokens.json\") or tokens!(\"tokens.resolver.json\"), with the path from the crate's root";
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let (path, span) = match tokens.as_slice() {
        [TokenTree::Literal(lit)] => (string_value(lit).ok_or_else(|| (lit.span(), usage.to_string()))?, lit.span()),
        _ => return err(Span::call_site(), usage),
    };
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let file = Path::new(&root).join(&path);
    let tokens = dtcg::read(&file).map_err(|e| (span, format!("{path}: {e}")))?;
    generate(&tokens, &path).map_err(|e| (span, format!("{path}: {e}")))
}

fn object<'a>(value: &'a Value, what: &str) -> Result<&'a [(String, Value)], String> {
    match value {
        Value::Object(entries) => Ok(entries),
        other => Err(format!("{what} is an object, not {}", other.kind())),
    }
}

/// CSS text Corten's extension gives as is: a utility's value, a media condition.
fn string<'a>(value: &'a Value, what: &str) -> Result<&'a str, String> {
    match value {
        Value::String(s) if s.contains([';', '{', '}']) => Err(format!("{what}: a value cannot contain ; {{ or }}")),
        Value::String(s) if s.trim().is_empty() => Err(format!("{what} is empty")),
        Value::String(s) => Ok(s.trim()),
        other => Err(format!("{what} is a string, not {}", other.kind())),
    }
}

/// Token, scale and utility names: lowercase letters, digits and `-`.
fn name(key: &str, what: &str) -> Result<(), String> {
    let ok = key.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !key.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err(format!("{what} {key:?}: names are lowercase letters, digits and -"))
    }
}

/// A class starts with a breakpoint or utility name, and a CSS class cannot start with a digit
/// unescaped (`.1\:gap-1` matches nothing).
fn letter_first(key: &str, what: &str) -> Result<(), String> {
    if key.starts_with(|c: char| c.is_ascii_lowercase()) {
        Ok(())
    } else {
        Err(format!("{what} {key:?}: its classes start with its name, and a class starts with a letter"))
    }
}

/// `ink` is `Ink`, `surface-2` is `Surface2`, `4` is `_4`, `2xl` is `_2xl`.
fn variant(key: &str) -> Result<String, String> {
    if key.starts_with(|c: char| c.is_ascii_digit()) {
        Ok(format!("_{}", key.replace('-', "_")))
    } else {
        class_ident(key)
    }
}

/// `padding-x` is `padding_x`; `2xl` is `_2xl`.
fn snake(key: &str, what: &str) -> Result<String, String> {
    let out = key.replace('-', "_");
    if KEYWORDS.contains(&out.as_str()) {
        return Err(format!("{what} {key:?} is a Rust keyword or a name tokens! uses; pick another"));
    }
    Ok(if out.starts_with(|c: char| c.is_ascii_digit()) { format!("_{out}") } else { out })
}

fn entries(value: &Value, what: &str) -> Result<Vec<Entry>, String> {
    let mut out: Vec<Entry> = Vec::new();
    for (key, value) in object(value, what)? {
        name(key, what)?;
        let variant = variant(key)?;
        if let Some(other) = out.iter().find(|e| e.variant == variant) {
            return Err(format!("{what}: {:?} and {key:?} are both {variant} in Rust", other.key));
        }
        out.push(Entry {
            key: key.clone(),
            value: string(value, &format!("{what}.{key}"))?.to_string(),
            variant,
            description: None,
        });
    }
    if out.is_empty() {
        return Err(format!("{what} has no entries"));
    }
    Ok(out)
}

/// A token file, checked.
struct Spec {
    breakpoints: Vec<(String, String)>,
    scales: Vec<Scale>,
    /// Tokens in no scale, as (custom property, value).
    loose: Vec<(String, String)>,
    themes: Vec<Theme>,
    utilities: Vec<Utility>,
}

/// A context's values, for its selector and, under a media condition, for the page that chose
/// no context: `[data-theme="dark"]`, and `:where(:root):not([data-theme])` (one attribute's
/// specificity, as every context's rule has, so a later modifier's rule still wins) under
/// `(prefers-color-scheme: dark)`.
struct Theme {
    selector: String,
    vars: Vec<(String, String)>,
    media: Vec<(String, String)>,
}

/// A media condition that cannot leave its block.
fn prelude(text: &str) -> bool {
    !text.contains(['{', '}', ';']) && !text.trim().is_empty()
}

fn parse(tokens: &dtcg::Tokens) -> Result<Spec, String> {
    let mut scales: Vec<Scale> = Vec::new();
    let mut loose: Vec<(String, String)> = Vec::new();
    for token in &tokens.tokens {
        let (scale, rest) = token.path.split_first().expect("a token has a path");
        // A token at the top of the file, or a top-level group's `$root`, is in no scale: a
        // custom property with no enum.
        if rest.is_empty() {
            loose.push((token.property(), token.css.clone()));
            continue;
        }
        let key = rest.join("-");
        let at = scales.iter().position(|s| s.name == *scale);
        let at = match at {
            Some(at) => at,
            None => {
                scales.push(Scale {
                    name: scale.clone(),
                    written: token.group.clone(),
                    ty: class_ident(scale)?,
                    entries: Vec::new(),
                });
                scales.len() - 1
            }
        };
        let variant = variant(&key)?;
        let entries = &mut scales[at].entries;
        if let Some(other) = entries.iter().find(|e| e.variant == variant || e.key == key) {
            return Err(format!("{scale}: {:?} and {key:?} are both {variant} in Rust (--{scale}-{key})", other.key));
        }
        entries.push(Entry { key, value: token.css.clone(), variant, description: token.description.clone() });
    }
    if scales.is_empty() && loose.is_empty() {
        return Err("there are no tokens".into());
    }
    let mut themes: Vec<Theme> = Vec::new();
    let mut attributes: Vec<(String, String)> = Vec::new();
    for modifier in &tokens.modifiers {
        let what = format!("modifier {}", modifier.name);
        let attribute = dtcg::slug(&modifier.name, "modifier")?;
        if let Some((_, other)) = attributes.iter().find(|(a, _)| *a == attribute) {
            return Err(format!("modifiers {other:?} and {:?} are both data-{attribute}; rename one", modifier.name));
        }
        attributes.push((attribute.clone(), modifier.name.clone()));
        let mut media: Vec<(String, String)> = Vec::new();
        if let Some(extension) = &modifier.extension {
            let at = format!("{what}: $extensions.\"{}\"", dtcg::VENDOR);
            for (key, value) in object(extension, &at)? {
                if key != "media" {
                    return Err(format!("{at}: unknown field {key:?}; a modifier's is media"));
                }
                for (context, condition) in object(value, &format!("{at}.media"))? {
                    if *context == modifier.default || !modifier.contexts.iter().any(|(c, _)| c == context) {
                        return Err(format!("{at}.media: {context:?} is not one of its contexts other than the default"));
                    }
                    let condition = string(condition, &format!("{at}.media.{context}"))?;
                    if !prelude(condition) {
                        return Err(format!("{at}.media: {condition:?} is not a media condition"));
                    }
                    media.push((context.clone(), condition.to_string()));
                }
            }
        }
        let mut values: Vec<(String, &str)> = Vec::new();
        for (context, vars) in &modifier.contexts {
            let value = dtcg::slug(context, &format!("{what}: context"))?;
            if let Some((_, other)) = values.iter().find(|(v, _)| *v == value) {
                return Err(format!(
                    "{what}: contexts {other:?} and {context:?} are both data-{attribute}=\"{value}\"; rename one"
                ));
            }
            values.push((value.clone(), context));
            let media = media.iter().filter(|(c, _)| c == context);
            let media =
                media.map(|(_, condition)| (condition.clone(), format!(":where(:root):not([data-{attribute}])"))).collect();
            if !vars.is_empty() {
                themes.push(Theme { selector: format!("[data-{attribute}=\"{value}\"]"), vars: vars.clone(), media });
            }
        }
    }
    let mut breakpoints: Vec<(String, String)> = Vec::new();
    let mut utilities: Vec<Utility> = Vec::new();
    let empty = Value::Object(Vec::new());
    let extension = tokens.extension.as_ref().unwrap_or(&empty);
    let top = object(extension, &format!("$extensions.\"{}\"", dtcg::VENDOR))?;
    for (key, _) in top {
        if !matches!(key.as_str(), "breakpoints" | "utilities") {
            return Err(format!(
                "$extensions.\"{}\": unknown field {key:?}; the fields are breakpoints and utilities",
                dtcg::VENDOR
            ));
        }
    }
    let section = |name: &str| top.iter().find(|(k, _)| k == name).map(|(_, v)| v);
    if let Some(value) = section("breakpoints") {
        // Each breakpoint's rules come after the smaller one's, so a wider screen's win.
        let mut last: Option<(f64, String, String)> = None;
        for (key, width) in object(value, "breakpoints")? {
            name(key, "breakpoint")?;
            letter_first(key, "breakpoint")?;
            let css = dtcg::dimension(width, &format!("breakpoints.{key}"))?;
            let unit =
                css.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '-' || c == 'e' || c == 'E' || c == '+');
            let n: f64 = css[..css.len() - unit.len()].parse().expect("a dimension's number");
            if n <= 0.0 {
                return Err(format!(
                    "breakpoints.{key}: {css} is no width; a breakpoint is wider than 0, where the rules with none apply"
                ));
            }
            if let Some((before, before_unit, before_key)) = &last {
                if *before_unit != unit {
                    return Err(format!("breakpoints: {before_key} is in {before_unit} and {key} in {unit}; a rem's px depend on the reader's font size, so they cannot be ordered"));
                }
                if n <= *before {
                    return Err(format!("breakpoints: {key} is not wider than {before_key}; they go smallest first"));
                }
            }
            last = Some((n, unit.to_string(), key.clone()));
            breakpoints.push((key.clone(), css));
        }
    }
    if let Some(value) = section("utilities") {
        for (key, spec) in object(value, "utilities")? {
            name(key, "utility")?;
            letter_first(key, "utility")?;
            let what = format!("utilities.{key}");
            let spec = object(spec, &what)?;
            let field = |f: &str| spec.iter().find(|(k, _)| k == f).map(|(_, v)| v);
            for (k, _) in spec {
                if !matches!(k.as_str(), "property" | "group" | "values") {
                    return Err(format!("{what}: unknown field {k:?}; a utility has a property and a group or values"));
                }
            }
            let property =
                string(field("property").ok_or_else(|| format!("{what} has no property"))?, &format!("{what}.property"))?;
            // A CSS property: a letter first, or one `-` and a letter (`-webkit-…`).
            let first = property.trim_start_matches('-');
            let dashes = property.len() - first.len();
            if dashes > 1
                || !first.starts_with(|c: char| c.is_ascii_lowercase())
                || !property.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            {
                return Err(format!("{what}.property {property:?} is not a CSS property name"));
            }
            let source = match (field("group"), field("values")) {
                (Some(group), None) => {
                    let group = string(group, &format!("{what}.group"))?;
                    Source::Scale(
                        scales
                            .iter()
                            .position(|s| s.written == group)
                            .ok_or_else(|| format!("{what}: there is no group {group:?} of tokens"))?,
                    )
                }
                (None, Some(values)) => Source::Values(Scale {
                    name: key.clone(),
                    written: key.clone(),
                    ty: class_ident(key)?,
                    entries: entries(values, &format!("{what}.values"))?,
                }),
                _ => return Err(format!("{what} has a group or values, one of the two")),
            };
            utilities.push(Utility { name: key.clone(), property: property.to_string(), source });
        }
    }
    let mut types: Vec<&str> = vec!["Responsive", "TOKENS"];
    for ty in scales.iter().map(|s| s.ty.as_str()).chain(utilities.iter().filter_map(|u| match &u.source {
        Source::Values(s) => Some(s.ty.as_str()),
        Source::Scale(_) => None,
    })) {
        if types.contains(&ty) {
            return Err(format!("two definitions are named {ty} in Rust; rename a group or utility"));
        }
        types.push(ty);
    }
    // Each utility's function and each of its values' own (`gap_3`, `mx_auto`) share one
    // namespace: two of one name would not compile, so the file is told which.
    let mut functions: Vec<(String, String)> = Vec::new();
    for u in &utilities {
        functions.push((snake(&u.name, "utility")?, format!("utility {}", u.name)));
    }
    for u in &utilities {
        let keys = match &u.source {
            Source::Scale(i) => &scales[*i].entries,
            Source::Values(s) => &s.entries,
        };
        for e in keys {
            let function = snake(&format!("{}-{}", u.name, e.key), "utility value")?;
            let by = format!("utilities.{} value {} (class {}-{})", u.name, e.key, u.name, e.key);
            if let Some((_, first)) = functions.iter().find(|(f, _)| *f == function) {
                return Err(format!("{first} and {by} are both {function}() in Rust; rename one"));
            }
            functions.push((function, by));
        }
    }

    Ok(Spec { breakpoints, scales, loose, themes, utilities })
}

/// The stylesheet: custom properties on `:root`, each theme (followed by its media conditions),
/// then the utilities, at every width and per breakpoint.
fn css(spec: &Spec) -> String {
    let Spec { breakpoints, scales, loose, themes, utilities } = spec;
    let mut root: Vec<String> = Vec::new();
    for scale in scales {
        for e in &scale.entries {
            root.push(format!("--{}-{}:{}", scale.name, e.key, e.value));
        }
    }
    root.extend(loose.iter().map(|(property, value)| format!("{property}:{value}")));
    let mut css = format!(":root{{{}}}", root.join(";"));
    for theme in themes {
        let body: Vec<String> = theme.vars.iter().map(|(k, v)| format!("{k}:{v}")).collect();
        let body = body.join(";");
        css.push_str(&format!("{}{{{body}}}", theme.selector));
        for (condition, selector) in &theme.media {
            css.push_str(&format!("@media {condition}{{{selector}{{{body}}}}}"));
        }
    }
    let values = |u: &Utility| -> Vec<(String, String)> {
        match &u.source {
            Source::Scale(i) => {
                scales[*i].entries.iter().map(|e| (e.key.clone(), format!("var(--{}-{})", scales[*i].name, e.key))).collect()
            }
            Source::Values(s) => s.entries.iter().map(|e| (e.key.clone(), e.value.clone())).collect(),
        }
    };
    for u in utilities {
        for (key, value) in values(u) {
            css.push_str(&format!(".{}-{key}{{{}:{value}}}", u.name, u.property));
        }
    }
    for (bp, width) in breakpoints {
        if utilities.is_empty() {
            break;
        }
        css.push_str(&format!("@media (min-width:{width}){{"));
        for u in utilities {
            for (key, value) in values(u) {
                css.push_str(&format!(".{bp}\\:{}-{key}{{{}:{value}}}", u.name, u.property));
            }
        }
        css.push('}');
    }

    css
}

fn generate(tokens: &dtcg::Tokens, path: &str) -> Result<TokenStream, String> {
    let spec = parse(tokens)?;
    let css = css(&spec);
    let Spec { breakpoints, scales, utilities, .. } = spec;
    // The Rust items.
    let span = Span::call_site();
    let id = |s: &str| -> TokenStream { TokenTree::Ident(Ident::new(s, span)).into() };
    let mut out = TokenStream::new();
    for file in &tokens.files {
        out.extend(q("const _: &str = ::core::include_str!(__A0);", &[literal(&file.display().to_string())]));
    }
    let mut enumerate = |scale: &Scale, doc: String, var: bool| {
        let mut variants = TokenStream::new();
        let mut all = TokenStream::new();
        let mut value_arms = TokenStream::new();
        let mut var_arms = TokenStream::new();
        for e in &scale.entries {
            let doc = match &e.description {
                Some(description) => format!(" `{}`: {}. {description}", e.key, e.value),
                None => format!(" `{}`: {}", e.key, e.value),
            };
            variants.extend(q("#[doc = __A0] __A1,", &[literal(&doc), id(&e.variant)]));
            all.extend(q("Self::__A0,", &[id(&e.variant)]));
            value_arms.extend(q("Self::__A0 => __A1,", &[id(&e.variant), literal(&e.value)]));
            var_arms.extend(q("Self::__A0 => __A1,", &[id(&e.variant), literal(&format!("var(--{}-{})", scale.name, e.key))]));
        }
        out.extend(q(
            "#[doc = __A0] #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub enum __A1 { __A2 }
            impl __A1 {
                /// Every value, in the file's order.
                pub const ALL: &'static [Self] = &[__A3];
                /// The value in the file (before any theme).
                pub const fn value(self) -> &'static str { match self { __A4 } }
            }",
            &[literal(&doc), id(&scale.ty), variants, all, value_arms],
        ));
        if var {
            out.extend(q(
                "impl __A0 {
                    /// The custom property, for CSS written in Rust: `var(--space-4)`.
                    pub const fn var(self) -> &'static str { match self { __A1 } }
                }",
                &[id(&scale.ty), var_arms],
            ));
        }
    };
    for scale in &scales {
        enumerate(scale, format!(" The `{}` scale of {path}: `var(--{}-…)` in CSS.", scale.name, scale.name), true);
    }
    for u in &utilities {
        if let Source::Values(s) = &u.source {
            enumerate(s, format!(" The values of the `{}` utility ({}) in {path}.", u.name, u.property), false);
        }
    }

    let mut fields = TokenStream::new();
    let mut nones = TokenStream::new();
    let mut builders = TokenStream::new();
    for (bp, width) in &breakpoints {
        let field = snake(bp, "breakpoint")?;
        fields.extend(q(
            "#[doc = __A0] pub __A1: ::core::option::Option<T>,",
            &[literal(&format!(" From {width} wide.")), id(&field)],
        ));
        nones.extend(q("__A0: ::core::option::Option::None,", &[id(&field)]));
        builders.extend(q(
            "#[doc = __A0] #[must_use] pub fn __A1(mut self, value: T) -> Self { self.__A1 = ::core::option::Option::Some(value); self }",
            &[literal(&format!(" The value from {width} wide.")), id(&field)],
        ));
    }
    out.extend(q(
        "/// A value per breakpoint; a plain value is the value at every width.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct Responsive<T> {
            /// At every width, unless a breakpoint overrides it.
            pub base: ::core::option::Option<T>,
            __A0
        }
        impl<T> Responsive<T> {
            pub const fn new(base: T) -> Self { Responsive { base: ::core::option::Option::Some(base), __A1 } }
            __A2
        }
        impl<T> ::core::default::Default for Responsive<T> {
            fn default() -> Self { Responsive { base: ::core::option::Option::None, __A1 } }
        }
        impl<T> ::core::convert::From<T> for Responsive<T> {
            fn from(base: T) -> Self { Self::new(base) }
        }",
        &[fields, nones, builders],
    ));

    let width = breakpoints.len() + 1;
    for u in &utilities {
        let ty = match &u.source {
            Source::Scale(i) => &scales[*i],
            Source::Values(s) => s,
        };
        for e in &ty.entries {
            let class = format!("{}-{}", u.name, e.key);
            let function = snake(&class, "utility value")?;
            let value = match &u.source {
                Source::Scale(_) => format!("var(--{}-{})", ty.name, e.key),
                Source::Values(_) => e.value.clone(),
            };
            let doc = format!(
                " `{class}`, `{}: {value}` at every width. A value per breakpoint, or one chosen as the page renders, is `{}(..)`.",
                u.property,
                snake(&u.name, "utility")?
            );
            out.extend(q(
                "#[doc = __A0] #[inline] pub const fn __A1() -> ::corten::style::Class { ::corten::style::Class::__new(__A2) }",
                &[literal(&doc), id(&function), literal(&class)],
            ));
        }
        let mut table = TokenStream::new();
        for e in &ty.entries {
            let mut row = literal(&format!("{}-{}", u.name, e.key));
            for (bp, _) in &breakpoints {
                row.extend(q(",", &[]));
                row.extend(literal(&format!("{bp}:{}-{}", u.name, e.key)));
            }
            table.extend(q("[__A0],", &[row]));
        }
        let mut picks = q("value.base.map(|v| NAMES[v as usize][0]),", &[]);
        for (n, (bp, _)) in breakpoints.iter().enumerate() {
            picks.extend(q(&format!("value.__A0.map(|v| NAMES[v as usize][{}]),", n + 1), &[id(&snake(bp, "breakpoint")?)]));
        }
        let doc = format!(" The `{}` classes: `{}` from the `{}` values, per breakpoint.", u.name, u.property, ty.name);
        out.extend(q(
            &format!(
                "#[doc = __A0] #[inline] pub fn __A1(value: impl ::core::convert::Into<Responsive<__A2>>) -> ::corten::style::Classes<{width}> {{
                    const NAMES: [[&str; {width}]; {}] = [__A3];
                    let value = value.into();
                    ::corten::style::Classes::__new([__A4])
                }}",
                ty.entries.len()
            ),
            &[literal(&doc), id(&snake(&u.name, "utility")?), id(&ty.ty), table, picks],
        ));
    }

    out.extend(q(
        "#[doc = __A0] pub const TOKENS: ::corten::Component = ::corten::Component {
            name: __A1,
            script: ::core::option::Option::None,
            event: ::core::option::Option::None,
            view: ::core::option::Option::None,
            style: ::core::option::Option::Some(::corten::style::Style { name: __A1, file: __A1, css: __A2, classes: &[] }),
            anchor: &0,
        };",
        &[
            literal(&format!(
                " {path} as CSS: custom properties, themes and utility classes. List it first in `App::components`."
            )),
            literal(path),
            literal(&css),
        ],
    ));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    fn spec(text: &str) -> Result<Spec, String> {
        let value = json::parse(text).unwrap();
        let tokens = if text.contains("resolutionOrder") {
            dtcg::resolver_document(&value, None, &mut Default::default())?
        } else {
            dtcg::token_file(&value)?
        };
        parse(&tokens)
    }

    fn run(text: &str) -> Result<String, String> {
        spec(text).map(|spec| css(&spec))
    }

    const SPACE: &str = r#""space": { "$type": "dimension",
        "1": { "$value": { "value": 4, "unit": "px" } }, "2": { "$value": { "value": 8, "unit": "px" } } }"#;
    const COLOR: &str =
        r#""color": { "$type": "color", "ink": { "$value": { "colorSpace": "srgb", "components": [0, 0, 0] } } }"#;

    #[test]
    fn tokens_become_custom_properties_and_utilities() {
        let css = run(&format!(
            r#"{{ {SPACE}, {COLOR}, "$extensions": {{ "run.corten": {{
                "breakpoints": {{ "md": {{ "value": 48, "unit": "rem" }} }},
                "utilities": {{
                    "gap": {{ "property": "gap", "group": "space" }},
                    "direction": {{ "property": "flex-direction", "values": {{ "row": "row" }} }}
                }}
            }} }} }}"#
        ))
        .unwrap();
        assert_eq!(
            css,
            concat!(
                ":root{--space-1:4px;--space-2:8px;--color-ink:#000000}",
                ".gap-1{gap:var(--space-1)}.gap-2{gap:var(--space-2)}.direction-row{flex-direction:row}",
                "@media (min-width:48rem){.md\\:gap-1{gap:var(--space-1)}.md\\:gap-2{gap:var(--space-2)}.md\\:direction-row{flex-direction:row}}"
            )
        );
    }

    #[test]
    fn each_context_is_a_data_attribute_and_one_can_follow_a_media_condition() {
        let css = run(&format!(
            r#"{{
                "version": "2025.10",
                "resolutionOrder": [
                    {{ "type": "set", "name": "base", "sources": [{{ {SPACE}, {COLOR} }}] }},
                    {{ "type": "modifier", "name": "theme", "default": "light",
                       "contexts": {{ "light": [], "dark": [{{ "color": {{ "ink": {{ "$value": {{ "colorSpace": "srgb", "components": [1, 1, 1] }} }} }} }}] }},
                       "$extensions": {{ "run.corten": {{ "media": {{ "dark": "(prefers-color-scheme: dark)" }} }} }} }},
                    {{ "type": "modifier", "name": "density", "default": "comfortable",
                       "contexts": {{ "comfortable": [], "compact": [{{ "space": {{ "2": {{ "$value": "{{space.1}}" }} }} }}], "same": [] }} }}
                ]
            }}"#
        ))
        .unwrap();
        assert_eq!(
            css,
            concat!(
                ":root{--space-1:4px;--space-2:8px;--color-ink:#000000}",
                "[data-theme=\"light\"]{--color-ink:#000000}",
                "[data-theme=\"dark\"]{--color-ink:#ffffff}",
                "@media (prefers-color-scheme: dark){:where(:root):not([data-theme]){--color-ink:#ffffff}}",
                "[data-density=\"comfortable\"]{--space-2:8px}",
                "[data-density=\"compact\"]{--space-2:var(--space-1)}"
            ),
            "each context under its attribute, the default with what the others change so it applies inside them; under the condition when the page chose none; a context that changes nothing has no rule"
        );
    }

    #[test]
    fn names_from_a_design_tool_and_tokens_in_no_scale() {
        let css = run(r#"{
                "version": "2025.10",
                "resolutionOrder": [
                    { "type": "set", "name": "base", "sources": [{
                        "Brand Color": { "$type": "number", "$root": { "$value": 1 }, "Primary Dark": { "$value": 2 } },
                        "lineHeight": { "$type": "number", "$value": 1.5 }
                    }] },
                    { "type": "modifier", "name": "Color Mode", "default": "Light",
                       "contexts": { "Light": [], "High Contrast": [{ "lineHeight": { "$type": "number", "$value": 2 } }] } }
                ]
            }"#)
        .unwrap();
        assert_eq!(
            css,
            concat!(
                ":root{--brand-color-primary-dark:2;--brand-color:1;--line-height:1.5}",
                "[data-color-mode=\"light\"]{--line-height:1.5}",
                "[data-color-mode=\"high-contrast\"]{--line-height:2}"
            ),
            "a top-level token and a top-level group's $root are custom properties with no enum"
        );
        let spec =
            spec(r#"{"Brand Color": {"$type": "number", "$root": {"$value": 1}, "Primary Dark": {"$value": 2}}}"#).unwrap();
        assert_eq!((spec.scales[0].ty.as_str(), spec.scales[0].entries[0].variant.as_str()), ("BrandColor", "PrimaryDark"));
    }

    #[test]
    fn the_compile_fail_examples_fail_for_their_reasons() {
        // What `tokens!` does with a file: read it, then parse what it read.
        let error = |file: &str| {
            let file = Path::new("../corten/src/style/tokens").join(file);
            dtcg::read(&file).and_then(|tokens| parse(&tokens).map(|_| ())).expect_err("an error")
        };
        for (file, reason) in [
            ("retypes-a-token.resolver.json", "color.ink is dimension, and color by default"),
            ("one-css-name.tokens.json", "color.font-size and color.FONT-SIZE are both --color-font-size in CSS"),
            ("breakpoints-out-of-order.tokens.json", "md is not wider than lg"),
            ("no-default.resolver.json", "has no default"),
            ("gradient.tokens.json", "a gradient token has no direction"),
            ("circular-reference.tokens.json", "go round in a circle"),
            ("reference-to-a-group.tokens.json", "{brand.ratio} is not a token"),
            ("typography.tokens.json", "a typography token is several CSS properties"),
            ("top-level-root.tokens.json", "$root at the top of the file"),
            ("empty-order.resolver.json", "resolutionOrder is empty"),
            ("too-large.tokens.json", "too large for CSS"),
            ("too-deep.tokens.json", "nest more than 128 deep"),
            ("many-contexts.resolver.json", "289 combinations of contexts"),
            ("long-pointer-chain.tokens.json", "a chain of more than 256 JSON pointers"),
            ("repeated-sets.resolver.json", "more than 100000 references"),
            ("absolute-path.resolver.json", "not a path relative to the file that names it"),
            ("digit-breakpoint.tokens.json", "a class starts with a letter"),
            ("two-modifiers.resolver.json", "--space-gap resolves to 12px"),
            ("media-for-the-default.resolver.json", "other than the default"),
        ] {
            let error = error(file);
            assert!(error.contains(reason), "{file}: {error}");
        }
    }

    #[test]
    fn the_compile_fail_example_fails_for_its_reason() {
        let error = dtcg::read(Path::new("../corten/src/style/tokens/adds-a-token.resolver.json")).err().expect("an error");
        assert!(error.contains("context dark defines color.link, which the defaults do not"), "{error}");
    }

    #[test]
    fn mistakes_are_errors_that_name_the_token() {
        let error = |text: &str| run(text).expect_err("an error");
        let resolver = |modifier: &str| {
            format!(
                r#"{{"version": "2025.10", "resolutionOrder": [{{"type": "set", "name": "base", "sources": [{{ {COLOR} }}]}}, {{"type": "modifier", "name": "theme", "default": "light", {modifier}}}]}}"#
            )
        };
        let dark = r#""contexts": {"light": [], "dark": [{"color": {"inc": {"$value": "{color.ink}"}}}]}"#;
        assert!(error(&resolver(dark)).contains("context dark defines color.inc, which the defaults do not"));
        let media = |media: &str| {
            let contexts = r#""contexts": {"light": [], "dark": []}"#;
            error(&resolver(&format!(r#"{contexts}, "$extensions": {{"run.corten": {{"media": {media}}}}}"#)))
        };
        assert!(media(r#"{"dark": "(prefers-color-scheme: dark) { a"}"#).contains("cannot contain"));
        assert!(media(r#"{"light": "(prefers-color-scheme: light)"}"#).contains("other than the default"));
        assert!(media(r#"{"dim": "(prefers-color-scheme: dark)"}"#).contains("other than the default"));
        assert!(error(&resolver(r#""contexts": {"light": [], "Dark": [], "dark": []}"#))
            .contains(r#"contexts "Dark" and "dark" are both data-theme="dark""#));
        assert!(error(r#"{"g": {"$type": "number", "a-b": {"$value": 1}, "a": {"b": {"$value": 2}}}}"#)
            .contains("g.a-b and g.a.b are both --g-a-b in CSS"));
        assert!(error(r#"{"g": {"$type": "number", "a": {"$value": 1}, "A": {"$value": 2}}}"#).contains("both --g-a in CSS"));
        let utility =
            |utilities: &str| error(&format!(r#"{{ {SPACE}, "$extensions": {{"run.corten": {{"utilities": {utilities}}}}} }}"#));
        assert!(utility(r#"{"gap": {"property": "gap", "group": "spacing"}}"#).contains(r#"no group "spacing""#));
        assert!(utility(r#"{"gap": {"property": "gap", "scale": "space"}}"#).contains(r#"unknown field "scale""#));
        // `border`'s value `b` would be `border_b()`, which is also the `border-b` utility.
        let clash = utility(
            r#"{"border-b": {"property": "border-block-end", "values": {"line": "1px solid"}},
                "border": {"property": "border", "values": {"b": "1px solid"}}}"#,
        );
        assert!(
            clash.contains("utility border-b and utilities.border value b (class border-b) are both border_b() in Rust"),
            "{clash}"
        );
        // Two values whose classes are the same, `.p-x-1`, where CSS would let the later rule win.
        let twice = utility(
            r#"{"p": {"property": "padding", "values": {"x-1": "4px"}}, "p-x": {"property": "padding-inline", "group": "space"}}"#,
        );
        assert!(twice.contains("utilities.p value x-1 (class p-x-1) and utilities.p-x value 1 (class p-x-1)"), "{twice}");
        assert!(utility(r#"{"w": {"property": "width", "values": {"full": "100%; color: red"}}}"#).contains("cannot contain"));
        assert!(error(r#"{"self": {"$type": "number", "a": {"$value": 1}}}"#).contains("no Rust name"));
        let breakpoint = |width: &str| {
            error(&format!(r#"{{ {SPACE}, "$extensions": {{"run.corten": {{"breakpoints": {{"md": {width}}}}}}} }}"#))
        };
        assert!(breakpoint(r#""48rem""#).contains("is an object, not a string"));
        let breakpoints =
            |widths: &str| error(&format!(r#"{{ {SPACE}, "$extensions": {{"run.corten": {{"breakpoints": {widths}}}}} }}"#));
        assert!(breakpoints(r#"{"lg": {"value": 64, "unit": "rem"}, "md": {"value": 48, "unit": "rem"}}"#)
            .contains("md is not wider than lg; they go smallest first"));
        assert!(breakpoints(r#"{"sm": {"value": 640, "unit": "px"}, "md": {"value": 48, "unit": "rem"}}"#)
            .contains("cannot be ordered"));
        assert!(breakpoints(r#"{"1": {"value": 40, "unit": "rem"}}"#).contains("a class starts with a letter"));
        assert!(utility(r#"{"2col": {"property": "gap", "group": "space"}}"#).contains("a class starts with a letter"));
        for property in ["-", "--", "--x", "-1x", "x_y"] {
            let wrong = utility(&format!(r#"{{"x": {{"property": "{property}", "group": "space"}}}}"#));
            assert!(wrong.contains("is not a CSS property name"), "{property}: {wrong}");
        }
        assert!(breakpoints(r#"{"sm": {"value": -1, "unit": "px"}}"#).contains("is no width; a breakpoint is wider than 0"));
        assert!(breakpoints(r#"{"sm": {"value": 0, "unit": "rem"}}"#).contains("is no width"));
        // A utility's group is the group as the file writes it, which CSS writes `brand-space`.
        let written = run(
            r#"{ "Brand Space": {"$type": "dimension", "1": {"$value": {"value": 4, "unit": "px"}}},
                "$extensions": {"run.corten": {"utilities": {"m": {"property": "-webkit-margin-start", "group": "Brand Space"}}}} }"#
        )
        .unwrap();
        assert!(written.contains(".m-1{-webkit-margin-start:var(--brand-space-1)}"), "{written}");
        assert!(error(
            r#"{ "Brand Space": {"$type": "dimension", "1": {"$value": {"value": 4, "unit": "px"}}},
            "$extensions": {"run.corten": {"utilities": {"m": {"property": "margin", "group": "brand-space"}}}} }"#
        )
        .contains(r#"no group "brand-space""#));
        assert!(error(&format!(r#"{{ {SPACE}, "$extensions": {{"run.corten": {{"scales": {{}}}}}} }}"#))
            .contains("unknown field \"scales\""));
    }
}
