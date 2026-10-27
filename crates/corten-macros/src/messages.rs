//! `messages!`: message catalogs read at compile time into a type with one method per key.
//!
//! Every locale must have every key of the first, with the same placeholders, or the build fails.
//! Each message is split into text and arguments here, so rendering one is a few string pushes.

use std::collections::BTreeSet;

use proc_macro::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};

use crate::{
    derive::string,
    json::{self, Value},
    tokens::{err, escape, is_ident, is_punct, q, stream, string_value, Error},
};

const RESERVED: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "const",
    "continue",
    "crate",
    "dyn",
    "else",
    "enum",
    "extern",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "in",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "pub",
    "ref",
    "return",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "trait",
    "true",
    "type",
    "unsafe",
    "use",
    "where",
    "while",
    "yield",
    "abstract",
    "become",
    "box",
    "do",
    "final",
    "gen",
    "macro",
    "override",
    "priv",
    "try",
    "typeof",
    "unsized",
    "virtual",
    "locale",
    "index",
    "from_index",
    "negotiate",
    "js",
    "by_key",
    "number",
    "money",
    "date",
    "time",
    "date_time",
];

#[derive(Debug, PartialEq)]
pub enum Part {
    Text(String),
    Arg(String),
}

struct Locale {
    tag: String,
    span: Span,
    file: String,
    messages: Vec<(String, Vec<Part>)>,
    /// Its number, currency and date data, as the tokens of a `LocaleData`.
    data: &'static str,
}

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let usage = "expected: messages! { pub struct Msg { \"en\" => \"locales/en.json\", \"fr\" => \"locales/fr.json\" } }";
    let mut i = 0;
    let mut attrs = TokenStream::new();
    while is_punct(tokens.get(i), '#') {
        attrs.extend(tokens[i..i + 2].iter().cloned());
        i += 2;
    }
    let mut vis = TokenStream::new();
    if is_ident(tokens.get(i), "pub") {
        vis.extend([tokens[i].clone()]);
        i += 1;
        if let Some(TokenTree::Group(g)) = tokens.get(i) {
            if g.delimiter() == Delimiter::Parenthesis {
                vis.extend([tokens[i].clone()]);
                i += 1;
            }
        }
    }
    if !is_ident(tokens.get(i), "struct") {
        return err(tokens.get(i).map_or(Span::call_site(), TokenTree::span), usage);
    }
    let (Some(TokenTree::Ident(name)), Some(TokenTree::Group(body))) = (tokens.get(i + 1), tokens.get(i + 2)) else {
        return err(Span::call_site(), usage);
    };
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let mut locales = Vec::new();
    let entries: Vec<TokenTree> = body.stream().into_iter().collect();
    let mut j = 0;
    while j < entries.len() {
        let (Some(TokenTree::Literal(tag)), true, true, Some(TokenTree::Literal(path))) =
            (entries.get(j), is_punct(entries.get(j + 1), '='), is_punct(entries.get(j + 2), '>'), entries.get(j + 3))
        else {
            return err(entries[j].span(), usage);
        };
        let (Some(tag_text), Some(path_text)) = (string_value(tag), string_value(path)) else {
            return err(tag.span(), usage);
        };
        if tag_text.is_empty() || !tag_text.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return err(tag.span(), format!("{tag_text:?} is not a language tag like \"en\" or \"pt-BR\""));
        }
        let file = std::path::Path::new(&root).join(&path_text);
        let text = std::fs::read_to_string(&file).map_err(|e| (path.span(), format!("cannot read {}: {e}", file.display())))?;
        let value = json::parse(&text).map_err(|e| (path.span(), format!("{path_text}: {e}")))?;
        let mut flat = Vec::new();
        flatten("", value, &mut flat).map_err(|e| (path.span(), format!("{path_text}: {e}")))?;
        let mut messages = Vec::with_capacity(flat.len());
        for (key, message) in flat {
            let parts = parse_message(&message).map_err(|e| (path.span(), format!("{path_text}: {key:?}: {e}")))?;
            messages.push((key, parts));
        }
        if locales.iter().any(|l: &Locale| l.tag == tag_text) {
            return err(tag.span(), format!("{tag_text:?} is listed twice"));
        }
        let data = locale_data(&tag_text).map_err(|e| (tag.span(), e))?;
        locales.push(Locale { tag: tag_text, span: path.span(), file: file.display().to_string(), messages, data });
        j += 4;
        if is_punct(entries.get(j), ',') {
            j += 1;
        }
    }
    if locales.is_empty() {
        return err(body.span(), usage);
    }
    if locales.len() > 255 {
        return err(body.span(), "at most 255 locales");
    }
    check(&locales)?;

    let mut methods = TokenStream::new();
    // The messages with arguments again, unfilled, for a script to fill in: `t.js().key()`.
    let mut js_methods = TokenStream::new();
    // `by_key`'s arms: a catalog key to its pieces in this locale.
    let mut key_arms = TokenStream::new();
    let mut seen = BTreeSet::new();
    let default = &locales[0];
    for (key, parts) in &default.messages {
        let method = method_name(key).map_err(|e| (default.span, e))?;
        if !seen.insert(method.clone()) {
            return err(default.span, format!("two keys become the method {method}; rename one of them"));
        }
        let args = arg_names(parts);
        let doc = format!(" {}: {}", default.tag, display(parts));
        let pieces_for = |locale: &Locale| -> TokenStream {
            let parts = &locale.messages.iter().find(|(k, _)| k == key).expect("checked").1;
            let mut list = TokenStream::new();
            for part in parts {
                list.extend(match part {
                    Part::Text(text) => q(
                        "::corten::i18n::__private::Piece::Text(__A0, __A1),",
                        &[string(text).into(), string(&escape(text)).into()],
                    ),
                    Part::Arg(name) => q("::corten::i18n::__private::Piece::Arg(__A0),", &[string(name).into()]),
                });
            }
            q("&[__A0]", &[list])
        };
        let pieces = if locales.len() == 1 {
            pieces_for(default)
        } else {
            let mut arms = TokenStream::new();
            for (n, locale) in locales.iter().enumerate().skip(1) {
                arms.extend(q(
                    "__A0 => __A1,",
                    &[TokenTree::Literal(proc_macro::Literal::u8_unsuffixed(n as u8)).into(), pieces_for(locale)],
                ));
            }
            arms.extend(q("_ => __A0,", &[pieces_for(default)]));
            q("match self.0 __A0", &[stream([TokenTree::Group(Group::new(Delimiter::Brace, arms))])])
        };
        key_arms.extend(q("__A0 => __A1,", &[string(key).into(), pieces]));
        let body_for = |locale: &Locale| -> TokenStream {
            let parts = &locale.messages.iter().find(|(k, _)| k == key).expect("checked").1;
            if args.is_empty() {
                return stream([string(&plain_text(parts))]);
            }
            let mut body = TokenStream::new();
            for part in parts {
                match part {
                    Part::Text(text) => {
                        body.extend(q("__corten_sink.text(__A0, __A1);", &[string(text).into(), string(&escape(text)).into()]))
                    }
                    Part::Arg(arg) => body.extend(q("__corten_sink.arg(&__A0);", &[arg_ident(arg).into()])),
                }
            }
            stream([TokenTree::Group(Group::new(Delimiter::Brace, body))])
        };
        let mut by_locale = if locales.len() == 1 {
            body_for(default)
        } else {
            let mut arms = TokenStream::new();
            for (n, locale) in locales.iter().enumerate().skip(1) {
                arms.extend(q(
                    "__A0 => __A1,",
                    &[TokenTree::Literal(proc_macro::Literal::u8_unsuffixed(n as u8)).into(), body_for(locale)],
                ));
            }
            arms.extend(q("_ => __A0,", &[body_for(default)]));
            q("match self.0 __A0", &[stream([TokenTree::Group(Group::new(Delimiter::Brace, arms))])])
        };
        if !args.is_empty() {
            let wire_for = |locale: &Locale| -> TokenStream {
                let parts = &locale.messages.iter().find(|(k, _)| k == key).expect("checked").1;
                stream([string(&wire(parts))])
            };
            let by_locale = if locales.len() == 1 {
                wire_for(default)
            } else {
                let mut arms = TokenStream::new();
                for (n, locale) in locales.iter().enumerate().skip(1) {
                    arms.extend(q(
                        "__A0 => __A1,",
                        &[TokenTree::Literal(proc_macro::Literal::u8_unsuffixed(n as u8)).into(), wire_for(locale)],
                    ));
                }
                arms.extend(q("_ => __A0,", &[wire_for(default)]));
                q("match self.0 __A0", &[stream([TokenTree::Group(Group::new(Delimiter::Brace, arms))])])
            };
            let names: Vec<&str> = args.iter().map(String::as_str).collect();
            let quoted: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
            let js_doc =
                format!(" {}: {}, for a handler whose JSDoc takes Message<{}>.", default.tag, display(parts), quoted.join(" | "));
            js_methods.extend(q(
                "#[doc = __A0] #[inline] __A1 const fn __A2(self) -> ::corten::i18n::Template<__A3> { ::corten::i18n::Template::new(__A4) }",
                &[
                    string(&js_doc).into(),
                    vis.clone(),
                    ident(&method).into(),
                    TokenTree::Literal(proc_macro::Literal::u64_suffixed(crate::script::message_id(&names))).into(),
                    by_locale,
                ],
            ));
        }
        if args.is_empty() {
            methods.extend(q(
                "#[doc = __A0] #[inline] __A1 const fn __A2(self) -> &'static str { __A3 }",
                &[string(&doc).into(), vis.clone(), ident(&method).into(), by_locale],
            ));
        } else {
            let mut params = TokenStream::new();
            for arg in &args {
                params.extend(q("__A0: impl ::std::fmt::Display,", &[arg_ident(arg).into()]));
            }
            by_locale = q(
                "::corten::i18n::Text::new(move |__corten_sink: &mut ::corten::i18n::Sink<'_>| __A0)",
                &[stream([TokenTree::Group(Group::new(Delimiter::Brace, by_locale))])],
            );
            methods.extend(q(
                "#[doc = __A0] #[inline] __A1 fn __A2(self, __A3) -> ::corten::i18n::Text<impl Fn(&mut ::corten::i18n::Sink<'_>)> { __A4 }",
                &[
                    string(&doc).into(),
                    vis.clone(),
                    ident(&method).into(),
                    params,
                    by_locale,
                ],
            ));
        }
    }

    let mut tags = TokenStream::new();
    let mut files = TokenStream::new();
    for locale in &locales {
        tags.extend(q("__A0,", &[string(&locale.tag).into()]));
        files.extend(q("const _: &[u8] = ::core::include_bytes!(__A0);", &[string(&locale.file).into()]));
    }
    let name_tokens: TokenStream = TokenTree::Ident(name.clone()).into();
    let count = locales.len();
    let mut data = TokenStream::new();
    for locale in &locales {
        let tokens: TokenStream = locale.data.parse().map_err(|_| (locale.span, "corten: bad locale data".to_string()))?;
        data.extend(q("&__A0,", &[tokens]));
    }
    methods.extend(q(
        "#[doc(hidden)] #[inline] fn __corten_locale(self) -> &'static ::corten::i18n::__private::LocaleData {
            static DATA: [&::corten::i18n::__private::LocaleData; __A1] = [__A0];
            DATA[self.0 as usize]
        }
        /// `n` (an integer or a `corten::i18n::Decimal`) in this locale: `1,234,567` in en.
        #[inline] __A2 fn number(self, n: impl ::corten::i18n::Number) -> ::corten::i18n::Formatted {
            ::corten::i18n::Formatted::number(self.__corten_locale(), n.to_decimal())
        }
        /// A ratio as a percentage in this locale, with at most `digits` fraction digits (rounded
        /// half away from zero, trailing zeros dropped): `t.percent(Decimal::new(29, 3), 2)` is
        /// `2.9%` in en, `2,9 %` in fr and `%2,9` in tr.
        #[inline] __A2 fn percent(self, ratio: impl ::corten::i18n::Number, digits: u8) -> ::corten::i18n::Formatted {
            ::corten::i18n::Formatted::percent(self.__corten_locale(), ratio.to_decimal(), digits)
        }
        /// An amount of money in this locale: `t.money(Minor(123450), USD)` is `$1,234.50` in en and
        /// `1 234,50 $US` in fr.
        #[inline] __A2 fn money(self, amount: impl ::corten::i18n::Amount, currency: ::corten::i18n::Currency) -> ::corten::i18n::Formatted {
            ::corten::i18n::Formatted::money(self.__corten_locale(), &amount, currency)
        }
        /// A date in this locale, in UTC unless `.zone(..)` says otherwise: `Mar 22, 2026` (en, `Medium`).
        #[inline] __A2 fn date(self, at: impl ::core::convert::Into<::std::time::SystemTime>, style: impl ::corten::i18n::DateOnly) -> ::corten::i18n::When {
            ::corten::i18n::When::date(self.__corten_locale(), at.into(), style)
        }
        /// A time with its zone in this locale, in UTC unless `.zone(..)` says otherwise: `3:04 PM UTC` (en, `Short`).
        #[inline] __A2 fn time(self, at: impl ::core::convert::Into<::std::time::SystemTime>, style: ::corten::i18n::TimeStyle) -> ::corten::i18n::When {
            ::corten::i18n::When::time(self.__corten_locale(), at.into(), style)
        }
        /// A date and a time with its zone in this locale, in UTC unless `.zone(..)` says otherwise:
        /// `Mar 22, 2026, 3:04 PM UTC` (en, `Medium`, `Short`).
        #[inline] __A2 fn date_time(
            self,
            at: impl ::core::convert::Into<::std::time::SystemTime>,
            date: ::corten::i18n::DateStyle,
            time: ::corten::i18n::TimeStyle,
        ) -> ::corten::i18n::When {
            ::corten::i18n::When::date_time(self.__corten_locale(), at.into(), date, time)
        }",
        &[data, TokenTree::Literal(proc_macro::Literal::usize_unsuffixed(count)).into(), vis.clone()],
    ));
    methods.extend(q(
        "/// The message whose catalog key (the dotted key, as in the JSON) is `key`, with its
        /// arguments taken by name from `args`: for a key an API answers with. `None` when the
        /// catalog has no such key or `args` lacks one of the message's arguments.
        __A0 fn by_key<'__corten_a, __CortenArgs: ::corten::i18n::Args + ?Sized>(
            self,
            key: &str,
            args: &'__corten_a __CortenArgs,
        ) -> ::core::option::Option<::corten::i18n::Found<'__corten_a, __CortenArgs>> {
            let pieces: &'static [::corten::i18n::__private::Piece] = match key { __A1 _ => return ::core::option::Option::None };
            ::corten::i18n::Found::new(pieces, args)
        }",
        &[vis.clone(), key_arms],
    ));
    {
        let js_tokens: TokenStream = TokenTree::Ident(Ident::new(&format!("{name}Js"), name.span())).into();
        js_methods.extend(q(
            "#[doc(hidden)] #[inline] fn __corten_locale(self) -> &'static ::corten::i18n::__private::LocaleData { __A0(self.0).__corten_locale() }
            /// This locale's number format, for a handler whose JSDoc takes a NumberFormat: it writes format(n) as t.number(n) does.
            #[inline] __A1 fn number(self) -> ::corten::i18n::NumberFormat { ::corten::i18n::NumberFormat::new(self.__corten_locale()) }
            /// This locale's percentage format, for a handler whose JSDoc takes a PercentFormat: it writes format(ratio) as t.percent(ratio, digits) does.
            #[inline] __A1 fn percent(self, digits: u8) -> ::corten::i18n::PercentFormat { ::corten::i18n::PercentFormat::new(self.__corten_locale(), digits) }
            /// An amount's format in `currency`, for a handler whose JSDoc takes a MoneyFormat: it writes format(minor) as t.money(Minor(minor), currency) does.
            #[inline] __A1 fn money(self, currency: ::corten::i18n::Currency) -> ::corten::i18n::MoneyFormat { ::corten::i18n::MoneyFormat::new(self.__corten_locale(), currency) }
            /// A date's format, for a handler whose JSDoc takes a DateFormat: it writes format(at) as t.date(at, style) does.
            #[inline] __A1 fn date(self, style: impl ::corten::i18n::DateOnly) -> ::corten::i18n::DateFormat { ::corten::i18n::DateFormat::date(self.__corten_locale(), style) }
            /// A time's format, for a handler whose JSDoc takes a DateFormat: it writes format(at) as t.time(at, style) does.
            #[inline] __A1 fn time(self, style: ::corten::i18n::TimeStyle) -> ::corten::i18n::DateFormat { ::corten::i18n::DateFormat::time(self.__corten_locale(), style) }
            /// A date and time's format, for a handler whose JSDoc takes a DateFormat: it writes format(at) as t.date_time(at, date, time) does.
            #[inline] __A1 fn date_time(self, date: ::corten::i18n::DateStyle, time: ::corten::i18n::TimeStyle) -> ::corten::i18n::DateFormat {
                ::corten::i18n::DateFormat::date_time(self.__corten_locale(), date, time)
            }",
            &[name_tokens.clone(), vis.clone()],
        ));
        let doc =
            format!(" The messages of {name} with arguments, unfilled, and its formats, for handlers to fill in from a script.");
        let to_js = " This locale's messages with arguments, for a handler to fill in: js::count(total, t.js().rows_selected()) passes one, and the script calls label({ count }).";
        methods.extend(q(
            "#[doc = __A0] #[inline] __A1 const fn js(self) -> __A2 { __A2(self.0) }",
            &[string(to_js).into(), vis.clone(), js_tokens.clone()],
        ));
        js_methods = q(
            "#[doc = __A0] #[derive(Clone, Copy, Debug)] #[allow(dead_code)] __A1 struct __A2(u8);
            #[allow(dead_code, non_snake_case)]
            impl __A2 { __A3 }",
            &[string(&doc).into(), vis.clone(), js_tokens, js_methods],
        );
    }
    let mut out = attrs;
    out.extend(q(
        "#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        __A0 struct __A1(u8);
        // Rebuild when a catalog changes.
        const _: () = { __A2 };
        #[automatically_derived]
        impl ::corten::i18n::Messages for __A1 {
            const LOCALES: &'static [&'static str] = &[__A3];
            fn from_index(index: usize) -> Self { if index < __A4 { __A1(index as u8) } else { __A1(0) } }
            fn index(self) -> usize { self.0 as usize }
        }
        #[allow(non_snake_case, clippy::too_many_arguments)]
        impl __A1 { __A5 }",
        &[vis, name_tokens, files, tags, TokenTree::Literal(proc_macro::Literal::usize_unsuffixed(count)).into(), methods],
    ));
    out.extend(js_methods);
    Ok(out)
}

