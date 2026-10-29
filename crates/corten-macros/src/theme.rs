//! `tokens!`: a design-token file read at compile time into CSS custom properties, theme
//! overrides, typed scales and responsive utility classes (see `corten::style`).

use std::path::Path;

use proc_macro::{Ident, Span, TokenStream, TokenTree};

use crate::{
    json::{self, Value},
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
}

struct Scale {
    name: String,
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
    let usage = "tokens!(\"tokens.json\"), with the path from the crate's root";
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let (path, span) = match tokens.as_slice() {
        [TokenTree::Literal(lit)] => (string_value(lit).ok_or_else(|| (lit.span(), usage.to_string()))?, lit.span()),
        _ => return err(Span::call_site(), usage),
    };
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let file = Path::new(&root).join(&path);
    let text = std::fs::read_to_string(&file).map_err(|e| (span, format!("cannot read {}: {e}", file.display())))?;
    let value = json::parse(&text).map_err(|e| (span, format!("{path}: {e}")))?;
    generate(&value, &file, &path).map_err(|e| (span, format!("{path}: {e}")))
}

fn object<'a>(value: &'a Value, what: &str) -> Result<&'a [(String, Value)], String> {
    match value {
        Value::Object(entries) => Ok(entries),
        Value::String(_) => Err(format!("{what} is an object, not a string")),
        Value::Other(kind) => Err(format!("{what} is an object, not {kind}")),
    }
}

fn string<'a>(value: &'a Value, what: &str) -> Result<&'a str, String> {
    match value {
        Value::String(s) if s.contains([';', '{', '}']) => Err(format!("{what}: a value cannot contain ; {{ or }}")),
        Value::String(s) if s.trim().is_empty() => Err(format!("{what} is empty")),
        Value::String(s) => Ok(s.trim()),
        Value::Object(_) => Err(format!("{what} is a string, not an object")),
        Value::Other(kind) => Err(format!("{what} is a string, not {kind}")),
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
        out.push(Entry { key: key.clone(), value: string(value, &format!("{what}.{key}"))?.to_string(), variant });
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
    themes: Vec<Theme>,
    utilities: Vec<Utility>,
}

/// A theme's overrides, for its selector and, under each media condition, another selector:
/// `"@media": {"(prefers-color-scheme: dark)": ":root:not([data-theme=\"light\"])"}` applies the
/// dark values when the system asks for them and the page did not choose light.
struct Theme {
    selector: String,
    vars: Vec<(String, String)>,
    media: Vec<(String, String)>,
}

/// A selector or media condition that cannot leave its block.
fn prelude(text: &str) -> bool {
    !text.contains(['{', '}', ';']) && !text.trim().is_empty()
}

