//! Which browser is asking: a random id in the `visitor` cookie, under which the server keeps the
//! browser's watchlist and alerts. A browser gets one when it first keeps something, not before,
//! so a page that only reads (News) makes nothing. A real app keys a signed-in user's things by
//! their account; this one keys a browser's by this id.
use std::convert::Infallible;

use corten::{
    HtmlResponse,
    axum::{extract::FromRequestParts, http::request::Parts},
    security::{
        Nonce,
        cookie::{self, SetCookie},
    },
};

pub struct Visitor {
    id: Option<String>,
    /// The id was made for this request, so the answer sets the cookie.
    new: bool,
}

impl Visitor {
    /// The id the browser sent, if it has one.
    pub fn known(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// The id, made now if the browser has none.
    pub fn id(&mut self) -> &str {
        if self.id.is_none() {
            self.new = true;
        }
        // 128 random bits as 32 hex digits, from Corten's pool of OS randomness: an id no one
        // can guess.
        self.id.get_or_insert_with(|| Nonce::generate().0.to_string())
    }

    /// `res` with the cookie that names the browser, for an answer to one that had none: it is set
    /// before the first change is kept, so two first changes sent at once go to one id.
    pub fn with_cookie(&mut self, res: HtmlResponse) -> HtmlResponse {
        match self.new_cookie() {
            Some(cookie) => res.with_cookie(cookie),
            None => res,
        }
    }

    /// The cookie, when this browser's id is new.
    fn new_cookie(&mut self) -> Option<SetCookie> {
        self.id();
        let id = self.id.as_deref().filter(|_| self.new)?;
        Some(SetCookie::new("visitor", id).max_age(365 * 24 * 60 * 60))
    }

    #[cfg(test)]
    pub(crate) fn of(id: Option<&str>) -> Self {
        Visitor { id: id.map(str::to_string), new: false }
    }
}

impl<S: Sync> FromRequestParts<S> for Visitor {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Infallible> {
        // An id this app made: 32 hex digits. Anything else is ignored and replaced.
        let id = cookie::get(&parts.headers, "visitor").filter(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()));
        Ok(Visitor { id: id.map(str::to_string), new: false })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_browser_without_an_id_gets_one_only_when_asked() {
        let mut reader = Visitor::of(None);
        assert!(reader.known().is_none());
        let cookie = reader.new_cookie().expect("a new id is set").header();
        let id = reader.known().unwrap().to_string();
        assert!(id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()), "{id}");
        assert!(cookie.to_str().unwrap().starts_with(&format!("visitor={id};")));
        let mut known = Visitor::of(Some(&id));
        assert!(known.new_cookie().is_none(), "a browser with an id keeps it");
    }
}