/// The number, currency and date data for `tag`: its own, its alias's, or (without either) that
/// of the tag without its last subtag (`fr-LU` has `fr`'s).
fn locale_data(tag: &str) -> Result<&'static str, String> {
    use crate::locale_data::{ALIASES, LOCALES};
    let mut candidate = tag;
    loop {
        let same = |t: &&str| t.eq_ignore_ascii_case(candidate);
        let target = ALIASES.iter().find(|(a, _)| same(a)).map_or(candidate, |(_, b)| *b);
        if let Some((_, data)) = LOCALES.iter().find(|(t, _)| t.eq_ignore_ascii_case(target)) {
            return Ok(data);
        }
        match candidate.rfind('-') {
            Some(dash) => candidate = &candidate[..dash],
            None => {
                return Err(format!(
                    "corten has no number and date data for {tag:?} or its language: the locales it has are the ones in scripts/locale-data.mjs"
                ))
            }
        }
    }
}

/// Every locale's data, `&[(tag, &LocaleData)]`: for corten's own test against Intl.
pub fn all_locales() -> TokenStream {
    let mut list = TokenStream::new();
    for (tag, data) in crate::locale_data::LOCALES {
        let tokens: TokenStream = data.parse().expect("locale data");
        list.extend(q("(__A0, &__A1),", &[string(tag).into(), tokens]));
    }
    q("&[__A0]", &[list])
}

