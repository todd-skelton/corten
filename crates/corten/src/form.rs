//! Forms: reading what a form posts ([`Form`]) and validation errors for it, shared
//! by every surface.
//!
//! [`Form`] and [`crate::Params`] read a body or a query string the way a browser sends it, a key once
//! per value, so a `Vec` field gets every checked box or selected option, a `bool` reads a
//! checkbox (`on`), and a blank field is `None` for an `Option`:
//!
//! ```
//! use corten::prelude::*;
//!
//! #[derive(serde::Deserialize, Fields)]
//! struct Archive {
//!     #[serde(default)]
//!     ids: Vec<String>,
//!     #[serde(default)]
//!     notify: bool,
//! }
//!
//! async fn archive(Form(form): Form<Archive>) -> String {
//!     format!("{} items, notify: {}", form.ids.len(), form.notify)
//! }
//! # let _: Methods = post(archive);
//! ```
//!
//! A browser leaves out a checkbox nobody checked and a list with nothing selected, so a `bool`
//! or list field of a `#[derive(Fields)]` struct needs `#[serde(default)]`; without it the
//! struct does not compile, rather than failing the first time someone checks nothing:
//!
//! ```compile_fail
//! #[derive(serde::Deserialize, corten::Fields)]
//! struct Archive {
//!     ids: Vec<String>,
//! }
//! ```
//!
//! ```compile_fail
//! #[derive(serde::Deserialize, corten::Fields)]
//! struct Alert {
//!     notify: bool,
//! }
//! ```
//!
//! A list's filters are a GET form whose controls are named as its links write the parameters.
//! One struct gives both, and it can borrow, so a page that writes many links copies nothing: its
//! `FIELDS` name the form's controls and its `#[derive(Route)]` writes the links, both renamed by
//! the same `#[serde(..)]`, which needs no serde derive of its own:
//!
//! ```
//! use corten::prelude::*;
//!
//! #[derive(Route, Fields)]
//! #[route("/search")]
//! #[serde(rename_all = "camelCase")]
//! struct Terms<'a> {
//!     price_min: Option<&'a str>,
//!     in_stock: Option<&'a str>,
//! }
//!
//! assert_eq!(Terms::FIELDS.price_min.as_str(), "priceMin");
//! assert_eq!(Terms { price_min: Some("5"), in_stock: None }.href(), "/search?priceMin=5");
//! ```
//!
//! Its handler reads what the form sent back as the route's own [`crate::Params`].
//!
//! The rest of this module carries validation errors to each surface.
//!
//! The rules stay in the app (they are business rules); Corten only carries the result to each
//! surface the same way: a browser gets the form re-rendered with messages next to its fields
//! and focus on the first one ([`crate::Req::invalid`]), a data client or an [`crate::App::api`]
//! route gets the JSON error with one message per field (`Errors` is a response, feature `json`),
//! and an agent tool gets the same messages (`ToolError::from(errors)`, feature `agent`).
//!
//! Fields are named by [`FieldName`]s from `#[derive(Fields)]`, never by text. In the form, mark
//! each field and print its message, which the field points at. Both go through
//! [`Errors::within`] the form's id (the `#[derive(Id)]` of the element a refusal replaces), so a
//! message's id, `{form id}-{field}-error`, is not another form's on the same page:
//!
//! ```ignore
//! let f = Input::FIELDS;
//! let e = errors.within(SignupId);
//! html! {
//!     <form id={SignupId} method="post" data-patch>
//!         <input name={f.email} value={input.email} {..e.aria(f.email)}>
//!         {e.message(f.email)}
//!     </form>
//! }
//! ```
//!
//! Ids are written by `#[derive(Id)]` types, never as text:
//!
//! ```compile_fail
//! let errors = corten::Errors::new();
//! let e = errors.within("signup");
//! ```
//!
//! A field named as text does not compile:
//!
//! ```compile_fail
//! let mut errors = corten::Errors::new();
//! errors.add("email", "Enter an email address.");
//! ```
//!
//! A group of fields is a `Fields` struct put in a form with `#[fields(group)]`, named under its
//! field (`ship_to.street`), and rows are a map from each row's key to one
//! (`#[fields(rows)]`), named under the key (`lines[7].qty`), so the key puts each value in its
//! row and each row's field has its own error:
//!
//! ```
//! use std::collections::BTreeMap;
//! use corten::prelude::*;
//!
//! #[derive(serde::Deserialize, Fields)]
//! struct Address { street: String }
//!
//! #[derive(serde::Deserialize, Fields)]
//! struct Line { qty: u32, #[serde(default)] gift: bool }
//!
//! #[derive(serde::Deserialize, Fields)]
//! struct Order {
//!     #[fields(group)]
//!     ship_to: Address,
//!     #[fields(rows)]
//!     #[serde(default)]
//!     lines: BTreeMap<u32, Line>,
//! }
//!
//! let f = Order::FIELDS;
//! assert_eq!(f.ship_to.street.to_string(), "ship_to.street");
//! assert_eq!(f.lines.at(&7).qty.to_string(), "lines[7].qty");
//!
//! let order: Order = corten::form::from_bytes(b"lines%5B9%5D.qty=1&ship_to.street=Elm&lines%5B7%5D.qty=2&lines%5B7%5D.gift=on").unwrap();
//! assert_eq!((order.ship_to.street.as_str(), order.lines[&7].qty, order.lines[&7].gift, order.lines[&9].gift), ("Elm", 2, true, false));
//!
//! let mut errors = Errors::new();
//! errors.add(f.lines.at(&9).qty, "At least 2.");
//! assert!(errors.has(f.lines.at(&9).qty) && !errors.has(f.lines.at(&7).qty));
//! ```
//!
//! A row is named by the map's key type only:
//!
//! ```compile_fail,E0308
//! # use std::collections::BTreeMap;
//! # use corten::prelude::*;
//! # #[derive(serde::Deserialize, Fields)]
//! # struct Line { qty: u32 }
//! #[derive(serde::Deserialize, Fields)]
//! struct Order {
//!     #[fields(rows)]
//!     #[serde(default)]
//!     lines: BTreeMap<u32, Line>,
//! }
//! let _ = Order::FIELDS.lines.at(&"7").qty;
//! ```
//!
//! and a row's field is reached through its row:
//!
//! ```compile_fail,E0609
//! # use std::collections::BTreeMap;
//! # use corten::prelude::*;
//! # #[derive(serde::Deserialize, Fields)]
//! # struct Line { qty: u32 }
//! # #[derive(serde::Deserialize, Fields)]
//! # struct Order { #[fields(rows)] #[serde(default)] lines: BTreeMap<u32, Line> }
//! let mut errors = Errors::new();
//! errors.add(Order::FIELDS.lines.qty, "At least 1.");
//! ```
//!
//! A form with none sends nothing for rows, so they need `#[serde(default)]`:
//!
//! ```compile_fail
//! # use std::collections::BTreeMap;
//! # #[derive(serde::Deserialize, corten::Fields)]
//! # struct Line { qty: u32 }
//! #[derive(serde::Deserialize, corten::Fields)]
//! struct Order {
//!     #[fields(rows)]
//!     lines: BTreeMap<u32, Line>,
//! }
//! ```
//!
//! and so does a map:
//!
//! ```compile_fail
//! # use std::collections::BTreeMap;
//! #[derive(serde::Deserialize, corten::Fields)]
//! struct Counts {
//!     #[fields(map)]
//!     counts: BTreeMap<u32, u32>,
//! }
//! ```
//!
//! A map's value may be an `Option`, so a count left blank reads as `None`:
//!
//! ```
//! # use std::collections::BTreeMap;
//! #[derive(serde::Deserialize, corten::Fields)]
//! struct Counts {
//!     #[fields(map)]
//!     #[serde(default)]
//!     counts: BTreeMap<u32, Option<u32>>,
//! }
//! let read: Counts = corten::form::from_bytes(b"counts%5B4%5D=2&counts%5B9%5D=").unwrap();
//! assert_eq!(read.counts, BTreeMap::from([(4, Some(2)), (9, None)]));
//! ```
//!
//! A field is `#[fields(group)]`, `#[fields(rows)]` or `#[fields(map)]`, and a name in the form
//! has no `.`, `[` or `]` of its own (they name a group's or a row's fields):
//!
//! ```compile_fail
//! # use std::collections::BTreeMap;
//! #[derive(serde::Deserialize, corten::Fields)]
//! struct Counts {
//!     #[fields(list)]
//!     #[serde(default)]
//!     counts: BTreeMap<u32, u32>,
//! }
//! ```
//!
//! ```compile_fail
//! #[derive(serde::Deserialize, corten::Fields)]
//! struct Order {
//!     #[serde(rename = "ship.zip")]
//!     zip: String,
//! }
//! ```
//!
//! A group's fields are plain, so a group of a type with groups or rows does not compile, and a
//! row holds plain fields and groups, not rows:
//!
//! ```compile_fail,E0080
//! # use corten::prelude::*;
//! #[derive(serde::Deserialize, Fields)]
//! struct Street { line: String }
//! #[derive(serde::Deserialize, Fields)]
//! struct Address { #[fields(group)] street: Street }
//! #[derive(serde::Deserialize, Fields)]
//! struct Order { #[fields(group)] ship_to: Address }
//! let _ = Order::FIELDS;
//! ```
//!
//! ```compile_fail,E0080
//! # use std::collections::BTreeMap;
//! # use corten::prelude::*;
//! #[derive(serde::Deserialize, Fields)]
//! struct Line { qty: u32 }
//! #[derive(serde::Deserialize, Fields)]
//! struct Address { #[fields(rows)] #[serde(default)] lines: BTreeMap<u32, Line> }
//! #[derive(serde::Deserialize, Fields)]
//! struct Order { #[fields(group)] ship_to: Address }
//! let _ = Order::FIELDS;
//! ```
//!
//! ```compile_fail,E0080
//! # use std::collections::BTreeMap;
//! # use corten::prelude::*;
//! #[derive(serde::Deserialize, Fields)]
//! struct Part { qty: u32 }
//! #[derive(serde::Deserialize, Fields)]
//! struct Line { #[fields(rows)] #[serde(default)] parts: BTreeMap<u32, Part> }
//! #[derive(serde::Deserialize, Fields)]
//! struct Order { #[fields(rows)] #[serde(default)] lines: BTreeMap<u32, Line> }
//! let _ = Order::FIELDS;
//! ```
//!
//! A group whose fields a form can all leave out (checkboxes, `Option`s, file inputs, which are
//! never among the text fields) needs a default of its own, or a form that sends none of them is
//! refused:
//!
//! ```compile_fail,E0080
//! # use corten::prelude::*;
//! #[derive(serde::Deserialize, Fields)]
//! struct Notices { #[serde(default)] email: bool, #[serde(default)] text: bool }
//! #[derive(serde::Deserialize, Fields)]
//! struct Account { name: String, #[fields(group)] notices: Notices }
//! let _ = Account::FIELDS;
//! ```
//!
//! ```compile_fail,E0080
//! # use corten::prelude::*;
//! #[derive(serde::Deserialize, Fields)]
//! struct Proof { scan: Files }
//! #[derive(serde::Deserialize, Fields)]
//! struct Claim { title: String, #[fields(group)] proof: Proof }
//! let _ = Claim::FIELDS;
//! ```
//!
//! ```
//! # use corten::prelude::*;
//! #[derive(serde::Deserialize, Fields, Default, PartialEq, Debug)]
//! struct Notices { #[serde(default)] email: bool, #[serde(default)] text: bool }
//! #[derive(serde::Deserialize, Fields)]
//! struct Account { name: String, #[fields(group)] #[serde(default)] notices: Notices }
//! let _ = Account::FIELDS;
//! let read: Account = corten::form::from_bytes(b"name=Ada").unwrap();
//! assert_eq!(read.notices, Notices::default());
//! ```
//!
//! A group is never an `Option`: a form sends a group's fields one by one, so it is a group of
//! `Option`s with a default, as above.
//!
//! ```compile_fail
//! # use corten::prelude::*;
//! #[derive(serde::Deserialize, Fields)]
//! struct Address { street: Option<String> }
//! #[derive(serde::Deserialize, Fields)]
//! struct Order { #[fields(group)] #[serde(default)] ship_to: Option<Address> }
//! ```

