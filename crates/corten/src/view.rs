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
//!   [`Attrs`] value (`<input {..e.aria(field)}>`, [`crate::Errors::within`]).
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
/// pub fn Panel(title: &str, children: impl Render) -> impl Render {
///     html! { <section><h2>{title}</h2>{children}</section> }
/// }
/// ```
///
/// A parameter written `impl Render` (or any `impl Trait`) is a type parameter of the struct.
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

impl<F> Html<F> {
    /// Used by `html!`, which gives every call site its own `size`.
    pub fn new(size: &'static AtomicUsize, render: F) -> Self {
        Html { render: Cell::new(Some(render)), size }
    }

    /// Markup that only ever renders into its parent's string (a component's children).
    pub fn child(render: F) -> Self {
        Html { render: Cell::new(Some(render)), size: &NO_SIZE }
    }
}

// The closure returns the view's shape (see `Patch`): `()` for children, which are never a patch.
impl<F: FnOnce(&mut String) -> S, S> Render for Html<F> {
    #[inline]
    fn render(&self, out: &mut String) {
        match self.render.take() {
            Some(render) => {
                render(out);
            }
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

/// Which part of a [`Document`] to render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The whole document, for a browser's first load.
    Full,
    /// What a client navigation swaps: the `<title>`, the tags the page puts in `<head>`,
    /// Corten's tags and the children of `<main>`.
    Swap,
    /// The text of the `<title>` alone (escaped), for [`crate::Req::screen`].
    Title,
}

/// A whole HTML document: what [`crate::Req::page`] sends. `html!` markup whose root is
/// `<html>` is one, and so is a component that returns one (`-> impl Document`), used as
/// `html! { <Layout title="Home">..</Layout> }`. Besides the whole document it renders the
/// parts a client navigation and a data client need, without running the rest of the layout.
///
/// The markup is checked when it compiles: one `<head>` holding one `<title>` and `<Corten />`
/// (where Corten's tags go), and one `<main>`, which a navigation swaps:
///
/// ```
/// use corten::prelude::*;
///
/// let doc = html! {
///     <!DOCTYPE html>
///     <html lang="en">
///         <head><title>"Home"</title><Corten /></head>
///         <body><main><h1>"Home"</h1></main></body>
///     </html>
/// };
/// assert_eq!(doc.to_html(), "<!DOCTYPE html><html lang=\"en\"><head><title>Home</title></head><body><main><h1>Home</h1></main></body></html>");
/// ```
///
/// A document without a `<title>` does not compile:
///
/// ```compile_fail
/// # use corten::prelude::*;
/// let doc = html! { <html><head><Corten /></head><body><main></main></body></html> };
/// ```
///
/// Nor does one without `<Corten />` in its `<head>`, or without a `<main>`:
///
/// ```compile_fail
/// # use corten::prelude::*;
/// let doc = html! { <html><head><title>"Home"</title></head><body><main></main></body></html> };
/// ```
///
/// ```compile_fail
/// # use corten::prelude::*;
/// let doc = html! { <html><head><title>"Home"</title><Corten /></head><body></body></html> };
/// ```
///
/// And a page that is not a document is not a page:
///
/// ```compile_fail
/// # use corten::prelude::*;
/// async fn home(req: Req) -> HtmlResult {
///     req.page(html! { <h1>"Home"</h1> })
/// }
/// ```
pub trait Document: Render {
    /// Writes `part` of the document.
    fn render_part(&self, out: &mut String, part: Part);
}

impl<T: Document + ?Sized> Document for &T {
    #[inline]
    fn render_part(&self, out: &mut String, part: Part) {
        (**self).render_part(out, part)
    }
}

/// A document view from `html!` (see [`Document`]).
pub struct DocHtml<F> {
    render: Cell<Option<F>>,
    size: &'static AtomicUsize,
}

impl<F: FnOnce(&mut String, Part)> DocHtml<F> {
    /// Used by `html!`, which gives every call site its own `size`.
    pub fn new(size: &'static AtomicUsize, render: F) -> Self {
        DocHtml { render: Cell::new(Some(render)), size }
    }
}

impl<F: FnOnce(&mut String, Part)> Render for DocHtml<F> {
    #[inline]
    fn render(&self, out: &mut String) {
        self.render_part(out, Part::Full)
    }

    fn to_html(&self) -> String {
        let mut out = String::with_capacity(self.size.load(Ordering::Relaxed).max(256));
        self.render(&mut out);
        self.size.store(out.len() + out.len() / 8, Ordering::Relaxed);
        out
    }
}

impl<F: FnOnce(&mut String, Part)> Document for DocHtml<F> {
    #[inline]
    fn render_part(&self, out: &mut String, part: Part) {
        match self.render.take() {
            Some(render) => render(out, part),
            None => panic!("a view from html! renders once; call the function that builds it again"),
        }
    }
}

/// Copies `html`, the tags a page put in a document's `<head>`, to `out` with
/// `data-corten-head` at the end of each top-level element's start tag, so a client navigation
/// replaces them with the next page's. `html` is markup Corten wrote: text and attribute values
/// are escaped, so every `<` starts a tag, a comment or the end of one and every `>` ends one;
/// `<script>` and `<style>` text is copied as it is.
fn mark_head_tags(html: &str, out: &mut String) {
    const VOID: &[&str] = &["base", "link", "meta", "area", "br", "col", "embed", "hr", "img", "input", "source", "track", "wbr"];
    let bytes = html.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while let Some(at) = bytes[i..].iter().position(|&b| b == b'<') {
        let lt = i + at;
        match bytes.get(lt + 1) {
            Some(b'/') => {
                depth = depth.saturating_sub(1);
                out.push_str(&html[i..lt + 2]);
                i = lt + 2;
            }
            Some(c) if c.is_ascii_alphabetic() => {
                let end = bytes[lt + 1..]
                    .iter()
                    .position(|b| !(b.is_ascii_alphanumeric() || *b == b'-'))
                    .map_or(bytes.len(), |n| lt + 1 + n);
                let name = &html[lt + 1..end];
                // The start tag's `>`, or its `/>`.
                let gt = bytes[end..].iter().position(|&b| b == b'>').map_or(bytes.len(), |n| end + n);
                let gt = if gt > end && bytes[gt - 1] == b'/' { gt - 1 } else { gt };
                out.push_str(&html[i..gt]);
                if depth == 0 {
                    out.push_str(" data-corten-head");
                }
                i = gt;
                if name.eq_ignore_ascii_case("script") || name.eq_ignore_ascii_case("style") {
                    // Raw text: copied to its end tag, which closes the element.
                    let close = if name.eq_ignore_ascii_case("script") { "</script>" } else { "</style>" };
                    let stop = html[i..].find(close).map_or(html.len(), |n| i + n + close.len());
                    out.push_str(&html[i..stop]);
                    i = stop;
                } else if !VOID.iter().any(|v| name.eq_ignore_ascii_case(v)) {
                    depth += 1;
                }
            }
            _ => {
                out.push_str(&html[i..lt + 1]);
                i = lt + 1;
            }
        }
    }
    out.push_str(&html[i..]);
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

/// In a patch, adds `children` at the end of the element with this id, after what it has:
/// `<template id="results" data-append>..</template>`. A list that grows (Load more, the next
/// page of results) gets only its new rows: the server reads and renders one page, and the rows
/// the page has stay as they are, with what the user did to them.
///
/// ```
/// use corten::{prelude::*, view::{Append, Id}};
///
/// #[derive(Id)]
/// struct ResultsId;
///
/// #[derive(Id)]
/// struct ResultId(u32);
///
/// let more = [25, 26];
/// let patch = html! { {Append(ResultsId, html! { for n in more { <li id={ResultId(n)}>{n}</li> } })} };
/// assert_eq!(
///     patch.to_html(),
///     r#"<template id="results" data-append><li id="result-25">25</li><li id="result-26">26</li></template>"#
/// );
/// ```
///
/// The list is named by its id type, so an id written as text does not compile:
///
/// ```compile_fail
/// use corten::{prelude::*, view::Append};
///
/// let patch = html! { {Append("results", html! { <li>"25"</li> })} };
/// ```
pub struct Append<T, V>(pub T, pub V);

impl<T: Id, V: Render> Render for Append<T, V> {
    fn render(&self, out: &mut String) {
        out.push_str("<template id=\"");
        self.0.write_id(out);
        out.push_str("\" data-append>");
        self.1.render(out);
        out.push_str("</template>");
    }
}

/// What a `<form data-patch>` answer sends ([`crate::Req::patch`]): elements the runtime puts in
/// place of the page's elements with the same ids. An element without an id would be dropped in
/// the browser, so a view is a patch only when the compiler can see that each of its top-level
/// elements has one:
///
/// - `html!` whose top-level elements each have `id={..}` with a [`#[derive(Id)]`](Id) type, and
///   whose other top-level parts (expressions, components, the bodies of `for`, `if` and
///   `match`) are patches too. `let` is allowed; text is not.
/// - A component declared `-> impl Patch`, whose markup is such an `html!`.
/// - [`Remove`], and a reference, `Option` (`None` sends nothing), `Vec` or slice of patches.
///
/// ```
/// use corten::{prelude::*, view::Remove};
///
/// #[derive(Id)]
/// struct RowId<'a>(&'a str);
/// #[derive(Id)]
/// struct StatusId;
///
/// #[component]
/// fn Status(text: &str) -> impl Patch {
///     html! { <p id={StatusId} role="status">{text}</p> }
/// }
///
/// fn archived<'a>(done: &'a [&'a str]) -> impl Patch + 'a {
///     html! {
///         for id in done { {Remove(RowId(id))} }
///         <Status text="Archived." />
///     }
/// }
/// # let _ = archived(&["7"]);
/// ```
///
/// An element without an id is not a patch, and neither is one whose id is text, nor text:
///
/// ```compile_fail,E0277
/// use corten::{prelude::*, view::Patch};
/// fn send(_: impl Patch) {}
/// send(html! { <p>"Saved"</p> });
/// ```
///
/// ```compile_fail,E0277
/// use corten::{prelude::*, view::Patch};
/// fn send(_: impl Patch) {}
/// send(html! { <p id={"status"}>"Saved"</p> });
/// ```
///
/// ```compile_fail,E0277
/// use corten::{prelude::*, view::Patch};
/// #[derive(Id)]
/// struct StatusId;
/// fn send(_: impl Patch) {}
/// send(html! { <p id={StatusId}>"Saved"</p> "and more" });
/// ```
///
/// A branch counts too, whether or not it runs:
///
/// ```compile_fail,E0277
/// use corten::{prelude::*, view::Patch};
/// #[derive(Id)]
/// struct StatusId;
/// fn send(_: impl Patch) {}
/// let saved = true;
/// send(html! { if saved { <p id={StatusId}>"Saved"</p> } else { <p>"Not saved"</p> } });
/// ```
///
/// A component is a patch only when it says so, and then its markup must be one:
///
/// ```compile_fail,E0277
/// use corten::{prelude::*, view::Patch};
/// #[derive(Id)]
/// struct StatusId;
/// #[component]
/// fn Status(text: &str) -> impl Render {
///     html! { <p id={StatusId}>{text}</p> }
/// }
/// fn send(_: impl Patch) {}
/// send(Status { text: "Saved" });
/// ```
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
/// #[component]
/// fn Status(text: &str) -> impl Patch {
///     html! { <p role="status">{text}</p> }
/// }
/// ```
///
/// `let`s and then one component are a patch when the component is one, and not otherwise:
///
/// ```
/// use corten::{prelude::*, view::Patch};
/// #[derive(Id)]
/// struct StatusId;
/// #[component]
/// fn Status(text: &str) -> impl Patch {
///     html! { <p id={StatusId} role="status">{text}</p> }
/// }
/// #[component]
/// fn Counted(n: u32) -> impl Patch {
///     html! {
///         let text = n.to_string();
///         <Status text={&text} />
///     }
/// }
/// fn send(view: impl Patch) -> String { view.to_html() }
/// assert_eq!(send(Counted { n: 3 }), r#"<p id="status" role="status">3</p>"#);
/// ```
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
/// #[component]
/// fn Note(text: &str) -> impl Render {
///     html! { <p>{text}</p> }
/// }
/// #[component]
/// fn Counted(n: u32) -> impl Patch {
///     html! {
///         let text = n.to_string();
///         <Note text={&text} />
///     }
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a patch",
    label = "a patch replaces elements by id",
    note = "give each top-level element an id={{..}} with a #[derive(Id)] type, and declare a component sent as a patch `-> impl Patch`"
)]
pub trait Patch: Render + __private::PatchSealed {}

impl<F: FnOnce(&mut String) -> S, S: __private::PatchShape> __private::PatchSealed for Html<F> {}
impl<F: FnOnce(&mut String) -> S, S: __private::PatchShape> Patch for Html<F> {}
impl<T: Id> __private::PatchSealed for Remove<T> {}
impl<T: Id> Patch for Remove<T> {}
impl<T: Id, V: Render> __private::PatchSealed for Append<T, V> {}
impl<T: Id, V: Render> Patch for Append<T, V> {}
impl<T: Patch + ?Sized> __private::PatchSealed for &T {}
impl<T: Patch + ?Sized> Patch for &T {}
impl<T: Patch> __private::PatchSealed for Option<T> {}
impl<T: Patch> Patch for Option<T> {}
impl<T: Patch> __private::PatchSealed for [T] {}
impl<T: Patch> Patch for [T] {}
impl<T: Patch> __private::PatchSealed for Vec<T> {}
impl<T: Patch> Patch for Vec<T> {}

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
    // Most text has nothing to escape; a short string is one test (`next_escaped` would check
    // anything under eight bytes one byte at a time). The loop is a function of its own, so text
    // with nothing to escape does not save and restore the registers the loop needs.
    if crate::ascii::any(text.as_bytes(), crate::ascii::html) {
        escape_each(text, out)
    } else {
        out.push_str(text)
    }
}

