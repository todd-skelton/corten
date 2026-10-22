//! Markup in Rust: [`html!`], [`component`] and the [`Render`] trait.
//!
//! `html!` reads like JSX and compiles to string pushes: static markup is merged into one
//! `push_str` per run at compile time, expressions are escaped as they are written, and nothing
//! is rendered until the response is built. A view is a value that implements [`Render`]; a
//! component is a function that returns one, or a struct from [`component`].
//!
//! ```ignore
//! use corten::prelude::*;
//!
//! #[component]
//! fn PriceRow<'a>(card: &'a Card, highlight: bool) -> impl Render {
//!     html! {
//!         <tr class={highlight.then_some("hot")}>
//!             <td>{card.name}</td>
//!             <td class="price" data-id={card.id}>{card.price}</td>
//!         </tr>
//!     }
//! }
//!
//! fn price_table(cards: &[Card]) -> impl Render + '_ {
//!     html! {
//!         <table>
//!             for card in cards { <PriceRow card={card} highlight={card.price_cents > 50_000} /> }
//!             if cards.is_empty() { <tr><td colspan="2">"Nothing to watch yet."</td></tr> }
//!         </table>
//!     }
//! }
//! ```
//!
//! Syntax:
//!
//! - Text is a string literal: `"Price"`. Source whitespace between tags is never output.
//! - `{expr}` prints an expression, escaped: anything that implements [`Render`] (strings,
//!   numbers, `Option`, other views) or else `Display`. [`raw`] prints trusted HTML as is.
//! - Attributes: `class="x"`, `required`, `href={url}`, `href={("/items/", &item.id)}` (a tuple
//!   prints its parts in order, without building a string). A `bool` value prints the bare name when
//!   true and nothing when false; `None` leaves the attribute out. `{..attrs}` spreads an
//!   [`Attrs`] value (`<input {..errors.aria(field)}>`).
//! - `for`, `if`/`else`, `match` and `let` are written as in Rust, with markup in their braces.
//! - `class={..}` takes a list: a class constant of the component's stylesheet (`class={Title}`,
//!   see [`crate::style`]), utility classes, text, an `Option` of one, or a tuple of them, joined
//!   with spaces; an empty list leaves the attribute out.
//! - In a component with a script ([`crate::script`]), `ref={save}` names an element for its
//!   handlers and `on:click={js::confirm(event, dialog)}` calls one of them.
//! - A view owns what it uses and renders once, so it can be returned from a function and markup
//!   can move or match the values it captured.
//! - `<Card title="x" compact>children</Card>` builds the struct `Card` (from [`component`] or
//!   written by hand) with those fields, `compact` meaning `compact: true`, and the children in
//!   its `children` field. A missing or misspelled prop is a compile error.
//! - Void elements (`<input>`, `<br>`, `<img>`, ...) need no closing tag; other elements close
//!   with `</name>` or `/>`. `<>...</>` groups nodes. `<!DOCTYPE html>` is allowed.
//! - Text inside `<script>` and `<style>` is written as is.