fn parse(root: &Value) -> Result<Spec, String> {
    let mut breakpoints: Vec<(String, String)> = Vec::new();
    let mut scales: Vec<Scale> = Vec::new();
    let mut themes: Vec<Theme> = Vec::new();
    let mut utilities: Vec<Utility> = Vec::new();
    let top = object(root, "the file")?;
    for (key, _) in top {
        if !matches!(key.as_str(), "breakpoints" | "scales" | "themes" | "utilities") {
            return Err(format!("unknown section {key:?}: the sections are breakpoints, scales, themes and utilities"));
        }
    }
    let section = |name: &str| top.iter().find(|(k, _)| k == name).map(|(_, v)| v);
    if let Some(value) = section("breakpoints") {
        for (key, width) in object(value, "breakpoints")? {
            name(key, "breakpoint")?;
            breakpoints.push((key.clone(), string(width, &format!("breakpoints.{key}"))?.to_string()));
        }
    }
    let Some(value) = section("scales") else { return Err("there is no \"scales\" section".into()) };
    for (key, value) in object(value, "scales")? {
        name(key, "scale")?;
        scales.push(Scale { name: key.clone(), ty: class_ident(key)?, entries: entries(value, &format!("scales.{key}"))? });
    }
    if let Some(value) = section("themes") {
        for (selector, overrides) in object(value, "themes")? {
            if !prelude(selector) {
                return Err(format!("themes: {selector:?} is not a selector"));
            }
            let mut vars = Vec::new();
            let mut media = Vec::new();
            for (scale_name, values) in object(overrides, &format!("themes.{selector}"))? {
                if scale_name == "@media" {
                    let what = format!("themes.{selector}.@media");
                    for (condition, target) in object(values, &what)? {
                        if !prelude(condition) {
                            return Err(format!("{what}: {condition:?} is not a media condition"));
                        }
                        // A selector, which `string` keeps inside its block.
                        media.push((condition.clone(), string(target, &format!("{what}.{condition}"))?.to_string()));
                    }
                    continue;
                }
                let scale = scales
                    .iter()
                    .find(|s| s.name == *scale_name)
                    .ok_or_else(|| format!("themes.{selector}: there is no scale {scale_name:?}"))?;
                for (key, value) in object(values, &format!("themes.{selector}.{scale_name}"))? {
                    if !scale.entries.iter().any(|e| e.key == *key) {
                        return Err(format!("themes.{selector}.{scale_name}: {key:?} is not in scales.{scale_name}"));
                    }
                    vars.push((
                        format!("--{scale_name}-{key}"),
                        string(value, &format!("themes.{selector}.{scale_name}.{key}"))?.to_string(),
                    ));
                }
            }
            themes.push(Theme { selector: selector.clone(), vars, media });
        }
    }
    if let Some(value) = section("utilities") {
        for (key, spec) in object(value, "utilities")? {
            name(key, "utility")?;
            let what = format!("utilities.{key}");
            let spec = object(spec, &what)?;
            let field = |f: &str| spec.iter().find(|(k, _)| k == f).map(|(_, v)| v);
            for (k, _) in spec {
                if !matches!(k.as_str(), "property" | "scale" | "values") {
                    return Err(format!("{what}: unknown field {k:?}; a utility has a property and a scale or values"));
                }
            }
            let property =
                string(field("property").ok_or_else(|| format!("{what} has no property"))?, &format!("{what}.property"))?;
            if !property.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                return Err(format!("{what}.property {property:?} is not a CSS property name"));
            }
            let source = match (field("scale"), field("values")) {
                (Some(scale), None) => {
                    let scale = string(scale, &format!("{what}.scale"))?;
                    Source::Scale(
                        scales
                            .iter()
                            .position(|s| s.name == scale)
                            .ok_or_else(|| format!("{what}: there is no scale {scale:?}"))?,
                    )
                }
                (None, Some(values)) => Source::Values(Scale {
                    name: key.clone(),
                    ty: class_ident(key)?,
                    entries: entries(values, &format!("{what}.values"))?,
                }),
                _ => return Err(format!("{what} has a scale or values, one of the two")),
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
            return Err(format!("two definitions are named {ty} in Rust; rename a scale or utility"));
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

    Ok(Spec { breakpoints, scales, themes, utilities })
}

/// The stylesheet: custom properties on `:root`, each theme (followed by its media conditions),
/// then the utilities, at every width and per breakpoint.
fn css(spec: &Spec) -> String {
    let Spec { breakpoints, scales, themes, utilities } = spec;
    let mut css = String::from(":root{");
    for scale in scales {
        for e in &scale.entries {
            css.push_str(&format!("--{}-{}:{};", scale.name, e.key, e.value));
        }
    }
    css.pop();
    css.push('}');
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

fn generate(root: &Value, file: &Path, path: &str) -> Result<TokenStream, String> {
    let spec = parse(root)?;
    let css = css(&spec);
    let Spec { breakpoints, scales, utilities, .. } = spec;
    // The Rust items.
    let span = Span::call_site();
    let id = |s: &str| -> TokenStream { TokenTree::Ident(Ident::new(s, span)).into() };
    let mut out = q("const _: &str = ::core::include_str!(__A0);", &[literal(&file.display().to_string())]);
    let mut enumerate = |scale: &Scale, doc: String, var: bool| {
        let mut variants = TokenStream::new();
        let mut all = TokenStream::new();
        let mut value_arms = TokenStream::new();
        let mut var_arms = TokenStream::new();
        for e in &scale.entries {
            variants.extend(q("#[doc = __A0] __A1,", &[literal(&format!(" `{}`: {}", e.key, e.value)), id(&e.variant)]));
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

    fn run(text: &str) -> Result<String, String> {
        parse(&json::parse(text).unwrap()).map(|spec| css(&spec))
    }

    #[test]
    fn tokens_become_custom_properties_themes_and_utilities() {
        let css = run(r##"{
            "breakpoints": { "md": "48rem" },
            "scales": { "space": { "1": "4px", "2": "8px" }, "color": { "ink": "#111" } },
            "themes": { "[data-theme=\"dark\"]": { "color": { "ink": "#eee" } } },
            "utilities": {
                "gap": { "property": "gap", "scale": "space" },
                "direction": { "property": "flex-direction", "values": { "row": "row" } }
            }
        }"##)
        .unwrap();
        assert_eq!(
            css,
            concat!(
                ":root{--space-1:4px;--space-2:8px;--color-ink:#111}",
                "[data-theme=\"dark\"]{--color-ink:#eee}",
                ".gap-1{gap:var(--space-1)}.gap-2{gap:var(--space-2)}.direction-row{flex-direction:row}",
                "@media (min-width:48rem){.md\\:gap-1{gap:var(--space-1)}.md\\:gap-2{gap:var(--space-2)}.md\\:direction-row{flex-direction:row}}"
            )
        );
    }

    #[test]
    fn a_theme_also_applies_under_its_media_conditions() {
        let css = run(r##"{
            "scales": { "color": { "ink": "#111", "bg": "#fff" } },
            "themes": { "[data-theme=\"dark\"]": {
                "@media": { "(prefers-color-scheme: dark)": ":root:not([data-theme=\"light\"])" },
                "color": { "ink": "#eee", "bg": "#000" }
            } }
        }"##)
        .unwrap();
        assert_eq!(
            css,
            concat!(
                ":root{--color-ink:#111;--color-bg:#fff}",
                "[data-theme=\"dark\"]{--color-ink:#eee;--color-bg:#000}",
                "@media (prefers-color-scheme: dark){:root:not([data-theme=\"light\"]){--color-ink:#eee;--color-bg:#000}}"
            ),
            "the values once in the file, under the selector and under the condition"
        );
    }

    #[test]
    fn mistakes_are_errors_that_name_the_token() {
        let error = |text: &str| run(text).expect_err("an error");
        assert!(error(
            r##"{"scales": {"color": {"ink": "#000"}}, "themes": {"[data-theme=dark]": {"color": {"inc": "#fff"}}}}"##
        )
        .contains(r##""inc" is not in scales.color"##));
        assert!(error(r##"{"scales": {"color": {"ink": "red; }"}}}"##).contains("cannot contain"));
        assert!(error(r##"{"scales": {"Color": {"ink": "red"}}}"##).contains("lowercase"));
        assert!(error(r##"{"scales": {"space": {"1": "4px"}}, "utilities": {"gap": {"property": "gap", "scale": "spacing"}}}"##)
            .contains(r##"no scale "spacing""##));
        assert!(error(r##"{"scale": {}}"##).contains("unknown section"));
        // `border`'s value `b` would be `border_b()`, which is also the `border-b` utility.
        let clash = error(
            r##"{"scales": {"space": {"1": "4px"}}, "utilities": {
                "border-b": {"property": "border-block-end", "values": {"line": "1px solid"}},
                "border": {"property": "border", "values": {"b": "1px solid"}}
            }}"##,
        );
        assert!(
            clash.contains("utility border-b and utilities.border value b (class border-b) are both border_b() in Rust"),
            "{clash}"
        );
        // Two values whose classes are the same, `.p-x-1`, where CSS would let the later rule win.
        let twice = error(
            r##"{"scales": {"space": {"1": "4px"}}, "utilities": {
                "p": {"property": "padding", "values": {"x-1": "4px"}},
                "p-x": {"property": "padding-inline", "scale": "space"}
            }}"##,
        );
        assert!(twice.contains("utilities.p value x-1 (class p-x-1) and utilities.p-x value 1 (class p-x-1)"), "{twice}");
        assert!(error(r##"{"scales": {"self": {"a": "1"}}}"##).contains("no Rust name"));
        let media = |media: &str| {
            error(&format!(
                r##"{{"scales": {{"color": {{"ink": "#000"}}}}, "themes": {{"[data-dark]": {{"@media": {media}}}}}}}"##
            ))
        };
        assert!(media(r##"{"(prefers-color-scheme: dark) { a": ":root"}"##).contains("is not a media condition"));
        assert!(media(r##"{"(prefers-color-scheme: dark)": ":root{}"}"##).contains("cannot contain"));
        assert!(media(r##"{"(prefers-color-scheme: dark)": {"color": {}}}"##).contains("is a string, not an object"));
    }
}