/// [`escape_into`] for text that may have a byte to escape.
#[inline(never)]
fn escape_each(text: &str, out: &mut String) {
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(i) = next_escaped(bytes, start) {
        out.push_str(&text[start..i]);
        out.push_str(ENTITIES[ESCAPES[bytes[i] as usize] as usize - 1]);
        start = i + 1;
    }
    out.push_str(&text[start..]);
}

/// The index of the first byte from `from` on that `escape_into` replaces. Most text has none, so
/// it skips eight bytes at a time while none of them is one, and finds it in those eight with the
/// table.
#[inline]
fn next_escaped(bytes: &[u8], from: usize) -> Option<usize> {
    let escaped = |chunk: &[u8]| chunk.iter().position(|&byte| ESCAPES[byte as usize] != 0);
    let mut i = from;
    while let Some(chunk) = bytes.get(i..i + 8) {
        // The word test can also stop at a `#`; the table then finds nothing and the skip goes on.
        if crate::ascii::html(u64::from_le_bytes(chunk.try_into().expect("eight bytes"))) {
            if let Some(at) = escaped(chunk) {
                return Some(i + at);
            }
        }
        i += 8;
    }
    escaped(&bytes[i..]).map(|at| i + at)
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

fn push_u64(n: u64, out: &mut String) {
    let mut buf = [0u8; 20];
    out.push_str(std::str::from_utf8(crate::ascii::digits(n, &mut buf)).expect("ASCII digits"));
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

// Digits and `-`: nothing to escape, so written as they are.
macro_rules! display {
    ($($t:ty),*) => {$(
        impl Render for $t {
            fn render(&self, out: &mut String) {
                let _ = fmt::Write::write_fmt(out, format_args!("{self}"));
            }
        }
    )*};
}
display!(u128, i128);

impl Render for f32 {
    fn render(&self, out: &mut String) {
        let _ = fmt::Write::write_fmt(out, format_args!("{self}"));
    }
}

impl Render for f64 {
    #[inline]
    fn render(&self, out: &mut String) {
        if may_tie(*self) {
            let _ = fmt::Write::write_fmt(out, format_args!("{self}"));
        } else {
            push_float(*self, out)
        }
    }
}

/// True when `value` might lie exactly halfway between its two nearest shortest forms, where
/// zmij keeps the even digit and `Display` rounds up (`312985.125f32` is `312985.12` to one and
/// `312985.13` to the other), or be too large to check. A tie needs an exact decimal of at most
/// 18 significant digits ending in 5 at least two places after the point: a short binary
/// fraction (`x.25`, `n / 2048`). Prices and other decimals are never exact in binary, so they
/// stay on the fast path.
fn may_tie(value: f64) -> bool {
    if !value.is_finite() || value == 0.0 {
        return false;
    }
    if value.abs() >= 9_007_199_254_740_992.0 {
        return true;
    }
    let bits = value.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1 << 52) - 1);
    let (mantissa, exponent) = if biased == 0 { (fraction, -1074) } else { (fraction | (1 << 52), biased - 1075) };
    let zeros = mantissa.trailing_zeros();
    // `value` is `odd * 2^exponent` exactly, so its decimal is `odd * 5^-exponent` digits long.
    let (odd, exponent) = (mantissa >> zeros, exponent + zeros as i32);
    if exponent > -2 {
        return false;
    }
    exponent >= -27 && u128::from(odd) * 5u128.pow(exponent.unsigned_abs()) < 10u128.pow(19)
}