use std::{
    borrow::Cow,
    cell::Cell,
    fmt,
    rc::Rc,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

/// JSX-like markup that compiles to string pushes. See the [module docs](self).
pub use corten_macros::html;

/// Turns a function into a component: a struct with one field per parameter, which `html!`
/// builds from a tag (`<PriceRow card={c} highlight />`) and renders in place.
///
/// ```ignore
/// #[component]
/// pub fn Panel<'a, C: Render>(title: &'a str, children: C) -> impl Render {
///     html! { <section><h2>{title}</h2>{children}</section> }
/// }
/// ```
///
/// Parameters that are `Copy` (references, numbers, `bool`) arrive with their declared type;
/// others arrive by reference. The function body runs when the component renders.
///
/// `#[component(css)]` gives the component the stylesheet named after it, next to it
/// (`panel.css`), and `#[component(css = Panel)]` shares another component's; see
/// [`crate::style`].
pub use corten_macros::component;

/// Something that writes HTML. Strings and numbers escape themselves; views from `html!` write
/// their markup.
pub trait Render {
    fn render(&self, out: &mut String);

    /// Renders into a new string.
    fn to_html(&self) -> String {
        let mut out = String::new();
        self.render(&mut out);
        out
    }
}

/// A view from `html!`: markup that renders when it is used, once. It owns what it captured
/// (references are copied in), so markup can move and match those values freely; build the view
/// again to render it again.
pub struct Html<F> {
    render: Cell<Option<F>>,
    /// The size this `html!` call last rendered to on its own, so the next string it renders into
    /// is allocated once at the right size.
    size: &'static AtomicUsize,
}

static NO_SIZE: AtomicUsize = AtomicUsize::new(0);

impl<F: FnOnce(&mut String)> Html<F> {
    /// Used by `html!`, which gives every call site its own `size`.
    pub fn new(size: &'static AtomicUsize, render: F) -> Self {
        Html { render: Cell::new(Some(render)), size }
    }

    /// Markup that only ever renders into its parent's string (a component's children).
    pub fn child(render: F) -> Self {
        Html { render: Cell::new(Some(render)), size: &NO_SIZE }
    }
}

impl<F: FnOnce(&mut String)> Render for Html<F> {
    #[inline]
    fn render(&self, out: &mut String) {
        match self.render.take() {
            Some(render) => render(out),
            None => panic!("a view from html! renders once; call the function that builds it again"),
        }
    }

    fn to_html(&self) -> String {
        let mut out = String::with_capacity(self.size.load(Ordering::Relaxed).max(256));
        self.render(&mut out);
        self.size.store(out.len() + out.len() / 8, Ordering::Relaxed);
        out
    }
}

/// An element id as a type, from `#[derive(Id)]`: the markup that renders an element and the
/// patch that replaces or removes it name it the same way, so they cannot drift.
///
/// A unit struct is one element; a struct holding a key is one per key. The id is the type's
/// name without a trailing `Id`, in kebab case, then the key (a library's ids start with its
/// prefix, `ui-`). Ids print with `id={..}`, and anywhere an attribute names an element:
/// `for`, `popovertarget`, `commandfor`, `aria-controls`.
///
/// ```
/// use corten::{prelude::*, view::{Id, Remove}};
///
/// #[derive(Id)]
/// struct RowId<'a>(&'a str);
///
/// #[derive(Id)]
/// struct DeskStatusId;
///
/// let row = html! { <tr id={RowId("i7")}><td>"Lamp"</td></tr> };
/// assert_eq!(row.to_html(), r#"<tr id="row-i7"><td>Lamp</td></tr>"#);
/// let status = html! { <p id={DeskStatusId} role="status">"Saved"</p> };
/// assert_eq!(status.to_html(), r#"<p id="desk-status" role="status">Saved</p>"#);
/// assert_eq!(html! { {Remove(RowId("i7"))} }.to_html(), r#"<template id="row-i7" data-remove></template>"#);
/// ```
///
/// Two ids with the same name in one program fail to build (each name is a linker symbol). A
/// library's ids (a crate with `[package.metadata.corten] scripts = "prefix"`) are told apart by
/// version too, so two incompatible versions of one library still build together. Their ids are
/// the same in the page, so render only one of them on a page.
///
/// ```compile_fail
/// mod grid {
///     #[derive(corten::view::Id)]
///     pub struct RowId<'a>(pub &'a str);
/// }
/// mod list {
///     #[derive(corten::view::Id)]
///     pub struct RowId<'a>(pub &'a str);
/// }
/// fn main() {}
/// ```
///
/// An id holds one key, or none:
///
/// ```compile_fail
/// #[derive(corten::view::Id)]
/// struct CellId<'a> { row: &'a str, column: u8 }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not an element id",
    label = "an element id is a `#[derive(Id)]` type, like `RowId(&item.id)`"
)]
pub trait Id {
    /// The id without its key: `"row"`.
    const NAME: &'static str;
    /// Writes the id (escaped for an attribute).
    fn write_id(&self, out: &mut String);
}