use std::fmt;

use axum::{
    extract::{rejection::RawFormRejection, FromRequest, RawForm, Request},
    http::{header, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;

use crate::view::{escape_into, quoted, AttrValue, Attrs, Id, Render};

pub(crate) mod de;
mod upload;
pub use de::{from_bytes, FormError};
pub use upload::{File, Files, Upload, UploadError};

/// Implemented by `#[derive(Fields)]` for a form with [`Files`], whose fields nothing else reads.
#[doc(hidden)]
pub trait __FileInputs {
    fn __file_inputs(&self) -> usize;
}

/// A form's fields: the body of a `POST` (`application/x-www-form-urlencoded`), or the query
/// string of a `GET`, read into `T` the way a browser sends them (see the [module](self)).
/// As an extractor it reads the body, so it comes last among a handler's arguments.
#[derive(Clone, Copy, Debug, Default)]
pub struct Form<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for Form<T> {
    type Rejection = FormRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, FormRejection> {
        let query = matches!(*req.method(), Method::GET | Method::HEAD);
        let RawForm(bytes) = RawForm::from_request(req, state).await.map_err(FormRejection::Body)?;
        match from_bytes(&bytes) {
            Ok(value) => Ok(Form(value)),
            // As axum's `Form`: a query string that does not fit is a bad request, a body a 422.
            Err(error) if query => Err(FormRejection::invalid(StatusCode::BAD_REQUEST, "Failed to deserialize form", error)),
            Err(error) => Err(FormRejection::invalid(StatusCode::UNPROCESSABLE_ENTITY, "Failed to deserialize form body", error)),
        }
    }
}

/// Why [`Form`] or [`crate::Params`] refused a request: plain text, as axum's rejections are, so a
/// browser gets the app's error page (the text under it in dev mode) and a JSON client the JSON
/// error.
#[derive(Debug)]
pub enum FormRejection {
    /// The body could not be read, or a `POST` was not `application/x-www-form-urlencoded`.
    Body(RawFormRejection),
    /// The fields did not fit the type: a value of the wrong kind, a missing field, a field sent
    /// twice.
    Invalid(StatusCode, String),
}

impl FormRejection {
    pub(crate) fn invalid(status: StatusCode, what: &str, error: FormError) -> Self {
        FormRejection::Invalid(status, format!("{what}: {error}"))
    }
}

impl fmt::Display for FormRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormRejection::Body(rejection) => fmt::Display::fmt(rejection, f),
            FormRejection::Invalid(_, text) => f.write_str(text),
        }
    }
}