/// Writes `value` as its `Display` does (unless `may_tie`): the shortest digits that read back
/// as the same value, never an exponent, no `.0` on a whole number, `NaN`, `inf` and `-inf`.
/// The digits come from zmij (already in axum's tree, through serde_json), several times faster
/// than `core::fmt`'s float path; zmij writes `1e21` and `3.0` where `Display` writes
/// `1000000000000000000000` and `3`, so the digits are laid out again here.
fn push_float(value: f64, out: &mut String) {
    let mut buffer = zmij::Buffer::new();
    let text = buffer.format(value);
    let Some((mantissa, exponent)) = text.split_once('e') else {
        out.push_str(text.strip_suffix(".0").unwrap_or(text));
        return;
    };
    let exponent: i32 = exponent.parse().expect("zmij writes a decimal exponent");
    let (sign, mantissa) = mantissa.strip_prefix('-').map_or(("", mantissa), |rest| ("-", rest));
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    // At most 17 significant digits, the first before the point.
    let mut digits = [0u8; 24];
    let count = whole.len() + fraction.len();
    digits[..whole.len()].copy_from_slice(whole.as_bytes());
    digits[whole.len()..count].copy_from_slice(fraction.as_bytes());
    let digits = std::str::from_utf8(&digits[..count]).expect("ASCII digits");
    // How many digits come before the decimal point.
    let point = whole.len() as i32 + exponent;
    out.push_str(sign);
    if point <= 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', point.unsigned_abs() as usize));
        out.push_str(digits);
    } else if point as usize >= count {
        out.push_str(digits);
        out.extend(std::iter::repeat_n('0', point as usize - count));
    } else {
        out.push_str(&digits[..point as usize]);
        out.push('.');
        out.push_str(&digits[point as usize..]);
    }
}

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

    thread_local! {
        static HEAD_SCRATCH: std::cell::Cell<String> = const { std::cell::Cell::new(String::new()) };
    }

    /// What a page puts in its document's `<head>` (an expression or component there), written
    /// with each element marked for client navigations (see `mark_head_tags`).
    pub fn head_slot(render: impl FnOnce(&mut String), out: &mut String) {
        let mut scratch = HEAD_SCRATCH.take();
        scratch.clear();
        render(&mut scratch);
        super::mark_head_tags(&scratch, out);
        HEAD_SCRATCH.set(scratch);
    }

    /// Implemented only by Corten and `#[component]`: what makes [`super::Patch`] a promise
    /// the compiler checked.
    pub trait PatchSealed {}

    // The shape of a view from `html!`, returned by its closure: one per top-level part, `Yes`
    // when the part is a patch and `No` when it is not, a `for`, `if` or `match` body's as an
    // `Option` (it may not run), in nested pairs that end in `End`. A view is a patch when all
    // of its shape is. Each is a zero-sized type, so working it out costs nothing at run time.
    pub struct Yes;
    pub struct No;
    pub struct End;

    #[diagnostic::on_unimplemented(
        message = "this view is not a patch: one of its top-level parts is not an element with a #[derive(Id)] id",
        label = "a patch replaces elements by id",
        note = "give each top-level element an id={{..}} with a #[derive(Id)] type; text, and an id written as text, cannot be a patch's top level"
    )]
    pub trait PatchShape {}
    impl PatchShape for Yes {}
    impl PatchShape for End {}
    impl<T: PatchShape> PatchShape for Option<T> {}
    impl<A: PatchShape, B: PatchShape> PatchShape for (A, B) {}

    // `(&Proof(&value)).__shape()` is `Yes` when the value is a patch (`ProofPatch` matches
    // without an extra reference) and `No` otherwise; `IdProof` likewise for an element id.
    pub struct Proof<'a, T: ?Sized>(pub &'a T);
    pub trait ProofPatch {
        #[inline(always)]
        fn __shape(&self) -> Yes {
            Yes
        }
    }
    impl<T: super::Patch + ?Sized> ProofPatch for Proof<'_, T> {}
    pub trait ProofOther {
        #[inline(always)]
        fn __shape(&self) -> No {
            No
        }
    }
    impl<T: ?Sized> ProofOther for &Proof<'_, T> {}

    pub struct IdProof<'a, T: ?Sized>(pub &'a T);
    pub trait ProofId {
        #[inline(always)]
        fn __id_shape(&self) -> Yes {
            Yes
        }
    }
    impl<T: super::Id + ?Sized> ProofId for IdProof<'_, T> {}
    pub trait ProofNotId {
        #[inline(always)]
        fn __id_shape(&self) -> No {
            No
        }
    }
    impl<T: ?Sized> ProofNotId for &IdProof<'_, T> {}

    /// A component declared `-> impl Patch` renders its markup through this, so markup that is
    /// not a patch does not compile.
    #[inline(always)]
    pub fn render_patch<T: super::Patch + ?Sized>(view: &T, out: &mut String) {
        view.render(out)
    }

    /// `html!` that is `let`s and then one component: built when it renders, as any view is, so
    /// its props may borrow what the `let`s made. The closure returns [`IsDoc`] when the
    /// component is a [`super::Document`] (a layout), so the view is one too, [`IsPatch`] when it
    /// is a [`super::Patch`], so the view is one too, and [`IsView`] otherwise.
    pub struct One<F> {
        render: std::cell::Cell<Option<F>>,
        size: &'static std::sync::atomic::AtomicUsize,
    }

    /// The component of a [`One`] is a document.
    pub struct IsDoc;
    /// The component of a [`One`] is a patch.
    pub struct IsPatch;
    /// The component of a [`One`] is a view.
    pub struct IsView;

    // A `One` is a patch when its component is: `IsPatch` is a patch's shape, `IsView` is not,
    // so a component that is not a patch gets the same error as any view that is not one.
    impl PatchShape for IsPatch {}
    impl<F: FnOnce(&mut String, Option<super::Part>) -> K, K: PatchShape> PatchSealed for One<F> {}
    impl<F: FnOnce(&mut String, Option<super::Part>) -> K, K: PatchShape> super::Patch for One<F> {}

    impl<F> One<F> {
        /// Used by `html!`, which gives every call site its own `size`.
        #[inline]
        pub fn new(size: &'static std::sync::atomic::AtomicUsize, render: F) -> Self {
            One { render: std::cell::Cell::new(Some(render)), size }
        }

        #[inline]
        fn take(&self) -> F {
            match self.render.take() {
                Some(render) => render,
                None => panic!("a view from html! renders once; call the function that builds it again"),
            }
        }
    }

    impl<F: FnOnce(&mut String, Option<super::Part>) -> K, K> Render for One<F> {
        #[inline]
        fn render(&self, out: &mut String) {
            (self.take())(out, None);
        }

        fn to_html(&self) -> String {
            use std::sync::atomic::Ordering;
            let mut out = String::with_capacity(self.size.load(Ordering::Relaxed).max(256));
            self.render(&mut out);
            self.size.store(out.len() + out.len() / 8, Ordering::Relaxed);
            out
        }
    }

    impl<F: FnOnce(&mut String, Option<super::Part>) -> IsDoc> super::Document for One<F> {
        #[inline]
        fn render_part(&self, out: &mut String, part: super::Part) {
            (self.take())(out, Some(part));
        }
    }

    // Inside a `One`'s closure: `(&&&OneOf(&component)).__one(out, part)` picks `ViaDocument`
    // when the component is a document, else `ViaPatch` when it is a patch, else `ViaView`: each
    // impl is on one reference fewer, so method resolution tries them in that order.
    pub struct OneOf<'a, T: ?Sized>(pub &'a T);

    pub trait ViaDocument {
        fn __one(&self, out: &mut String, part: Option<super::Part>) -> IsDoc;
    }

    impl<T: super::Document + ?Sized> ViaDocument for &&OneOf<'_, T> {
        #[inline]
        fn __one(&self, out: &mut String, part: Option<super::Part>) -> IsDoc {
            match part {
                Some(part) => self.0.render_part(out, part),
                None => self.0.render(out),
            }
            IsDoc
        }
    }

    pub trait ViaPatch {
        fn __one(&self, out: &mut String, part: Option<super::Part>) -> IsPatch;
    }

    impl<T: super::Patch + ?Sized> ViaPatch for &OneOf<'_, T> {
        #[inline]
        fn __one(&self, out: &mut String, _: Option<super::Part>) -> IsPatch {
            self.0.render(out);
            IsPatch
        }
    }

    pub trait ViaView {
        fn __one(&self, out: &mut String, part: Option<super::Part>) -> IsView;
    }

    impl<T: Render + ?Sized> ViaView for OneOf<'_, T> {
        #[inline]
        fn __one(&self, out: &mut String, _: Option<super::Part>) -> IsView {
            self.0.render(out);
            IsView
        }
    }
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