pub use corten_macros::Id;

impl<T: Id + ?Sized> Id for &T {
    const NAME: &'static str = T::NAME;
    fn write_id(&self, out: &mut String) {
        (**self).write_id(out)
    }
}

/// In a patch, removes the element with this id: `<template id="row-7" data-remove></template>`.
/// It takes an [`Id`], the type the element's markup prints its id with.
///
/// ```
/// use corten::{prelude::*, view::{Id, Remove}};
///
/// #[derive(Id)]
/// struct RowId<'a>(&'a str);
///
/// let archived = ["7", "9"];
/// let patch = html! { for id in archived { {Remove(RowId(id))} } };
/// assert_eq!(
///     patch.to_html(),
///     r#"<template id="row-7" data-remove></template><template id="row-9" data-remove></template>"#
/// );
/// ```
///
/// An id written as text does not compile:
///
/// ```compile_fail
/// use corten::{prelude::*, view::Remove};
///
/// let patch = html! { {Remove(("row-", "7"))} };
/// ```
pub struct Remove<T>(pub T);

impl<T: Id> Render for Remove<T> {
    fn render(&self, out: &mut String) {
        out.push_str("<template id=\"");
        self.0.write_id(out);
        out.push_str("\" data-remove></template>");
    }
}

/// Trusted HTML, written without escaping. Only for markup the app produced itself.
pub struct Raw<T>(pub T);

/// Prints `html` as is: `{raw(&cached_fragment)}`. Never pass it user input.
pub fn raw<T: AsRef<str>>(html: T) -> Raw<T> {
    Raw(html)
}

impl<T: AsRef<str>> Render for Raw<T> {
    fn render(&self, out: &mut String) {
        out.push_str(self.0.as_ref());
    }
}

/// For each byte, 0 when it is written as is, else the index of its entity in `ENTITIES` plus one.
static ESCAPES: [u8; 256] = {
    let mut table = [0u8; 256];
    table[b'&' as usize] = 1;
    table[b'<' as usize] = 2;
    table[b'>' as usize] = 3;
    table[b'"' as usize] = 4;
    table[b'\'' as usize] = 5;
    table
};
const ENTITIES: [&str; 5] = ["&amp;", "&lt;", "&gt;", "&quot;", "&#39;"];

/// Appends `text` to `out`, escaped for HTML text and double-quoted attribute values.
#[inline]
pub fn escape_into(text: &str, out: &mut String) {
    let bytes = text.as_bytes();
    let mut start = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        let entity = ESCAPES[byte as usize];
        if entity != 0 {
            out.push_str(&text[start..i]);
            out.push_str(ENTITIES[entity as usize - 1]);
            start = i + 1;
        }
    }
    out.push_str(&text[start..]);
}

/// `value`'s `Display` output, escaped, without an intermediate string.
pub fn escape_display(value: &(impl fmt::Display + ?Sized), out: &mut String) {
    struct Escaping<'a>(&'a mut String);
    impl fmt::Write for Escaping<'_> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            escape_into(s, self.0);
            Ok(())
        }
    }
    let _ = fmt::write(&mut Escaping(out), format_args!("{value}"));
}

impl Render for str {
    #[inline]
    fn render(&self, out: &mut String) {
        escape_into(self, out)
    }
}

impl Render for String {
    #[inline]
    fn render(&self, out: &mut String) {
        escape_into(self, out)
    }
}

impl Render for Cow<'_, str> {
    fn render(&self, out: &mut String) {
        escape_into(self, out)
    }
}

impl Render for char {
    fn render(&self, out: &mut String) {
        escape_into(self.encode_utf8(&mut [0; 4]), out)
    }
}

impl<T: Render + ?Sized> Render for &T {
    #[inline]
    fn render(&self, out: &mut String) {
        (**self).render(out)
    }
}

impl<T: Render + ?Sized> Render for Box<T> {
    fn render(&self, out: &mut String) {
        (**self).render(out)
    }
}