impl std::error::Error for FormRejection {}

impl IntoResponse for FormRejection {
    fn into_response(self) -> Response {
        match self {
            FormRejection::Body(rejection) => rejection.into_response(),
            FormRejection::Invalid(status, text) => {
                (status, [(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"))], text).into_response()
            }
        }
    }
}

macro_rules! deref {
    ($($name:ident),*) => {$(
        impl<T> std::ops::Deref for $name<T> {
            type Target = T;
            fn deref(&self) -> &T {
                &self.0
            }
        }

        impl<T> std::ops::DerefMut for $name<T> {
            fn deref_mut(&mut self) -> &mut T {
                &mut self.0
            }
        }
    )*};
}
deref!(Form);

/// The name a form field posts under, from `#[derive(Fields)]`: `Input::FIELDS.email`. Markup
/// prints it (`name={f.email}`) and [`Errors`] is keyed by it, so renaming or misspelling a field
/// of the form struct is a compile error in both.
///
/// `P` is where the field is: the form itself ([`Root`], the default), a group
/// (`f.ship_to.street`, posted `ship_to.street`) or a row of [`Rows`]
/// (`f.domains.at(&Domain::SellerPortal).max_concurrent`, posted
/// `domains[seller-portal].max_concurrent`). A form's own field is its name alone, so it costs
/// what a `&'static str` does.
#[derive(Clone, Copy)]
pub struct FieldName<P = Root> {
    at: P,
    name: &'static str,
}

impl FieldName<Root> {
    /// Used by the code `#[derive(Fields)]` generates.
    #[doc(hidden)]
    pub const fn __new(name: &'static str) -> Self {
        FieldName { at: Root, name }
    }

    /// The name as posted: `"email"`. A field of a group or a row is written, not stored: print
    /// it, or compare it with text (`f.ship_to.street == "ship_to.street"`).
    pub const fn as_str(self) -> &'static str {
        self.name
    }
}

impl<P: Place> FieldName<P> {
    /// Used by the code `#[derive(Fields)]` generates.
    #[doc(hidden)]
    pub const fn __at(at: P, name: &'static str) -> Self {
        FieldName { at, name }
    }
}

/// Whether this name is the field `text` names, as a form posts it: `f.email == "email"`.
impl<P: Place> PartialEq<str> for FieldName<P> {
    fn eq(&self, text: &str) -> bool {
        self.at.is_text(self.name, text)
    }
}

impl<P: Place> PartialEq<&str> for FieldName<P> {
    fn eq(&self, text: &&str) -> bool {
        self.at.is_text(self.name, text)
    }
}

impl<P: Place> PartialEq for FieldName<P> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.at == other.at
    }
}

impl<P: Place> Eq for FieldName<P> {}

impl<P: Place> std::hash::Hash for FieldName<P> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.at.hash(state);
        self.name.hash(state);
    }
}

impl<P: Place> fmt::Debug for FieldName<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FieldName({self})")
    }
}

impl<P: Place> fmt::Display for FieldName<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.at.display(self.name, f)
    }
}

impl<P: Place> Render for FieldName<P> {
    #[inline]
    fn render(&self, out: &mut String) {
        self.at.render(self.name, out)
    }
}

impl<P: Place> AttrValue for FieldName<P> {
    #[inline]
    fn write_attr(&self, name: &str, out: &mut String) {
        quoted(name, self, out)
    }
}

/// Where a form struct's fields are, written in front of each field's name: implemented by
/// [`Root`], [`Group`], [`Row`] and [`RowGroup`]. Sealed.
pub trait Place: Copy + PartialEq + std::hash::Hash + private::Sealed {
    /// The name `name` here, escaped for HTML.
    #[doc(hidden)]
    fn render(&self, name: &'static str, out: &mut String) {
        let mut text = String::new();
        self.write(name, &mut text);
        escape_into(&text, out)
    }
    /// The name `name` here as posted, a row's key %-encoded.
    #[doc(hidden)]
    fn write(&self, name: &'static str, out: &mut String);
    #[doc(hidden)]
    fn display(&self, name: &'static str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut text = String::new();
        self.write(name, &mut text);
        f.write_str(&text)
    }
    #[doc(hidden)]
    fn is_text(&self, name: &'static str, text: &str) -> bool {
        let mut own = String::with_capacity(text.len());
        self.write(name, &mut own);
        own == text
    }
    /// What an [`Errors`] keeps of it.
    #[doc(hidden)]
    fn kept(&self) -> At;
    /// Whether an [`Errors`] kept it as `at`.
    #[doc(hidden)]
    fn is_kept(&self, at: &At) -> bool {
        self.kept() == *at
    }
    /// The rows and key of a row's field, which [`Errors`] finds by an index.
    #[doc(hidden)]
    fn row(&self) -> Option<(&'static str, &(dyn RowKey + Sync))> {
        None
    }
}

mod private {
    pub trait Sealed {}
    impl Sealed for super::Root {}
    impl Sealed for super::Group {}
    impl Sealed for super::Row<'_> {}
    impl Sealed for super::RowGroup<'_> {}
}

/// A form's own fields: `email`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Root;

impl Place for Root {
    #[inline]
    fn render(&self, name: &'static str, out: &mut String) {
        escape_into(name, out)
    }
    fn write(&self, name: &'static str, out: &mut String) {
        out.push_str(name)
    }
    fn display(&self, name: &'static str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(name)
    }
    #[inline]
    fn is_text(&self, name: &'static str, text: &str) -> bool {
        name == text
    }
    fn kept(&self) -> At {
        At::Root
    }
    #[inline]
    fn is_kept(&self, at: &At) -> bool {
        matches!(at, At::Root)
    }
}

/// A group's fields (`#[fields(group)]`): `ship_to.street`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Group(&'static str);

impl Place for Group {
    fn render(&self, name: &'static str, out: &mut String) {
        escape_into(self.0, out);
        out.push('.');
        escape_into(name, out);
    }
    fn write(&self, name: &'static str, out: &mut String) {
        out.push_str(self.0);
        out.push('.');
        out.push_str(name);
    }
    fn kept(&self) -> At {
        At::Group(self.0)
    }
    fn is_kept(&self, at: &At) -> bool {
        *at == At::Group(self.0)
    }
}

/// A row's fields (`#[fields(rows)]`): `domains[seller-portal].max_concurrent`. `'k` borrows the key.
#[derive(Clone, Copy)]
pub struct Row<'k> {
    rows: &'static str,
    key: &'k (dyn RowKey + Sync),
}

impl PartialEq for Row<'_> {
    fn eq(&self, other: &Self) -> bool {
        let (mut a, mut b) = (KeyBuf::new(), KeyBuf::new());
        self.rows == other.rows && self.key.key_text(&mut a) == other.key.key_text(&mut b)
    }
}

impl std::hash::Hash for Row<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.rows, self.key.key_text(&mut KeyBuf::new())).hash(state)
    }
}

impl Place for Row<'_> {
    fn render(&self, name: &'static str, out: &mut String) {
        render_row(self.rows, self.key, "", name, out)
    }
    fn write(&self, name: &'static str, out: &mut String) {
        write_row(self.rows, self.key.key_text(&mut KeyBuf::new()), out);
        write_tail("", name, out);
    }
    fn kept(&self) -> At {
        At::Row(self.rows, self.key.key_text(&mut KeyBuf::new()).into(), "")
    }
    fn is_kept(&self, at: &At) -> bool {
        matches!(at, At::Row(rows, key, "") if *rows == self.rows && **key == *self.key.key_text(&mut KeyBuf::new()))
    }
    fn row(&self) -> Option<(&'static str, &(dyn RowKey + Sync))> {
        Some((self.rows, self.key))
    }
}