fn ident(name: &str) -> TokenTree {
    TokenTree::Ident(Ident::new(name, Span::call_site()))
}

/// Names that are a method of the messages type but not a Rust keyword: an argument may use them.
const METHODS: &[&str] =
    &["locale", "index", "from_index", "negotiate", "js", "by_key", "number", "money", "date", "time", "date_time"];

/// An argument's parameter: the catalog's name in snake case (`{updatedAt}` is `updated_at`), raw
/// when it is a Rust keyword (`{type}` is `r#type`), so a catalog shared with other apps compiles
/// as it is. `by_key` and `Message<..>` keep the catalog's name.
fn arg_ident(name: &str) -> TokenTree {
    let snake = snake_case(name);
    if RESERVED.contains(&snake.as_str()) && !METHODS.contains(&snake.as_str()) {
        TokenTree::Ident(Ident::new_raw(&snake, Span::call_site()))
    } else {
        ident(&snake)
    }
}

/// `updatedAt` as `updated_at`, `itemID` as `item_id`; a snake case name as it is.
fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut previous = '_';
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            if previous.is_ascii_lowercase() || previous.is_ascii_digit() {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
        previous = c;
    }
    out
}

/// Nested objects become dotted keys, in file order.
fn flatten(prefix: &str, value: Value, out: &mut Vec<(String, String)>) -> Result<(), String> {
    match value {
        Value::Object(entries) => {
            for (key, value) in entries {
                let full = if prefix.is_empty() { key } else { format!("{prefix}.{key}") };
                flatten(&full, value, out)?;
            }
            Ok(())
        }
        Value::String(text) if prefix.is_empty() => Err(format!("expected an object of messages, found {text:?}")),
        Value::String(text) => {
            if out.iter().any(|(k, _)| *k == prefix) {
                return Err(format!("{prefix:?} appears twice"));
            }
            out.push((prefix.to_string(), text));
            Ok(())
        }
        Value::Other(what) => Err(format!("{prefix:?} is {what}; messages are strings")),
    }
}

