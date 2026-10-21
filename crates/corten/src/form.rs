//! Validation errors for a submitted form, shared by every surface.
//!
//! The rules stay in the app (they are business rules); Corten only carries the result to each
//! surface the same way: a browser gets the form re-rendered with messages next to its fields
//! and focus on the first one ([`crate::Req::invalid`]), a data client gets
//! `{"errors": {"field": "message"}}`, and an agent tool gets the same messages
//! (`ToolError::from(errors)`, feature `agent`).
//!
//! In the form, mark each field and print its message, which the field points at:
//!
//! ```ignore
//! html! {
//!     <input name={f.email} value={input.email} {..errors.aria(f.email)}>
//!     {errors.message(f.email)}
//! }
//! ```

use std::fmt;

use crate::view::{escape_into, Attrs, Render};

/// Field name to message, in the order they were added. A field keeps its first message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Errors(Vec<(String, String)>);

impl Errors {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `message` for `field`, unless the field already has one.
    pub fn add(&mut self, field: &str, message: impl Into<String>) {
        if !self.has(field) {
            self.0.push((field.to_string(), message.into()));
        }
    }

    pub fn get(&self, field: &str) -> Option<&str> {
        self.0.iter().find(|(f, _)| f == field).map(|(_, m)| m.as_str())
    }

    pub fn has(&self, field: &str) -> bool {
        self.0.iter().any(|(f, _)| f == field)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(f, m)| (f.as_str(), m.as_str()))
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
    pub fn aria<'a>(&'a self, field: &'a str) -> Aria<'a> {
        Aria { field, invalid: self.has(field) }
    }

    /// `<p class="field-error" id="{field}-error">message</p>` when `field` has an error, else
    /// nothing: the element [`Errors::aria`] points at.
    pub fn message<'a>(&'a self, field: &'a str) -> FieldMessage<'a> {
        FieldMessage { field, message: self.get(field) }
    }
}

/// The ARIA attributes of a form field, from [`Errors::aria`].
pub struct Aria<'a> {
    field: &'a str,
    invalid: bool,
}

impl Attrs for Aria<'_> {
    fn render_attrs(&self, out: &mut String) {
        if self.invalid {
            out.push_str(r#" aria-invalid="true" aria-describedby=""#);
            escape_into(self.field, out);
            out.push_str(r#"-error""#);
        }
    }
}

/// A field's error message, from [`Errors::message`].
pub struct FieldMessage<'a> {
    field: &'a str,
    message: Option<&'a str>,
}

impl Render for FieldMessage<'_> {
    fn render(&self, out: &mut String) {
        if let Some(message) = self.message {
            out.push_str(r#"<p class="field-error" id=""#);
            escape_into(self.field, out);
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
            map.serialize_entry(field, message)?;
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_keeps_its_first_message_and_order_is_kept() {
        let mut e = Errors::new();
        e.add("email", "Enter an email address.");
        e.add("price", "Enter a price.");
        e.add("email", "second");
        assert_eq!(e.get("email"), Some("Enter an email address."));
        assert_eq!(e.len(), 2);
        assert_eq!(e.to_string(), "email: Enter an email address.; price: Enter a price.");
        let mut out = String::new();
        e.aria("price").render_attrs(&mut out);
        e.aria("name").render_attrs(&mut out);
        assert_eq!(out, r#" aria-invalid="true" aria-describedby="price-error""#);
        assert_eq!(e.message("price").to_html(), r#"<p class="field-error" id="price-error">Enter a price.</p>"#);
        assert_eq!(e.message("name").to_html(), "");
        assert!(e.clone().into_result(1).is_err());
        assert_eq!(Errors::new().into_result(1), Ok(1));
    }

    #[cfg(feature = "json")]
    #[test]
    fn serializes_as_an_object() {
        let mut e = Errors::new();
        e.add("b", "2");
        e.add("a", "1");
        assert_eq!(serde_json::to_string(&e).unwrap(), r#"{"b":"2","a":"1"}"#);
    }
}