/// A group's fields in a row: `shipments[7].address.street`.
#[derive(Clone, Copy)]
pub struct RowGroup<'k> {
    row: Row<'k>,
    group: &'static str,
}

impl PartialEq for RowGroup<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.row == other.row && self.group == other.group
    }
}

impl std::hash::Hash for RowGroup<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.row.hash(state);
        self.group.hash(state);
    }
}

impl Place for RowGroup<'_> {
    fn render(&self, name: &'static str, out: &mut String) {
        render_row(self.row.rows, self.row.key, self.group, name, out)
    }
    fn write(&self, name: &'static str, out: &mut String) {
        write_row(self.row.rows, self.row.key.key_text(&mut KeyBuf::new()), out);
        write_tail(self.group, name, out);
    }
    fn kept(&self) -> At {
        At::Row(self.row.rows, self.row.key.key_text(&mut KeyBuf::new()).into(), self.group)
    }
    fn is_kept(&self, at: &At) -> bool {
        matches!(at, At::Row(rows, key, group) if *rows == self.row.rows && *group == self.group && **key == *self.row.key.key_text(&mut KeyBuf::new()))
    }
    fn row(&self) -> Option<(&'static str, &(dyn RowKey + Sync))> {
        Some((self.row.rows, self.row.key))
    }
}

/// `rows[key]`, the key %-encoded.
fn write_row(rows: &str, key: &str, out: &mut String) {
    out.push_str(rows);
    out.push('[');
    encode_key(key, out);
    out.push(']');
}

/// `.name` after a row, or nothing for a map's value (`#[fields(map)]`, which has no name).
fn write_tail(group: &str, name: &str, out: &mut String) {
    if !group.is_empty() {
        out.push('.');
        out.push_str(group);
    }
    if !name.is_empty() {
        out.push('.');
        out.push_str(name);
    }
}

/// `rows[key].group.name` for HTML: the names escaped, the key %-encoded (which leaves only
/// characters HTML takes as they are), with nothing allocated.
fn render_row(rows: &'static str, key: &(dyn RowKey + Sync), group: &'static str, name: &'static str, out: &mut String) {
    escape_into(rows, out);
    out.push('[');
    encode_key(key.key_text(&mut KeyBuf::new()), out);
    out.push(']');
    if !group.is_empty() {
        out.push('.');
        escape_into(group, out);
    }
    if !name.is_empty() {
        out.push('.');
        escape_into(name, out);
    }
}

/// Where a field an [`Errors`] keeps is, its row's key copied.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum At {
    Root,
    Group(&'static str),
    /// The rows' field, the key and a group in the row (`""` for none).
    Row(&'static str, Box<str>, &'static str),
}

impl At {
    fn write(&self, name: &'static str, out: &mut String) {
        match self {
            At::Root => out.push_str(name),
            At::Group(group) => Group(group).write(name, out),
            At::Row(rows, key, group) => {
                write_row(rows, key, out);
                write_tail(group, name, out);
            }
        }
    }
}

/// A row key's characters other than ASCII letters, digits, `-` and `_` as `%XX` (each byte of
/// their UTF-8), so a key cannot end its brackets or put a space in a message's id.
fn encode_key(key: &str, out: &mut String) {
    for b in key.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(b"0123456789ABCDEF"[usize::from(b >> 4)]));
            out.push(char::from(b"0123456789ABCDEF"[usize::from(b & 15)]));
        }
    }
}

/// A key of a [`Rows`] field, written in its fields' names as the form reads it back: text, an
/// integer, or a `#[derive(UrlValue)]` enum (its word). A newtype key implements it as the value
/// it wraps (`self.0.key_text(buf)`), and its `Deserialize` must read it back from that text and
/// from no other (not case-insensitively, say), or one row could be reached by two keys.
///
/// An enum with a variant serde reads by more than its word (`#[serde(alias)]`, `#[serde(other)]`)
/// is no row's key: two keys the form tells apart would be one row on the server, so naming its
/// row does not compile:
///
/// ```compile_fail,E0277
/// use std::collections::BTreeMap;
/// use corten::prelude::*;
///
/// #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, UrlValue, serde::Deserialize)]
/// #[serde(rename_all = "kebab-case")]
/// enum Condition { #[serde(alias = "nm")] NearMint, Played }
///
/// #[derive(serde::Deserialize, Fields)]
/// struct Price { cents: u32 }
///
/// #[derive(serde::Deserialize, Fields)]
/// struct Prices {
///     #[fields(rows)]
///     #[serde(default)]
///     by_condition: BTreeMap<Condition, Price>,
/// }
/// let _ = Prices::FIELDS.by_condition.at(&Condition::NearMint).cents;
/// ```
#[diagnostic::on_unimplemented(
    message = "{Self} is not a row's key",
    label = "names a row by a {Self}",
    note = "a row's key is text, an integer, a newtype of one, or a #[derive(UrlValue)] enum whose every variant serde reads by its word alone (no alias, no other)"
)]
pub trait RowKey {
    /// The key as text, as the form's field reads it back: its own text, or written into `buf`
    /// when it has none stored (an integer), so naming a row allocates nothing.
    fn key_text<'a>(&'a self, buf: &'a mut KeyBuf) -> &'a str;
}

/// Room for a [`RowKey`] with no text stored to write it: 40 bytes, the longest `i128`.
pub struct KeyBuf {
    bytes: [u8; 40],
    len: usize,
}

impl KeyBuf {
    pub fn new() -> Self {
        KeyBuf { bytes: [0; 40], len: 0 }
    }

    /// Writes an integer and gives it back as text. Private: an integer fits (`i128::MIN` is 40
    /// bytes), and any other key has text of its own (a newtype gives its field's).
    fn write(&mut self, value: impl fmt::Display) -> &str {
        use fmt::Write;
        self.len = 0;
        let _ = write!(self, "{value}");
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or_default()
    }
}

impl Default for KeyBuf {
    fn default() -> Self {
        KeyBuf::new()
    }
}

impl fmt::Write for KeyBuf {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len + text.len();
        self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?.copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

impl RowKey for String {
    fn key_text<'a>(&'a self, _: &'a mut KeyBuf) -> &'a str {
        self
    }
}

impl RowKey for Box<str> {
    fn key_text<'a>(&'a self, _: &'a mut KeyBuf) -> &'a str {
        self
    }
}

impl RowKey for &str {
    fn key_text<'a>(&'a self, _: &'a mut KeyBuf) -> &'a str {
        self
    }
}

macro_rules! integer_keys {
    ($($t:ty),*) => {$(
        impl RowKey for $t {
            fn key_text<'a>(&'a self, buf: &'a mut KeyBuf) -> &'a str {
                buf.write(self)
            }
        }
    )*};
}
integer_keys!(u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize);

/// A form struct's field names, from `#[derive(Fields)]`; what lets a struct be a group
/// (`#[fields(group)]`) or a row (`#[fields(rows)]`) of another.
pub trait Fields {
    /// Its names at the form's top.
    type Names: Copy + fmt::Debug;
    /// Its names as a group, `ship_to.street`.
    #[doc(hidden)]
    type InGroup: Copy + fmt::Debug;
    /// Its names in a row, `lines[7].qty`.
    #[doc(hidden)]
    type InRow<'k>: Copy + fmt::Debug;
    /// Its names as a group in a row, `lines[7].address.street`.
    #[doc(hidden)]
    type InRowGroup<'k>: Copy + fmt::Debug;
    /// 0 when it has no group and no rows, 1 when it has groups and no rows, 2 with rows.
    #[doc(hidden)]
    const SHAPE: u8;
    /// Whether a form can send none of its fields (each an `Option` or `#[serde(default)]`): such
    /// a group needs a default of its own.
    #[doc(hidden)]
    const LEAVE_OUT: bool;
    #[doc(hidden)]
    fn in_row<'k>(rows: &'static str, key: &'k (dyn RowKey + Sync)) -> Self::InRow<'k>;
}

/// What a group's or a row's names hold where another group or rows would be: nothing, since a
/// group holds plain fields and a row plain fields and groups (refused when the app builds).
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct Refused;

/// Used by the code `#[derive(Fields)]` generates.
#[doc(hidden)]
pub mod __private {
    use super::*;

