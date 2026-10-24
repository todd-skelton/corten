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
//! # let _: Router = Router::new().route("/archive", post(archive));
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
//! The rest of this module carries validation errors to each surface.
//!
//! The rules stay in the app (they are business rules); Corten only carries the result to each
//! surface the same way: a browser gets the form re-rendered with messages next to its fields
//! and focus on the first one ([`crate::Req::invalid`]), a data client or an [`crate::App::api`]
//! route gets the JSON error with one message per field (`Errors` is a response, feature `json`),
//! and an agent tool gets the same messages (`ToolError::from(errors)`, feature `agent`).
//!
//! Fields are named by [`FieldName`]s from `#[derive(Fields)]`, never by text. In the form, mark
//! each field and print its message, which the field points at:
//!
//! ```ignore
//! let f = Input::FIELDS;
//! html! {
//!     <input name={f.email} value={input.email} {..errors.aria(f.email)}>
//!     {errors.message(f.email)}
//! }
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

use crate::view::{escape_into, quoted, AttrValue, Attrs, Render};

mod de;
pub use de::{from_bytes, FormError};

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

    /// `aria-invalid="true" aria-describedby="{field}-error"` when `field` has an error, else
    /// nothing: `<input name={f.email} {..errors.aria(f.email)}>`.
    pub fn aria(&self, field: FieldName) -> Aria {
        Aria { field, invalid: self.has(field) }
    }

    /// `<p class="field-error" id="{field}-error">message</p>` when `field` has an error, else
    /// nothing: the element [`Errors::aria`] points at.
    pub fn message(&self, field: FieldName) -> FieldMessage<'_> {
        FieldMessage { field, message: self.get(field) }
    }

    /// `<p class="form-error" role="alert">message</p>` when the form has a message, else
    /// nothing. Put it at the top of the form; `role="alert"` announces it when a patch puts it in.
    pub fn form_message(&self) -> FormMessage<'_> {
        FormMessage { message: self.form() }
    }
}

/// The ARIA attributes of a form field, from [`Errors::aria`].
pub struct Aria {
    field: FieldName,
    invalid: bool,
}

impl Attrs for Aria {
    fn render_attrs(&self, out: &mut String) {
        if self.invalid {
            out.push_str(r#" aria-invalid="true" aria-describedby=""#);
            escape_into(self.field.0, out);
            out.push_str(r#"-error""#);
        }
    }
}

/// A field's error message, from [`Errors::message`].
pub struct FieldMessage<'a> {
    field: FieldName,
    message: Option<&'a str>,
}

impl Render for FieldMessage<'_> {
    fn render(&self, out: &mut String) {
        if let Some(message) = self.message {
            out.push_str(r#"<p class="field-error" id=""#);
            escape_into(self.field.0, out);
            out.push_str(r#"-error">"#);
            escape_into(message, out);
            out.push_str("</p>");
        }
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
        e.aria(PRICE).render_attrs(&mut out);
        e.aria(NAME).render_attrs(&mut out);
        assert_eq!(out, r#" aria-invalid="true" aria-describedby="price-error""#);
        assert_eq!(e.message(PRICE).to_html(), r#"<p class="field-error" id="price-error">Enter a price.</p>"#);
        assert_eq!(e.message(NAME).to_html(), "");
        assert!(e.clone().into_result(1).is_err());
        assert_eq!(Errors::new().into_result(1), Ok(1));
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
