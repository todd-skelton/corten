//! [`Json`]: a JSON response or request body, as axum's `Json`, written in one pass.
//!
//! axum's `Json` serializes through a `BytesMut` writer, which costs more for each of the many
//! small writes `serde_json` makes (every key, quote and comma). This one serializes into a
//! `Vec`, which the response body takes as it is. As an extractor it is axum's, with the same
//! rejections (under [`crate::App::api`] they are answered as JSON errors).
//!
//! ```
//! use corten::prelude::*;
//!
//! #[derive(serde::Serialize)]
//! struct Price { id: String, lowest: &'static str }
//!
//! async fn price(Path(id): Path<String>) -> Json<Price> {
//!     Json(Price { id, lowest: "$0.50" })
//! }
//! ```

use axum::{
    body::Body,
    extract::{rejection::JsonRejection, FromRequest, Request},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{de::DeserializeOwned, Serialize};

/// JSON out (`application/json`) or in (a request body with a JSON content type).
#[derive(Clone, Copy, Debug, Default)]
#[must_use]
pub struct Json<T>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        match serde_json::to_vec(&self.0) {
            Ok(body) => {
                // `Body::from(Vec)` keeps the vector's allocation: no copy.
                let mut res = Response::new(Body::from(body));
                // Room for the headers `App` adds too, so the map is allocated once.
                res.headers_mut().reserve(12);
                res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(crate::api::JSON));
                res
            }
            // What axum's `Json` answers: a 500 with the error as text (a JSON error under an API
            // prefix, with the text logged).
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"))],
                error.to_string(),
            )
                .into_response(),
        }
    }
}

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for Json<T> {
    type Rejection = JsonRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let axum::Json(value) = axum::Json::<T>::from_request(req, state).await?;
        Ok(Json(value))
    }
}

impl<T> From<T> for Json<T> {
    fn from(value: T) -> Self {
        Json(value)
    }
}

impl<T> std::ops::Deref for Json<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn writes_what_axum_writes() {
        let value = serde_json::json!({ "id": "item_1", "price": 0.5, "tags": ["a", "<b>"], "none": null });
        let ours = Json(&value).into_response();
        let theirs = axum::Json(&value).into_response();
        assert_eq!(ours.status(), theirs.status());
        assert_eq!(ours.headers()[header::CONTENT_TYPE], theirs.headers()[header::CONTENT_TYPE]);
        assert_eq!(axum::body::HttpBody::size_hint(ours.body()).exact(), axum::body::HttpBody::size_hint(theirs.body()).exact());
        let (ours, theirs) =
            (to_bytes(ours.into_body(), usize::MAX).await.unwrap(), to_bytes(theirs.into_body(), usize::MAX).await.unwrap());
        assert_eq!(ours, theirs);
    }

    #[tokio::test]
    async fn a_value_that_cannot_be_written_is_a_500_with_the_reason() {
        let mut map = std::collections::HashMap::new();
        map.insert((1, 2), "a tuple key is not a JSON key");
        let res = Json(map).into_response();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(res.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/plain"));
    }

    #[tokio::test]
    async fn reads_a_body_as_axum_does() {
        let req =
            |ct: &str, body: &'static str| Request::post("/").header(header::CONTENT_TYPE, ct).body(Body::from(body)).unwrap();
        let Json(v): Json<serde_json::Value> = Json::from_request(req("application/json", r#"{"a":1}"#), &()).await.unwrap();
        assert_eq!(v["a"], 1);
        let missing = Json::<serde_json::Value>::from_request(req("text/plain", "{}"), &()).await.unwrap_err();
        assert_eq!(missing.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let bad = Json::<serde_json::Value>::from_request(req("application/json", "{"), &()).await.unwrap_err();
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    }
}