    pub const fn group(name: &'static str) -> Group {
        Group(name)
    }

    pub const fn row<'k>(rows: &'static str, key: &'k (dyn RowKey + Sync)) -> Row<'k> {
        Row { rows, key }
    }

    pub const fn row_group<'k>(row: Row<'k>, group: &'static str) -> RowGroup<'k> {
        RowGroup { row, group }
    }
}

/// A form field that is a map from a key to a group of fields, one per row (`#[fields(rows)]`
/// on `BTreeMap<K, V>`): a row's field is named by its key, `f.domains.at(&key).max_concurrent`,
/// so each row's values are read into its own entry and each row's field has its own error.
pub struct Rows<K, V> {
    name: &'static str,
    row: std::marker::PhantomData<fn() -> (K, V)>,
}

impl<K, V> Clone for Rows<K, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K, V> Copy for Rows<K, V> {}

impl<K, V> fmt::Debug for Rows<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rows({})", self.name)
    }
}

impl<K, V: Fields> Rows<K, V> {
    /// Used by the code `#[derive(Fields)]` generates.
    #[doc(hidden)]
    pub const fn __new(name: &'static str) -> Self {
        Rows { name, row: std::marker::PhantomData }
    }

    /// The names of the row whose key is `key`: `f.domains.at(&Domain::SellerPortal).max_concurrent`
    /// is `domains[seller-portal].max_concurrent`. It takes a reference to the map's key type
    /// (`&row.domain`; for a `String` key, `&String`), so a key of another type does not compile.
    pub fn at<'k>(&self, key: &'k K) -> V::InRow<'k>
    where
        K: RowKey + Sync,
    {
        V::in_row(self.name, key)
    }
}

/// A form field that is a map from a key to one plain value (`#[fields(map)]` on
/// `BTreeMap<K, V>`): each key's value is a field of its own, `f.counts.at(&sku)`, posted as
/// `counts[sku]`, with its own error.
pub struct Keyed<K> {
    name: &'static str,
    key: std::marker::PhantomData<fn() -> K>,
}

impl<K> Clone for Keyed<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K> Copy for Keyed<K> {}

impl<K> fmt::Debug for Keyed<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Keyed({})", self.name)
    }
}

impl<K> Keyed<K> {
    /// Used by the code `#[derive(Fields)]` generates.
    #[doc(hidden)]
    pub const fn __new(name: &'static str) -> Self {
        Keyed { name, key: std::marker::PhantomData }
    }

    /// The field of the key `key`: `f.counts.at(&sku)` is `counts[sku]`. It takes a reference to
    /// the map's key type, so a key of another type does not compile.
    pub fn at<'k>(&self, key: &'k K) -> FieldName<Row<'k>>
    where
        K: RowKey + Sync,
    {
        FieldName { at: Row { rows: self.name, key }, name: "" }
    }
}

/// Field to message, in the order they were added, and a message about the whole form. A field
/// keeps its first message, and so does the form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Errors {
    fields: Vec<(Kept, String)>,
    form: Option<String>,
    /// Where each row's messages are in `fields`, by the rows' field and the row's key, so a form
    /// with many rows finds a row's message without reading every other row's.
    /// Made with the first row's message, so a form without rows builds and drops no map.
    rows: Option<Box<RowIndex>>,
    refusal: Refusal,
}

/// [`Errors`]' messages of rows: by the rows' field, then the row's key, where each is in the list.
type RowIndex = std::collections::HashMap<&'static str, std::collections::HashMap<Box<str>, Vec<u32>>>;

/// A [`FieldName`] an [`Errors`] keeps: where it is (its row's key copied) and its name.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Kept {
    at: At,
    name: &'static str,
}

impl Kept {
    fn new<P: Place>(field: FieldName<P>) -> Self {
        Kept { at: field.at.kept(), name: field.name }
    }

    /// Whether this is `field`: for a form's own field, a comparison of two names.
    #[inline]
    fn is<P: Place>(&self, field: &FieldName<P>) -> bool {
        self.name == field.name && field.at.is_kept(&self.at)
    }
}

/// A field's name as [`Errors::iter`] gives it: printed (`Display`) or compared with text.
#[derive(Clone, Copy, Debug)]
pub struct Named<'e>(&'e Kept);

impl fmt::Display for Named<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq<str> for Named<'_> {
    fn eq(&self, text: &str) -> bool {
        let mut own = String::with_capacity(text.len());
        self.0.at.write(self.0.name, &mut own);
        own == text
    }
}

impl PartialEq<&str> for Named<'_> {
    fn eq(&self, text: &&str) -> bool {
        *self == **text
    }
}

impl<P: Place> PartialEq<FieldName<P>> for Named<'_> {
    fn eq(&self, field: &FieldName<P>) -> bool {
        self.0.is(field)
    }
}

impl fmt::Display for Kept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        self.at.write(self.name, &mut out);
        f.write_str(&out)
    }
}

/// Why a submit was refused, which [`Req::invalid`](crate::Req::invalid) answers with its status.
/// A closed set: a refusal is the client's to fix, so it is a 4xx, and a 2xx or a 5xx (or any
/// `StatusCode`) does not compile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Refusal {
    /// 422: the input breaks a rule ([`Errors::add`], [`Errors::add_form`]).
    #[default]
    Invalid,
    /// 409: the submit was made against a state that has moved on (a stale revision, a step
    /// already taken or out of order). Sending the same input again would be refused again; the
    /// user needs the current state first.
    Conflict,
    /// 410: what the submit acts on is gone (deleted meanwhile).
    Gone,
}

impl Refusal {
    /// The status the refusal is answered with.
    pub fn status(self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Refusal::Invalid => StatusCode::UNPROCESSABLE_ENTITY,
            Refusal::Conflict => StatusCode::CONFLICT,
            Refusal::Gone => StatusCode::GONE,
        }
    }
}

impl Errors {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `message` for `field`, unless the field already has one.
    pub fn add<P: Place>(&mut self, field: FieldName<P>, message: impl Into<String>) {
        if self.find(&field).is_some() {
            return;
        }
        if let Some((rows, key)) = field.at.row() {
            let at = u32::try_from(self.fields.len()).unwrap_or(u32::MAX);
            let mut buf = KeyBuf::new();
            let key = key.key_text(&mut buf);
            let row = self.rows.get_or_insert_with(Box::default).entry(rows).or_default();
            match row.get_mut(key) {
                Some(at_key) => at_key.push(at),
                None => {
                    row.insert(key.into(), vec![at]);
                }
            }
        }
        self.fields.push((Kept::new(field), message.into()));
    }

