//! Validation errors for a submitted form, shared by every surface.
//!
//! The rules stay in the app (they are business rules); Corten only carries the result to each
//! surface the same way: a browser gets the form re-rendered with messages next to its fields
//! and focus on the first one ([`crate::Req::invalid`]), a data client gets
//! `{"errors": {"field": "message"}}`, and an agent tool gets the same messages
//! (`ToolError::from(errors)`, feature `agent`).
//!
//! In the form template, mark each field and print its message with the id the field points at:
//!
//! ```text
//! <input name="email" {{ errors.aria("email")|safe }}>
//! {% if let Some(message) = errors.get("email") %}<p id="email-error">{{ message }}</p>{% endif %}
//! ```

use std::fmt;

use crate::escape;

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

    /// ` aria-invalid="true" aria-describedby="{field}-error"` when `field` has an error, else
    /// nothing. Print it with `|safe` inside the field's tag.
    pub fn aria(&self, field: &str) -> String {
        if self.has(field) {
            let id = escape(field);
            format!(r#" aria-invalid="true" aria-describedby="{id}-error""#)
        } else {
            String::new()
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
        assert_eq!(e.aria("price"), r#" aria-invalid="true" aria-describedby="price-error""#);
        assert_eq!(e.aria("name"), "");
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
