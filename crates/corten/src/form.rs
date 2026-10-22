//! Validation errors for a submitted form, shared by every surface.
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

use crate::view::{escape_into, quoted, AttrValue, Attrs, Render};

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

/// Field to message, in the order they were added. A field keeps its first message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Errors(Vec<(FieldName, String)>);

impl Errors {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `message` for `field`, unless the field already has one.
    pub fn add(&mut self, field: FieldName, message: impl Into<String>) {
        if !self.has(field) {
            self.0.push((field, message.into()));
        }
    }

    pub fn get(&self, field: FieldName) -> Option<&str> {
        self.0.iter().find(|(f, _)| *f == field).map(|(_, m)| m.as_str())
    }

    pub fn has(&self, field: FieldName) -> bool {
        self.0.iter().any(|(f, _)| *f == field)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (FieldName, &str)> {
        self.0.iter().map(|(f, m)| (*f, m.as_str()))
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

/// `field: message; field: message`, for logs and agent tool results.
impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (field, message)) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{field}: {message}")?;
        }
        Ok(())
    }
}

/// A JSON object, field to message.
#[cfg(feature = "json")]
impl serde::Serialize for Errors {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (field, message) in &self.0 {
            map.serialize_entry(field.0, message)?;
        }
        map.end()
    }
}

/// 422 Unprocessable Content with the JSON error every JSON failure has, plus the fields:
/// `{"error":{"status":422,"message":"email: Enter an email address.","fields":{"email":"Enter an email address."}}}`.
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
        // The error object with one more member: `..."message":"..."` + `,"fields":{...}}}`.
        let body = format!(r#"{},"fields":{fields}}}}}"#, &error[..error.len() - 2]);
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

    #[cfg(feature = "json")]
    #[test]
    fn serializes_as_an_object() {
        let mut e = Errors::new();
        e.add(FieldName("b"), "2");
        e.add(FieldName("a"), "1");
        assert_eq!(serde_json::to_string(&e).unwrap(), r#"{"b":"2","a":"1"}"#);
    }
}