    /// Where `field`'s message is: a row's by the index, any other by reading the list of messages
    /// (one per field given one, rows' included).
    fn find<P: Place>(&self, field: &FieldName<P>) -> Option<usize> {
        match field.at.row() {
            None => self.fields.iter().position(|(f, _)| f.is(field)),
            Some((rows, key)) => {
                let at = self.rows.as_ref()?.get(rows)?.get(key.key_text(&mut KeyBuf::new()))?;
                at.iter().map(|&i| i as usize).find(|&i| self.fields[i].0.is(field))
            }
        }
    }

    /// Records a message about the whole form, unless it has one: a rule no single field breaks,
    /// or a refusal from the service the handler wrote to ("The policy was retired meanwhile.").
    /// Prefer [`Errors::add`] when a field is at fault, so the message sits next to it.
    pub fn add_form(&mut self, message: impl Into<String>) {
        self.form.get_or_insert_with(|| message.into());
    }

    /// Refuses the submit for the state it was made against, not its input: the form's message
    /// (in place of one [`Errors::add_form`] recorded, so the status and the message agree),
    /// answered with the refusal's status (409 for
    /// [`Refusal::Conflict`]) by [`Req::invalid`](crate::Req::invalid) in every mode. The form
    /// is drawn again with what the user typed and the message at its top.
    ///
    /// ```
    /// use corten::{Errors, Refusal};
    ///
    /// let mut errors = Errors::new();
    /// errors.refuse(Refusal::Conflict, "Someone saved this sheet first. Reload to see their changes.");
    /// assert_eq!(errors.refusal().status().as_u16(), 409);
    /// assert_eq!(errors.form(), Some("Someone saved this sheet first. Reload to see their changes."));
    /// ```
    ///
    /// A status that is not a refusal does not compile:
    ///
    /// ```compile_fail,E0308
    /// let mut errors = corten::Errors::new();
    /// errors.refuse(200, "Saved.");
    /// ```
    pub fn refuse(&mut self, why: Refusal, message: impl Into<String>) {
        self.refusal = why;
        self.form = Some(message.into());
    }

    /// Why the submit was refused: [`Refusal::Invalid`] unless [`Errors::refuse`] said otherwise.
    pub fn refusal(&self) -> Refusal {
        self.refusal
    }

    pub fn get<P: Place>(&self, field: FieldName<P>) -> Option<&str> {
        self.find(&field).map(|i| self.fields[i].1.as_str())
    }

    pub fn has<P: Place>(&self, field: FieldName<P>) -> bool {
        self.find(&field).is_some()
    }

    /// The message about the whole form, from [`Errors::add_form`].
    pub fn form(&self) -> Option<&str> {
        self.form.as_deref()
    }

    /// True when no field and not the form has a message.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.form.is_none()
    }

    /// How many messages there are, the form's included.
    pub fn len(&self) -> usize {
        self.fields.len() + usize::from(self.form.is_some())
    }

    /// The fields' messages, in the order they were added.
    pub fn iter(&self) -> impl Iterator<Item = (Named<'_>, &str)> {
        self.fields.iter().map(|(f, m)| (Named(f), m.as_str()))
    }

    /// `Ok(value)` when nothing was added, otherwise the errors.
    pub fn into_result<T>(self, value: T) -> Result<T, Errors> {
        if self.is_empty() {
            Ok(value)
        } else {
            Err(self)
        }
    }

    /// The fields' marks and messages in the form whose id is `form`: the `#[derive(Id)]` of the
    /// element a refused submit replaces (the form, or the block around it). Each message's id is
    /// `{form}-{field}-error`, so two forms on a page with a field of the same name (a price in
    /// every row) do not share one, and each field is described by its own message. A field
    /// several forms share is a helper that takes the form's errors, `e: &corten::Within<'_, I>`.
    #[doc(alias = "aria")]
    #[doc(alias = "message")]
    pub fn within<I: Id>(&self, form: I) -> Within<'_, I> {
        Within { errors: self, form }
    }

    /// `<p class="form-error" role="alert">message</p>` when the form has a message, else
    /// nothing. Put it at the top of the form; `role="alert"` announces it when a patch puts it in.
    pub fn form_message(&self) -> FormMessage<'_> {
        FormMessage { message: self.form() }
    }
}

/// [`Errors`] in one form, from [`Errors::within`].
pub struct Within<'a, I> {
    errors: &'a Errors,
    form: I,
}

impl<I: Id> Within<'_, I> {
    /// `aria-invalid="true" aria-describedby="{form}-{field}-error"` when `field` has an error,
    /// else nothing: `<input name={f.email} {..e.aria(f.email)}>`. A field with a hint of its own
    /// gives it here, `{..e.aria(f.email).hint(&EmailHintId)}`, and is described by the hint and
    /// then the error; a tag that writes `aria-describedby` itself as well does not compile.
    pub fn aria<P: Place>(&self, field: FieldName<P>) -> Aria<'_, I, P> {
        Aria { form: &self.form, field, invalid: self.errors.has(field), hint: None }
    }

    /// `<p class="field-error" id="{form}-{field}-error">message</p>` when `field` has an error,
    /// else nothing: the element [`Within::aria`] points at.
    pub fn message<P: Place>(&self, field: FieldName<P>) -> FieldMessage<'_, I, P> {
        FieldMessage { form: &self.form, field, message: self.errors.get(field) }
    }
}

/// `{form}-{field}-error`, escaped for an attribute.
fn message_id<P: Place>(form: &impl Id, field: FieldName<P>, out: &mut String) {
    form.write_id(out);
    out.push('-');
    field.render(out);
    out.push_str("-error");
}

/// The ARIA attributes of a form field, from [`Within::aria`].
pub struct Aria<'a, I, P = Root> {
    form: &'a I,
    field: FieldName<P>,
    invalid: bool,
    hint: Option<&'a dyn WriteId>,
}

impl<'a, I, P> Aria<'a, I, P> {
    /// The field's hint, the element with the id `hint` (a `#[derive(Id)]`), which describes it
    /// always, ahead of its error: `aria-describedby="{hint} {form}-{field}-error"`.
    pub fn hint<H: Id>(self, hint: &'a H) -> Self {
        Aria { hint: Some(hint), ..self }
    }
}

/// [`Id`] without its `NAME`, so a hint of any `Id` type is held behind one pointer.
trait WriteId {
    fn write_id(&self, out: &mut String);
}

impl<T: Id + ?Sized> WriteId for T {
    fn write_id(&self, out: &mut String) {
        Id::write_id(self, out)
    }
}

impl<I, P> crate::view::__private::Writes for Aria<'_, I, P> {
    const WRITES: &'static [&'static str] = &["aria-invalid", "aria-describedby"];
}

impl<I: Id, P: Place> Attrs for Aria<'_, I, P> {
    fn render_attrs(&self, out: &mut String) {
        if self.invalid {
            out.push_str(r#" aria-invalid="true""#);
        }
        if self.invalid || self.hint.is_some() {
            out.push_str(r#" aria-describedby=""#);
            if let Some(hint) = self.hint {
                hint.write_id(out);
                if self.invalid {
                    out.push(' ');
                }
            }
            if self.invalid {
                message_id(self.form, self.field, out);
            }
            out.push('"');
        }
    }
}

/// A field's error message, from [`Within::message`].
pub struct FieldMessage<'a, I, P = Root> {
    form: &'a I,
    field: FieldName<P>,
    message: Option<&'a str>,
}

