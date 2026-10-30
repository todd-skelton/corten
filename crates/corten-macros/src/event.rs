//! `#[event]`: a struct as a typed DOM event. Its fields are the event's; a script dispatches
//! it (`target.dispatchEvent(new RowToggled({ id, checked }))`, the class imported from
//! `corten/events`) and markup listens for it with `on:RowToggled={..}`.

use proc_macro::{Delimiter, Span, TokenStream, TokenTree};

use crate::{
    names::kebab,
    tokens::{err, is_ident, is_punct, literal, q, stream, Error},
};

pub fn expand(args: TokenStream, item: TokenStream) -> Result<TokenStream, Error> {
    if let Some(first) = args.into_iter().next() {
        return err(first.span(), "#[event] takes no arguments");
    }
    let tokens: Vec<TokenTree> = item.clone().into_iter().collect();
    let mut i = 0;
    let mut vis = Vec::new();
    loop {
        match tokens.get(i) {
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => i += 2,
            Some(TokenTree::Ident(id)) if id.to_string() == "pub" => {
                vis.push(tokens[i].clone());
                i += 1;
                if let Some(TokenTree::Group(g)) = tokens.get(i) {
                    if g.delimiter() == Delimiter::Parenthesis {
                        vis.push(tokens[i].clone());
                        i += 1;
                    }
                }
            }
            _ => break,
        }
    }
    let span = tokens.get(i).map_or(Span::call_site(), TokenTree::span);
    if !is_ident(tokens.get(i), "struct") {
        return err(span, "#[event] goes on a struct with named fields: #[event] pub struct RowToggled { pub id: String }");
    }
    let Some(TokenTree::Ident(name)) = tokens.get(i + 1) else { return err(span, "expected the event's name") };
    let fields = match tokens.get(i + 2) {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => g.stream(),
        _ => {
            return err(
                name.span(),
                format!("an event is a struct with braces, so its name is free for the constant scripts that import it carry: struct {name} {{}}"),
            )
        }
    };
    // A library's events are named under its scripts' prefix (`ui-copied`), as its scopes are, so
    // they never meet an app's.
    let dom = match crate::script::prefix() {
        Some(prefix) => format!("{prefix}-{}", kebab(&name.to_string())),
        None => kebab(&name.to_string()),
    };
    let mut list = TokenStream::new();
    for field in crate::component::split(fields, ',') {
        let mut p = 0;
        while is_punct(field.get(p), '#') {
            p += 2;
        }
        if is_ident(field.get(p), "pub") {
            p += 1;
            if matches!(field.get(p), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis) {
                p += 1;
            }
        }
        let (Some(TokenTree::Ident(field_name)), true) = (field.get(p), is_punct(field.get(p + 1), ':')) else {
            return err(name.span(), "event fields are `name: Type`");
        };
        list.extend(q(
            "::corten::script::Field { name: __A0, ts: ::corten::script::__private::ts::<__A1>() },",
            &[literal(&field_name.to_string()), stream(field[p + 2..].iter().cloned())],
        ));
    }
    // The browser makes the event and reads its fields; Rust only describes them.
    let mut out = q("#[allow(dead_code)]", &[]);
    out.extend(item);
    // `corten/events` exports one class of a name and the page dispatches one event of a name, so
    // two of either fail to build, at any version (the class carries no prefix).
    out.extend(crate::id::one_per_program("two_events_are", &name.to_string(), false));
    out.extend(crate::id::one_per_program("two_events_named", &dom, false));
    out.extend(q(
        "impl ::corten::script::DomEvent for __A0 { const NAME: &'static str = __A1; }
        impl ::corten::script::ev::EventType for __A0 { const TS: &'static str = __A2; }
        impl ::corten::script::ev::SubOf<__A0> for __A0 {}",
        &[TokenTree::Ident(name.clone()).into(), literal(&dom), literal(&name.to_string())],
    ));
    out.extend(q(
        "#[doc = __A0] #[allow(non_upper_case_globals, dead_code)]",
        &[literal(&format!(" The `{dom}` event's class, served with the scripts that import it."))],
    ));
    out.extend(stream(vis));
    out.extend(q(
        "const __A0: ::corten::Component = ::corten::Component {
            name: __A1,
            script: ::core::option::Option::None,
            style: ::core::option::Option::None,
            event: ::core::option::Option::Some(::corten::script::EventDef { class: __A1, name: __A2, fields: &[__A3] }),
            view: ::core::option::Option::None,
        };",
        &[TokenTree::Ident(name.clone()).into(), literal(&name.to_string()), literal(&dom), list],
    ));
    Ok(out)
}
