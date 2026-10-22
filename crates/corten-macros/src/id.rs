//! `#[derive(Id)]`: an element id as a type, so the markup that renders an element and the patch
//! that replaces or removes it cannot spell the id differently.
//!
//! `struct DeskStatusId;` is `desk-status`; `struct RowId<'a>(&'a str);` is `row-` and its key.
//! The name is the type's, without a trailing `Id`, in kebab case (after the library prefix,
//! `ui-`, in a crate that sets one). Each name is also an exported symbol, so two ids with the same
//! name in one program fail to build. A library's symbol (a crate with a prefix) also carries its
//! version, so two semver-incompatible versions of one library still link together.

use proc_macro::{TokenStream, TokenTree};

use crate::{
    derive::{parse_item, string},
    tokens::{err, q, Error},
};

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let item = parse_item(input, "Id", true)?;
    let usage = "Id is derived for a unit struct (`struct StatusId;`) or one holding its key (`struct RowId<'a>(&'a str);`)";
    if item.is_enum || item.body.as_ref().is_some_and(|b| !b.stream().is_empty()) {
        return err(item.name.span(), usage);
    }
    let keyed = match &item.tuple {
        None => false,
        Some(fields) if top_level_items(fields) == 1 => true,
        Some(fields) => return err(fields.span(), usage),
    };
    let rust = item.name.to_string();
    let base = rust.strip_suffix("Id").filter(|b| !b.is_empty()).unwrap_or(&rust);
    let mut name = crate::names::kebab(base);
    if let Some(prefix) = crate::script::prefix() {
        name = format!("{prefix}-{name}");
    }
    let write = if keyed {
        q("__OUT.push_str(__A0); ::corten::view::Render::render(&self.0, __OUT);", &[string(&format!("{name}-")).into()])
    } else {
        q("__OUT.push_str(__A0);", &[string(&name).into()])
    };
    let mut out = item.impl_block(
        "::corten::view::Id",
        q(
            "const NAME: &'static str = __A0; fn write_id(&self, __OUT: &mut ::std::string::String) { __A1 }",
            &[string(&name).into(), write],
        ),
    );
    out.extend(item.impl_block(
        "::corten::view::Render",
        q("fn render(&self, out: &mut ::std::string::String) { ::corten::view::Id::write_id(self, out) }", &[]),
    ));
    out.extend(item.impl_block(
        "::corten::view::AttrValue",
        q(
            "fn write_attr(&self, name: &str, out: &mut ::std::string::String) {
                out.push_str(name);
                out.push_str(\"=\\\"\");
                ::corten::view::Id::write_id(self, out);
                out.push('\"');
            }",
            &[],
        ),
    ));
    // One symbol per name: a second id with the same name is a duplicate symbol, in this crate or
    // in any other linked into the program.
    let mut symbol = name.clone();
    if crate::script::prefix().is_some() {
        symbol = format!("{symbol}__{}", std::env::var("CARGO_PKG_VERSION").unwrap_or_default());
    }
    let symbol: String = symbol.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    out.extend(q(
        "const _: () = { #[unsafe(export_name = __A0)] #[used] static __CORTEN_ID: u8 = 0; };",
        &[string(&format!("corten_id__{symbol}")).into()],
    ));
    Ok(out)
}

/// The number of comma-separated items at the top level of a group (`<..>` nests).
fn top_level_items(group: &proc_macro::Group) -> usize {
    let mut items = 0;
    let mut depth = 0i32;
    let mut pending = false;
    for token in group.stream() {
        match &token {
            TokenTree::Punct(p) if p.as_char() == '<' => depth += 1,
            TokenTree::Punct(p) if p.as_char() == '>' => depth -= 1,
            TokenTree::Punct(p) if p.as_char() == ',' && depth == 0 => {
                items += usize::from(pending);
                pending = false;
                continue;
            }
            _ => {}
        }
        pending = true;
    }
    items + usize::from(pending)
}