impl<I: Id, P: Place> Render for FieldMessage<'_, I, P> {
    fn render(&self, out: &mut String) {
        if let Some(message) = self.message {
            out.push_str(r#"<p class="field-error" id=""#);
            message_id(self.form, self.field, out);
            out.push_str(r#"">"#);
            escape_into(message, out);
            out.push_str("</p>");
        }
    }
}

/// One submission of a form, so a write whose answer was lost and is sent again applies once.
///
/// Put a `SubmitKey` field in the form struct and draw it with [`SubmitKey::field`]: a hidden
/// input with a fresh random UUID v4 (122 random bits), named by the field's [`FieldName`]. A
/// UUID because that is what APIs take as an idempotency key, so the app sends it on unchanged.
///
/// ```
/// use corten::prelude::*;
///
/// #[derive(serde::Deserialize, Fields)]
/// struct Alert {
///     email: String,
///     submit: SubmitKey,
/// }
///
/// let f = Alert::FIELDS;
/// let form = html! {
///     <form method="post" data-patch>
///         {SubmitKey::field(f.submit)}
///         <input name={f.email}>
///     </form>
/// };
/// let html = form.to_html();
/// assert!(html.contains(r#"<input type="hidden" name="submit" data-submit value=""#));
///
/// let key = &html[html.find("value=\"").unwrap() + 7..][..36];
/// assert!(key.as_bytes()[14] == b'4' && b"89ab".contains(&key.as_bytes()[19]), "a version 4 UUID: {key}");
///
/// let posted: Alert = corten::form::from_bytes(b"email=a%40b.c&submit=3F2504E0-4F89-41D3-9A0C-0305E82C3301").unwrap();
/// assert_eq!(posted.submit.as_str(), "3f2504e0-4f89-41d3-9a0c-0305e82c3301", "read in either case, kept lowercase");
/// for bad in ["x", "3f2504e04f8941d39a0c0305e82c3301", "3f2504e0-4f89-11d3-9a0c-0305e82c3301"] {
///     let body = format!("email=a%40b.c&submit={bad}");
///     assert!(corten::form::from_bytes::<Alert>(body.as_bytes()).is_err(), "{bad}: not a UUID v4, a 422 naming the field");
/// }
/// ```
///
/// The browser sends the same key for as long as the submission has no answer: a page reloaded
/// after a post sends it again, and so does the runtime when a `data-patch` form is sent again
/// after a network error, a 5xx or an abort. Once an answer arrives (any status below 500) the
/// runtime gives the form a new key, so the next submit is a new one. The app keeps the keys it
/// has applied (next to the write, in the same transaction) and answers a repeat with what it did
/// the first time; Corten keeps no store. [`corten_cluster`]'s `begin` takes one as its
/// idempotency key.
///
/// [`corten_cluster`]: https://docs.rs/corten-cluster
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubmitKey(Box<str>);

impl SubmitKey {
    /// The hidden input that carries a new key: `<input type="hidden" name=".." data-submit value="..">`.
    pub fn field(name: FieldName) -> SubmitField {
        SubmitField { name, key: uuid_v4(crate::security::random_hex()) }
    }

    /// The UUID, lowercase and hyphenated: `3f2504e0-4f89-41d3-9a0c-0305e82c3301`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SubmitKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for SubmitKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        let bytes = text.as_bytes();
        let shaped = bytes.len() == 36
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| if matches!(i, 8 | 13 | 18 | 23) { *b == b'-' } else { b.is_ascii_hexdigit() });
        if shaped && bytes[14] == b'4' && b"89abAB".contains(&bytes[19]) {
            Ok(SubmitKey(text.to_ascii_lowercase().into()))
        } else {
            Err(serde::de::Error::custom("a submit key is a UUID v4, like 3f2504e0-4f89-41d3-9a0c-0305e82c3301"))
        }
    }
}

/// 32 random hex digits as a version 4 UUID: the version and variant digits set, hyphens added.
fn uuid_v4(hex: [u8; 32]) -> [u8; 36] {
    let mut out = [b'-'; 36];
    let mut digits = hex;
    digits[12] = b'4';
    digits[16] = b"89ab"[(digits[16] as usize) % 4];
    let mut at = 0;
    for (i, digit) in digits.iter().enumerate() {
        if matches!(i, 8 | 12 | 16 | 20) {
            at += 1;
        }
        out[at] = *digit;
        at += 1;
    }
    out
}

/// [`SubmitKey::field`]'s hidden input.
pub struct SubmitField {
    name: FieldName,
    key: [u8; 36],
}

impl Render for SubmitField {
    fn render(&self, out: &mut String) {
        out.push_str(r#"<input type="hidden" name=""#);
        self.name.render(out);
        out.push_str(r#"" data-submit value=""#);
        out.push_str(std::str::from_utf8(&self.key).expect("hex is ASCII"));
        out.push_str(r#"">"#);
    }
}

/// The form's message, from [`Errors::form_message`].
pub struct FormMessage<'a> {
    message: Option<&'a str>,
}

impl Render for FormMessage<'_> {
    fn render(&self, out: &mut String) {
        if let Some(message) = self.message {
            out.push_str(r#"<p class="form-error" role="alert">"#);
            escape_into(message, out);
            out.push_str("</p>");
        }
    }
}

/// `form message; field: message; field: message`, for logs and agent tool results.
impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(form) = &self.form {
            f.write_str(form)?;
        }
        for (i, (field, message)) in self.fields.iter().enumerate() {
            if i > 0 || self.form.is_some() {
                f.write_str("; ")?;
            }
            write!(f, "{field}: {message}")?;
        }
        Ok(())
    }
}

/// A JSON object, field to message (the form's message is not a field; see [`Errors::form`]).
#[cfg(feature = "json")]
impl serde::Serialize for Errors {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.fields.len()))?;
        let mut name = String::new();
        for (field, message) in &self.fields {
            name.clear();
            field.at.write(field.name, &mut name);
            map.serialize_entry(&name, message)?;
        }
        map.end()
    }
}

/// 422 Unprocessable Content (or the status of [`Errors::refuse`]'s refusal, 409 for a stale
/// state) with the Problem Details document (RFC 9457) every JSON failure has, plus the fields
/// as an extension member:
/// `{"title":"Unprocessable Entity","status":422,"detail":"email: Enter an email address.","fields":{"email":"Enter an email address."}}`.
/// A message about the whole form comes first in `detail` and is `form` too:
/// `{"title":"Unprocessable Entity","status":422,"detail":"Sold out.","form":"Sold out.","fields":{}}`.
/// A JSON handler `?`s it (`check(&input)?` in a `JsonResult<T>` handler, whose error is a
/// [`crate::JsonError`]); pages use [`crate::Req::invalid`], which answers this way to data clients.
#[cfg(feature = "json")]
impl axum::response::IntoResponse for Errors {
    fn into_response(self) -> axum::response::Response {
        self.response()
    }
}