impl<T: Render + ?Sized> Render for Rc<T> {
    fn render(&self, out: &mut String) {
        (**self).render(out)
    }
}

impl<T: Render + ?Sized> Render for Arc<T> {
    fn render(&self, out: &mut String) {
        (**self).render(out)
    }
}

/// Nothing for `None`.
impl<T: Render> Render for Option<T> {
    fn render(&self, out: &mut String) {
        if let Some(value) = self {
            value.render(out)
        }
    }
}

/// Each item in order.
impl<T: Render> Render for [T] {
    fn render(&self, out: &mut String) {
        for item in self {
            item.render(out)
        }
    }
}

impl<T: Render> Render for Vec<T> {
    fn render(&self, out: &mut String) {
        self.as_slice().render(out)
    }
}

impl Render for () {
    fn render(&self, _: &mut String) {}
}

/// Each part in order, without building a string: `href={("/items/", &item.id)}`.
macro_rules! tuples {
    ($(($($name:ident),+)),*) => {$(
        impl<$($name: Render),+> Render for ($($name,)+) {
            #[allow(non_snake_case)]
            fn render(&self, out: &mut String) {
                let ($($name,)+) = self;
                $($name.render(out);)+
            }
        }

        impl<$($name: Render),+> AttrValue for ($($name,)+) {
            fn write_attr(&self, name: &str, out: &mut String) {
                quoted(name, self, out)
            }
        }
    )*};
}
tuples!((A, B), (A, B, C), (A, B, C, D));

