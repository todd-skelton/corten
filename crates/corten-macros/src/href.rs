//! `#[derive(Href)]`: an enum whose variants each hold a link (a typed route, a place on a page),
//! so a value that is one of several routes (an app's screens) is a link, `href={screen}`.

use proc_macro::{TokenStream, TokenTree};

use crate::{
    component::split,
    derive::parse_item,
    tokens::{err, is_punct, q, Error},
};

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let item = parse_item(input, "Href", false)?;
    if !item.is_enum {
        return err(
            item.name.span(),
            "Href is derived for an enum whose variants each hold a route, enum Screen { Items(url::Items) }; a struct is a route, #[derive(Route)]",
        );
    }
    let Some(body) = &item.body else { return err(item.name.span(), "Href: expected the enum's variants") };
    let mut arms = TokenStream::new();
    let mut texts = TokenStream::new();
    for tokens in split(body.stream(), ',') {
        let mut j = 0;
        while is_punct(tokens.get(j), '#') {
            j += 2;
        }
        let Some(TokenTree::Ident(ident)) = tokens.get(j) else { return err(body.span(), "Href: expected a variant") };
        let one = match tokens.get(j + 1) {
            Some(TokenTree::Group(g)) if g.delimiter() == proc_macro::Delimiter::Parenthesis => split(g.stream(), ',').len() == 1,
            _ => false,
        };
        if !one || tokens.len() > j + 2 {
            return err(
                ident.span(),
                format!("Href: {ident} holds one link and nothing else, {ident}(url::{ident}), the route it goes to (a unit variant has no URL)"),
            );
        }
        arms.extend(q(
            "Self::__A0(__corten_to) => ::corten::route::Href::write_link(__corten_to, __OUT, __corten_html),",
            &[TokenTree::Ident(ident.clone()).into()],
        ));
        texts.extend(q(
            "Self::__A0(__corten_to) => ::corten::route::Href::as_text(__corten_to),",
            &[TokenTree::Ident(ident.clone()).into()],
        ));
    }
    let mut out = item.impl_block(
        "::corten::route::Href",
        q(
            "fn write_link(&self, __OUT: &mut ::std::string::String, __corten_html: bool) {
                match self { __A0 }
            }
            fn as_text(&self) -> ::core::option::Option<&str> {
                match self { __A1 }
            }",
            &[arms, texts],
        ),
    );
    out.extend(item.impl_block(
        "::corten::view::AttrValue",
        q(
            "fn write_attr(&self, name: &str, out: &mut ::std::string::String) {
                out.push_str(name);
                out.push_str(\"=\\\"\");
                ::corten::route::Href::write_link(self, out, true);
                out.push('\"');
            }",
            &[],
        ),
    ));
    Ok(out)
}
