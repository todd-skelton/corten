//! Forms: reading what a form posts ([`Form`], [`Query`]) and validation errors for it, shared
//! by every surface.
//!
//! [`Form`] and [`Query`] read a body or a query string the way a browser sends it, a key once
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

use std::fmt;

use axum::{
    extract::{rejection::RawFormRejection, FromRequest, FromRequestParts, RawForm, Request},
    http::{header, request::Parts, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;

use crate::view::{escape_into, quoted, AttrValue, Attrs, Id, Render};

mod de;
mod upload;
pub use de::{from_bytes, FormError};
pub use upload::{File, Files, Upload};

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

/// The query string read into `T` as [`Form`] reads a body: `?condition=nm&condition=lp` is a
/// `Vec` field with both. A typed route's list field writes it that way.
#[derive(Clone, Copy, Debug, Default)]
pub struct Query<T>(pub T);

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

impl<T: DeserializeOwned, S: Send + Sync> FromRequestParts<S> for Query<T> {
    type Rejection = FormRejection;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, FormRejection> {
        let query = parts.uri.query().unwrap_or_default();
        from_bytes(query.as_bytes())
            .map(Query)
            .map_err(|error| FormRejection::invalid(StatusCode::BAD_REQUEST, "Failed to deserialize query string", error))
    }
}

/// Why [`Form`] or [`Query`] refused a request: plain text, as axum's rejections are, so a
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
    fn invalid(status: StatusCode, what: &str, error: FormError) -> Self {
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
deref!(Form, Query);

/// The name a form field posts under, from `#[derive(Fields)]`: `Input::FIELDS.email`. Markup
/// prints it (`name={f.email}`) and [`Errors`] is keyed by it, so renaming or misspelling a field
/// of the form struct is a compile error in both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FieldName(&'static str);

impl FieldName {
    /// Used by the code `#[derive(Fields)]` generates.
    #[doc(hidden)]
    pub const fn __new(name: &'static str) -> Self {
        FieldName(name)
    }

    /// The name as posted: `"email"`.
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl Render for FieldName {
    #[inline]
    fn render(&self, out: &mut String) {
        escape_into(self.0, out)
    }
}

impl AttrValue for FieldName {
    #[inline]
    fn write_attr(&self, name: &str, out: &mut String) {
        quoted(name, self, out)
    }
}

/// Field to message, in the order they were added, and a message about the whole form. A field
/// keeps its first message, and so does the form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Errors {
    fields: Vec<(FieldName, String)>,
    form: Option<String>,
}

impl Errors {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `message` for `field`, unless the field already has one.
    pub fn add(&mut self, field: FieldName, message: impl Into<String>) {
        if !self.has(field) {
            self.fields.push((field, message.into()));
        }
    }

    /// Records a message about the whole form, unless it has one: a rule no single field breaks,
    /// or a refusal from the service the handler wrote to ("The policy was retired meanwhile.").
    /// Prefer [`Errors::add`] when a field is at fault, so the message sits next to it.
    pub fn add_form(&mut self, message: impl Into<String>) {
        self.form.get_or_insert_with(|| message.into());
    }

    pub fn get(&self, field: FieldName) -> Option<&str> {
        self.fields.iter().find(|(f, _)| *f == field).map(|(_, m)| m.as_str())
    }

    pub fn has(&self, field: FieldName) -> bool {
        self.fields.iter().any(|(f, _)| *f == field)
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
    pub fn iter(&self) -> impl Iterator<Item = (FieldName, &str)> {
        self.fields.iter().map(|(f, m)| (*f, m.as_str()))
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
    /// every row) do not share one, and each field is described by its own message.
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
    pub fn aria(&self, field: FieldName) -> Aria<'_, I> {
        Aria { form: &self.form, field, invalid: self.errors.has(field), hint: None }
    }

    /// `<p class="field-error" id="{form}-{field}-error">message</p>` when `field` has an error,
    /// else nothing: the element [`Within::aria`] points at.
    pub fn message(&self, field: FieldName) -> FieldMessage<'_, I> {
        FieldMessage { form: &self.form, field, message: self.errors.get(field) }
    }
}

/// `{form}-{field}-error`, escaped for an attribute.
fn message_id(form: &impl Id, field: FieldName, out: &mut String) {
    form.write_id(out);
    out.push('-');
    escape_into(field.0, out);
    out.push_str("-error");
}

/// The ARIA attributes of a form field, from [`Within::aria`].
pub struct Aria<'a, I> {
    form: &'a I,
    field: FieldName,
    invalid: bool,
    hint: Option<&'a dyn WriteId>,
}

impl<'a, I> Aria<'a, I> {
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

impl<I> crate::view::__private::Writes for Aria<'_, I> {
    const WRITES: &'static [&'static str] = &["aria-invalid", "aria-describedby"];
}

impl<I: Id> Attrs for Aria<'_, I> {
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
pub struct FieldMessage<'a, I> {
    form: &'a I,
    field: FieldName,
    message: Option<&'a str>,
}