#[cfg(feature = "json")]
impl Errors {
    pub(crate) fn response(&self) -> axum::response::Response {
        use axum::{
            http::{header, HeaderValue},
            response::IntoResponse,
        };
        let status = self.refusal.status();
        let mut members = String::new();
        if let Some(form) = &self.form {
            members.push_str(r#","form":"#);
            members.push_str(&serde_json::to_string(form).expect("strings serialize"));
        }
        members.push_str(r#","fields":"#);
        members.push_str(&serde_json::to_string(&self).expect("strings serialize"));
        let body = crate::api::problem(status, &self.to_string(), &members);
        let headers = [(header::CONTENT_TYPE, HeaderValue::from_static(crate::api::PROBLEM))];
        (status, headers, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMAIL: FieldName = FieldName::__new("email");
    const PRICE: FieldName = FieldName::__new("price");
    const NAME: FieldName = FieldName::__new("name");

    #[derive(crate::view::Id)]
    struct RowId<'a>(&'a str);

    #[derive(crate::view::Id)]
    struct SignupId;

    #[test]
    fn a_refusal_replaces_the_form_message_so_status_and_message_agree() {
        let mut e = Errors::new();
        e.add_form("Pick a date after today.");
        e.refuse(Refusal::Conflict, "Someone saved first.");
        assert_eq!((e.refusal(), e.form()), (Refusal::Conflict, Some("Someone saved first.")));
        e.refuse(Refusal::Gone, "The sheet was deleted.");
        assert_eq!((e.refusal().status().as_u16(), e.form()), (410, Some("The sheet was deleted.")));
        e.add_form("later");
        assert_eq!(e.form(), Some("The sheet was deleted."), "add_form keeps the first, as before");
    }

    #[test]
    fn a_field_keeps_its_first_message_and_order_is_kept() {
        let mut e = Errors::new();
        e.add(EMAIL, "Enter an email address.");
        e.add(PRICE, "Enter a price.");
        e.add(EMAIL, "second");
        assert_eq!(e.get(EMAIL), Some("Enter an email address."));
        assert_eq!(e.len(), 2);
        assert_eq!(e.to_string(), "email: Enter an email address.; price: Enter a price.");
        let mut out = String::new();
        let row = e.within(RowId("s<1"));
        row.aria(PRICE).render_attrs(&mut out);
        row.aria(NAME).render_attrs(&mut out);
        assert_eq!(out, r#" aria-invalid="true" aria-describedby="row-s&lt;1-price-error""#);
        assert_eq!(row.message(PRICE).to_html(), r#"<p class="field-error" id="row-s&lt;1-price-error">Enter a price.</p>"#);
        assert_eq!(row.message(NAME).to_html(), "");
        // Two forms with a price field: each message has its own id.
        assert_eq!(
            e.within(RowId("s2")).message(PRICE).to_html(),
            r#"<p class="field-error" id="row-s2-price-error">Enter a price.</p>"#
        );
        assert_eq!(
            e.within(SignupId).message(EMAIL).to_html(),
            r#"<p class="field-error" id="signup-email-error">Enter an email address.</p>"#
        );
        assert!(e.clone().into_result(1).is_err());
        assert_eq!(Errors::new().into_result(1), Ok(1));
    }

    #[derive(crate::view::Id)]
    struct HintId<'a>(&'a str);

    #[test]
    fn a_hint_describes_its_field_ahead_of_the_error() {
        let mut e = Errors::new();
        e.add(PRICE, "Enter a price.");
        let row = e.within(RowId("s1"));
        let attrs = |aria: Aria<'_, RowId<'_>>| {
            let mut out = String::new();
            aria.render_attrs(&mut out);
            out
        };
        assert_eq!(
            attrs(row.aria(PRICE).hint(&HintId("a<b"))),
            r#" aria-invalid="true" aria-describedby="hint-a&lt;b row-s1-price-error""#
        );
        assert_eq!(attrs(row.aria(NAME).hint(&HintId("n"))), r#" aria-describedby="hint-n""#);
        assert_eq!(attrs(row.aria(NAME)), "");
    }

    #[test]
    fn the_form_has_a_message_of_its_own() {
        let mut e = Errors::new();
        e.add_form("The policy was retired meanwhile.");
        e.add_form("second");
        assert_eq!(e.form(), Some("The policy was retired meanwhile."));
        assert!(!e.is_empty() && e.len() == 1 && e.iter().next().is_none(), "no field has a message");
        assert!(e.clone().into_result(1).is_err(), "a form message alone fails the check");
        assert_eq!(e.form_message().to_html(), r#"<p class="form-error" role="alert">The policy was retired meanwhile.</p>"#);
        assert_eq!(Errors::new().form_message().to_html(), "");
        e.add(EMAIL, "Enter an email address.");
        assert_eq!(e.to_string(), "The policy was retired meanwhile.; email: Enter an email address.");
    }

    #[cfg(feature = "json")]
    #[tokio::test]
    async fn json_clients_get_the_form_message() {
        use http_body_util::BodyExt;
        let mut e = Errors::new();
        e.add_form("Sold out.");
        let res = e.response();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(res.headers()["content-type"], "application/problem+json", "RFC 9457's media type");
        let body = res.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            &body[..],
            br#"{"title":"Unprocessable Entity","status":422,"detail":"Sold out.","form":"Sold out.","fields":{}}"#
        );
        e.add(EMAIL, "Enter an email address.");
        let body = e.response().into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["fields"]["email"], "Enter an email address.");
        assert_eq!(value["form"], "Sold out.");
    }

    #[derive(Debug, serde::Deserialize)]
    struct Picks {
        #[serde(default)]
        ids: Vec<String>,
        page: Option<u32>,
    }

    fn request(method: &str, uri: &str, content_type: Option<&str>, body: &'static str) -> Request {
        let mut req = axum::http::Request::builder().method(method).uri(uri);
        if let Some(content_type) = content_type {
            req = req.header(header::CONTENT_TYPE, content_type);
        }
        req.body(axum::body::Body::from(body)).unwrap()
    }

    #[tokio::test]
    async fn form_reads_a_body_or_a_get_query_with_repeated_keys() {
        const URLENCODED: Option<&str> = Some("application/x-www-form-urlencoded");
        let Form(picks): Form<Picks> =
            Form::from_request(request("POST", "/", URLENCODED, "ids=a&ids=b&page="), &()).await.unwrap();
        assert_eq!((picks.ids, picks.page), (vec!["a".to_string(), "b".to_string()], None));
        let Form(picks): Form<Picks> = Form::from_request(request("GET", "/?ids=c&page=2", None, ""), &()).await.unwrap();
        assert_eq!((picks.ids, picks.page), (vec!["c".to_string()], Some(2)));

        let wrong_type = Form::<Picks>::from_request(request("POST", "/", Some("text/plain"), "ids=a"), &()).await.unwrap_err();
        assert_eq!(wrong_type.into_response().status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let bad = Form::<Picks>::from_request(request("POST", "/", URLENCODED, "page=x"), &()).await.unwrap_err();
        assert!(bad.to_string().starts_with("Failed to deserialize form body: page: invalid value"), "{bad}");
        let res = bad.into_response();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(crate::api::is_plain_text(res.headers()));
        let bad_get = Form::<Picks>::from_request(request("GET", "/?page=x", None, ""), &()).await.unwrap_err();
        assert_eq!(bad_get.into_response().status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "json")]
    #[test]
    fn serializes_as_an_object() {
        let mut e = Errors::new();
        e.add(FieldName::__new("b"), "2");
        e.add(FieldName::__new("a"), "1");
        assert_eq!(serde_json::to_string(&e).unwrap(), r#"{"b":"2","a":"1"}"#);
    }
}