/// `{name}` is an argument, `{{` and `}}` are braces, any other brace is an error.
pub fn parse_message(message: &str) -> Result<Vec<Part>, String> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut chars = message.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push('}');
            }
            '{' => {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => name.push(c),
                        None => return Err("a { is not closed (write {{ for a brace)".into()),
                    }
                }
                let valid = name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !valid {
                    return Err(format!("{{{name}}} is not an argument name like {{count}} (write {{{{ for a brace)"));
                }
                // The only names Rust cannot give a parameter, even as `r#name`, and the generated
                // code's own.
                if matches!(snake_case(&name).as_str(), "self" | "super" | "crate" | "_") || name.starts_with("__corten") {
                    return Err(format!("{{{name}}} cannot be an argument's name in Rust; rename it in the catalog"));
                }
                // Two names that make one parameter (`{updatedAt}` and `{updated_at}`).
                if let Some(Part::Arg(other)) =
                    parts.iter().find(|p| matches!(p, Part::Arg(o) if *o != name && snake_case(o) == snake_case(&name)))
                {
                    return Err(format!(
                        "{{{other}}} and {{{name}}} are both the parameter {}; use one name in the catalog",
                        snake_case(&name)
                    ));
                }
                if !text.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut text)));
                }
                parts.push(Part::Arg(name));
            }
            '}' => return Err("a } without a { (write }} for a brace)".into()),
            c => text.push(c),
        }
    }
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    Ok(parts)
}