fn push_u64(mut n: u64, out: &mut String) {
    let mut digits = [0u8; 20];
    let mut i = digits.len();
    loop {
        i -= 1;
        digits[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.push_str(std::str::from_utf8(&digits[i..]).expect("ASCII digits"));
}

macro_rules! unsigned {
    ($($t:ty),*) => {$(
        impl Render for $t {
            #[inline]
            fn render(&self, out: &mut String) {
                push_u64(*self as u64, out)
            }
        }
    )*};
}
unsigned!(u8, u16, u32, u64, usize);

macro_rules! signed {
    ($($t:ty),*) => {$(
        impl Render for $t {
            #[inline]
            fn render(&self, out: &mut String) {
                if *self < 0 {
                    out.push('-');
                }
                push_u64(self.unsigned_abs() as u64, out)
            }
        }
    )*};
}
signed!(i8, i16, i32, i64, isize);

macro_rules! display {
    ($($t:ty),*) => {$(
        impl Render for $t {
            fn render(&self, out: &mut String) {
                escape_display(self, out)
            }
        }
    )*};
}
display!(u128, i128, f32, f64);

/// An attribute value. `true` prints the bare name and `false` nothing; `None` leaves the
/// attribute out; text and numbers print ` name="value"`, escaped. Types that implement only
/// `Display` work too.
pub trait AttrValue {
    /// Writes ` name="value"` (`name` already has its leading space), or nothing.
    fn write_attr(&self, name: &str, out: &mut String);
}

pub(crate) fn quoted(name: &str, value: &(impl Render + ?Sized), out: &mut String) {
    out.push_str(name);
    out.push_str("=\"");
    value.render(out);
    out.push('"');
}

impl AttrValue for bool {
    fn write_attr(&self, name: &str, out: &mut String) {
        if *self {
            out.push_str(name)
        }
    }
}

impl<T: AttrValue> AttrValue for Option<T> {
    fn write_attr(&self, name: &str, out: &mut String) {
        if let Some(value) = self {
            value.write_attr(name, out)
        }
    }
}

impl<T: AttrValue + ?Sized> AttrValue for &T {
    fn write_attr(&self, name: &str, out: &mut String) {
        (**self).write_attr(name, out)
    }
}

macro_rules! attr_text {
    ($($t:ty),*) => {$(
        impl AttrValue for $t {
            #[inline]
            fn write_attr(&self, name: &str, out: &mut String) {
                quoted(name, self, out)
            }
        }
    )*};
}
attr_text!(str, String, Cow<'_, str>, char, u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64);

/// Attributes spread into a start tag with `{..value}`.
pub trait Attrs {
    fn render_attrs(&self, out: &mut String);
}

impl<T: Attrs + ?Sized> Attrs for &T {
    fn render_attrs(&self, out: &mut String) {
        (**self).render_attrs(out)
    }
}

impl<T: Attrs> Attrs for Option<T> {
    fn render_attrs(&self, out: &mut String) {
        if let Some(attrs) = self {
            attrs.render_attrs(out)
        }
    }
}

/// `[("data-id", id), ("title", title)]`: each pair as an attribute. Names come from the app's
/// code; a name with characters an attribute name cannot have is skipped.
impl<N: AsRef<str>, V: AttrValue> Attrs for [(N, V)] {
    fn render_attrs(&self, out: &mut String) {
        for (name, value) in self {
            let name = name.as_ref();
            if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.')) {
                value.write_attr(&format!(" {name}"), out);
            }
        }
    }
}

impl<N: AsRef<str>, V: AttrValue, const L: usize> Attrs for [(N, V); L] {
    fn render_attrs(&self, out: &mut String) {
        self.as_slice().render_attrs(out)
    }
}

impl<N: AsRef<str>, V: AttrValue> Attrs for Vec<(N, V)> {
    fn render_attrs(&self, out: &mut String) {
        self.as_slice().render_attrs(out)
    }
}

/// Used by the code `html!` and `#[component]` generate. Not a public API.
#[doc(hidden)]
pub mod __private {
    use std::fmt::Display;

    use super::{escape_display, AttrValue, Render};

    // Method resolution picks `ViaRender` when the value implements Render (it matches without
    // an extra reference) and falls back to `ViaDisplay`, so `{x}` works for both.
    pub struct Wrap<'a, T: ?Sized>(pub &'a T);

    pub trait ViaRender {
        fn __render(&self, out: &mut String);
    }

    impl<T: Render + ?Sized> ViaRender for Wrap<'_, T> {
        #[inline]
        fn __render(&self, out: &mut String) {
            self.0.render(out)
        }
    }

    pub trait ViaDisplay {
        fn __render(&self, out: &mut String);
    }

    impl<T: Display + ?Sized> ViaDisplay for &Wrap<'_, T> {
        #[inline]
        fn __render(&self, out: &mut String) {
            escape_display(self.0, out)
        }
    }

    pub struct Attr<'a, T: ?Sized>(pub &'a T);

    pub trait AttrViaValue {
        fn __attr(&self, name: &str, out: &mut String);
    }

    impl<T: AttrValue + ?Sized> AttrViaValue for Attr<'_, T> {
        #[inline]
        fn __attr(&self, name: &str, out: &mut String) {
            self.0.write_attr(name, out)
        }
    }

    pub trait AttrViaDisplay {
        fn __attr(&self, name: &str, out: &mut String);
    }

    impl<T: Display + ?Sized> AttrViaDisplay for &Attr<'_, T> {
        fn __attr(&self, name: &str, out: &mut String) {
            out.push_str(name);
            out.push_str("=\"");
            escape_display(self.0, out);
            out.push('"');
        }
    }

    // A component parameter that is Copy arrives as its own type; anything else by reference.
    pub struct Param<'a, T: ?Sized>(pub &'a T);

    pub trait ParamCopy<T> {
        fn __param(&self) -> T;
    }

    impl<T: Copy> ParamCopy<T> for Param<'_, T> {
        #[inline]
        fn __param(&self) -> T {
            *self.0
        }
    }

    pub trait ParamRef<'a, T: ?Sized> {
        fn __param(&self) -> &'a T;
    }

    impl<'a, T: ?Sized> ParamRef<'a, T> for &Param<'a, T> {
        #[inline]
        fn __param(&self) -> &'a T {
            self.0
        }
    }
}

#[cfg(test)]
mod tests;