impl<I: Id> Render for FieldMessage<'_, I> {
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
        escape_into(self.name.as_str(), out);
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
        for (field, message) in &self.fields {
            map.serialize_entry(field.0, message)?;
        }
        map.end()
    }
}

/// 422 Unprocessable Content with the JSON error every JSON failure has, plus the fields:
/// `{"error":{"status":422,"message":"email: Enter an email address.","fields":{"email":"Enter an email address."}}}`.
/// A message about the whole form comes first in `message` and is `form` too:
/// `{"error":{"status":422,"message":"Sold out.","form":"Sold out.","fields":{}}}`.
/// An [`crate::App::api`] handler returns it as its error, `Result<Json<T>, Errors>`; pages use
/// [`crate::Req::invalid`], which answers this way to data clients.
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
            http::{header, HeaderValue, StatusCode},
            response::IntoResponse,
        };
        let status = StatusCode::UNPROCESSABLE_ENTITY;
        let error = crate::api::error_body(status, &self.to_string());
        let fields = serde_json::to_string(&self).expect("strings serialize");
        let form = match &self.form {
            Some(form) => format!(r#","form":{}"#, serde_json::to_string(form).expect("strings serialize")),
            None => String::new(),
        };
        // The error object with more members: `..."message":"..."` + `,"form":"..","fields":{...}}}`.
        let body = format!(r#"{}{form},"fields":{fields}}}}}"#, &error[..error.len() - 2]);
        let headers = [(header::CONTENT_TYPE, HeaderValue::from_static(crate::api::JSON))];
        (status, headers, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMAIL: FieldName = FieldName("email");
    const PRICE: FieldName = FieldName("price");
    const NAME: FieldName = FieldName("name");

    #[derive(crate::view::Id)]
    struct RowId<'a>(&'a str);

    #[derive(crate::view::Id)]
    struct SignupId;

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
        let body = res.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], br#"{"error":{"status":422,"message":"Sold out.","form":"Sold out.","fields":{}}}"#);
        e.add(EMAIL, "Enter an email address.");
        let body = e.response().into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["fields"]["email"], "Enter an email address.");
        assert_eq!(value["error"]["form"], "Sold out.");
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

    #[tokio::test]
    async fn query_reads_repeated_keys() {
        let (mut parts, _) = request("GET", "/search?ids=nm&ids=lp", None, "").into_parts();
        let Query(picks): Query<Picks> = Query::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(picks.ids, ["nm", "lp"]);
        let (mut parts, _) = request("GET", "/search", None, "").into_parts();
        let Query(picks): Query<Picks> = Query::from_request_parts(&mut parts, &()).await.unwrap();
        assert!(picks.ids.is_empty() && picks.page.is_none());
        let (mut parts, _) = request("GET", "/search?page=1&page=2", None, "").into_parts();
        let bad = Query::<Picks>::from_request_parts(&mut parts, &()).await.unwrap_err();
        assert_eq!(
            bad.to_string(),
            "Failed to deserialize query string: page: sent 2 times; a field that takes several values is a Vec"
        );
        assert_eq!(bad.into_response().status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "json")]
    #[test]
    fn serializes_as_an_object() {
        let mut e = Errors::new();
        e.add(FieldName("b"), "2");
        e.add(FieldName("a"), "1");
        assert_eq!(serde_json::to_string(&e).unwrap(), r#"{"b":"2","a":"1"}"#);
    }
}