/// A message as a handler argument, as it goes in `data-on` (HTML-escaped): text and argument
/// names alternating, text first and last: %["","count"," selected"].
fn wire(parts: &[Part]) -> String {
    let mut pieces = vec![String::new()];
    for part in parts {
        match part {
            Part::Text(text) => pieces.last_mut().expect("text first").push_str(text),
            Part::Arg(name) => {
                pieces.push(name.clone());
                pieces.push(String::new());
            }
        }
    }
    let mut json = String::from("%[");
    for (i, piece) in pieces.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push('"');
        for c in piece.chars() {
            match c {
                '"' => json.push_str("\\\""),
                '\\' => json.push_str("\\\\"),
                c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => json.push_str(&format!("\\u{:04x}", c as u32)),
                c => json.push(c),
            }
        }
        json.push('"');
    }
    json.push(']');
    escape(&json)
}

/// The arguments in order of first use.
fn arg_names(parts: &[Part]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for part in parts {
        if let Part::Arg(name) = part {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }
    names
}

fn plain_text(parts: &[Part]) -> String {
    parts.iter().map(|p| if let Part::Text(t) = p { t.as_str() } else { "" }).collect()
}

fn display(parts: &[Part]) -> String {
    parts
        .iter()
        .map(|p| match p {
            Part::Text(t) => t.clone(),
            Part::Arg(a) => format!("{{{a}}}"),
        })
        .collect()
}

/// `item.listing_count` is the method `item_listing_count`.
pub fn method_name(key: &str) -> Result<String, String> {
    let name: String = key.chars().map(|c| if c == '.' || c == '-' { '_' } else { c }).collect();
    let valid = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !RESERVED.contains(&name.as_str());
    if valid {
        Ok(name)
    } else {
        Err(format!("the key {key:?} cannot be a method name; use letters, digits and _ (dots become _)"))
    }
}

/// Every locale has exactly the default's keys, each with the same arguments.
fn check(locales: &[Locale]) -> Result<(), Error> {
    let default = &locales[0];
    for locale in &locales[1..] {
        for (key, parts) in &default.messages {
            let Some((_, theirs)) = locale.messages.iter().find(|(k, _)| k == key) else {
                return err(
                    locale.span,
                    format!("the {:?} catalog has no {key:?}, which the {:?} catalog has", locale.tag, default.tag),
                );
            };
            let ours: BTreeSet<String> = arg_names(parts).into_iter().collect();
            let other: BTreeSet<String> = arg_names(theirs).into_iter().collect();
            if ours != other {
                return err(
                    locale.span,
                    format!("{key:?} takes {} in {:?} but {} in {:?}", list(&ours), default.tag, list(&other), locale.tag),
                );
            }
        }
        if let Some((key, _)) = locale.messages.iter().find(|(k, _)| !default.messages.iter().any(|(d, _)| d == k)) {
            return err(
                locale.span,
                format!("the {:?} catalog has {key:?}, which the {:?} catalog does not", locale.tag, default.tag),
            );
        }
    }
    Ok(())
}

fn list(names: &BTreeSet<String>) -> String {
    if names.is_empty() {
        "no arguments".into()
    } else {
        names.iter().map(|n| format!("{{{n}}}")).collect::<Vec<_>>().join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_split_into_text_and_arguments() {
        assert_eq!(
            parse_message("Show all {count} offers").unwrap(),
            vec![Part::Text("Show all ".into()), Part::Arg("count".into()), Part::Text(" offers".into())]
        );
        assert_eq!(parse_message("{{literal}} {n}").unwrap(), vec![Part::Text("{literal} ".into()), Part::Arg("n".into())]);
        assert!(parse_message("{open").is_err());
        assert!(parse_message("close}").is_err());
        assert!(parse_message("{Count}").is_err());
        assert_eq!(
            parse_message("a {type}").unwrap(),
            vec![Part::Text("a ".into()), Part::Arg("type".into())],
            "a keyword is a raw parameter"
        );
        assert!(parse_message("{self}").unwrap_err().contains("rename it in the catalog"));
        assert!(parse_message("{_}").is_err());
        assert_eq!(parse_message("{updatedAt}").unwrap(), vec![Part::Arg("updatedAt".into())], "the catalog's name is kept");
        assert_eq!(
            (snake_case("updatedAt"), snake_case("itemID"), snake_case("v2Name"), snake_case("count")),
            ("updated_at".into(), "item_id".into(), "v2_name".into(), "count".into())
        );
        let both = parse_message("{updatedAt} {updated_at}").unwrap_err();
        assert!(both.contains("{updatedAt} and {updated_at} are both the parameter updated_at"), "{both}");
        assert_eq!(arg_names(&parse_message("{a} {b} {a}").unwrap()), vec!["a", "b"]);
    }

    #[test]
    fn a_message_for_a_script_alternates_text_and_names() {
        assert_eq!(wire(&parse_message("{n} selected").unwrap()), "%[&quot;&quot;,&quot;n&quot;,&quot; selected&quot;]");
        assert_eq!(
            wire(&parse_message("\"{a}{b}\" & <\u{2028}>").unwrap()),
            r#"%[&quot;\&quot;&quot;,&quot;a&quot;,&quot;&quot;,&quot;b&quot;,&quot;\&quot; &amp; &lt;\u2028&gt;&quot;]"#
        );
    }

    #[test]
    fn keys_become_method_names() {
        assert_eq!(method_name("item.listing_count").unwrap(), "item_listing_count");
        assert_eq!(method_name("nav.sign-in").unwrap(), "nav_sign_in");
        assert!(method_name("type").is_err());
        assert!(method_name("1st").is_err());
    }

    #[test]
    fn nested_objects_flatten_and_non_strings_are_refused() {
        let mut out = Vec::new();
        flatten("", json::parse(r#"{"a": {"b": "x"}, "c": "y"}"#).unwrap(), &mut out).unwrap();
        assert_eq!(out, vec![("a.b".to_string(), "x".to_string()), ("c".to_string(), "y".to_string())]);
        assert!(flatten("", json::parse(r#"{"a": 1}"#).unwrap(), &mut Vec::new()).is_err());
        assert!(flatten("", json::parse(r#"{"a.b": "x", "a": {"b": "y"}}"#).unwrap(), &mut Vec::new()).is_err());
    }
}
