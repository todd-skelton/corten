//! Each browser's watchlist, kept on the server under its visitor id (`visitor.rs`). A change
//! is made to the stored list, under a lock, so two changes sent at once (two rows removed in
//! quick succession) both land; a list kept in the cookie itself would be read, changed and
//! written back by each, and the last answer would undo the other. A real app keeps a signed-in
//! user's list in its database; this one keeps the newest lists in memory.
use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
};

use corten::{
    HtmlResponse,
    axum::{
        extract::{FromRef, FromRequestParts},
        http::request::Parts,
    },
};

use crate::{market::Market, recent::Recent, visitor::Visitor};

/// What a browser watches before it changes anything.
const FIRST_WATCHED: [&str; 3] = ["ACME", "GLBX", "INIT"];

/// Lists kept at most; past it the one used longest ago is forgotten (`Recent`), and its browser
/// starts again from `FIRST_WATCHED`.
const KEPT: usize = 10_000;

/// Each browser's list. Tickers are the market's own strings, so reading a list copies pointers
/// alone.
#[derive(Clone)]
pub struct Lists(Arc<Mutex<Recent<Vec<&'static str>>>>);

impl Default for Lists {
    fn default() -> Self {
        Lists(Arc::new(Mutex::new(Recent::new(KEPT))))
    }
}

impl Lists {
    fn lock(&self) -> std::sync::MutexGuard<'_, Recent<Vec<&'static str>>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The tickers `id`'s list holds, in the order they were added.
    fn get(&self, id: &str) -> Vec<&'static str> {
        self.lock().get_mut(id).map_or_else(|| FIRST_WATCHED.to_vec(), |list| list.clone())
    }

    /// Changes `id`'s list (made from `FIRST_WATCHED` if it has none) and returns it.
    fn change(&self, id: &str, change: impl FnOnce(&mut Vec<&'static str>)) -> Vec<&'static str> {
        let mut kept = self.lock();
        let list = kept.entry(id, || FIRST_WATCHED.to_vec());
        change(list);
        list.clone()
    }
}

/// The list of the browser asking, and who it is (`Visitor`). A browser without an id gets one
/// when its list changes or its answer sets the cookie, not before.
pub struct Watching {
    visitor: Visitor,
    pub ids: Vec<&'static str>,
}

impl Watching {
    pub fn has(&self, id: &str) -> bool {
        self.ids.contains(&id)
    }

    /// Adds `id` to the end of the stored list, once, if the market lists it.
    pub fn add(&mut self, lists: &Lists, id: &str) {
        let Some(ticker) = Market::ticker(id) else { return };
        self.ids = lists.change(self.visitor.id(), |ids| {
            if !ids.contains(&ticker) {
                ids.push(ticker);
            }
        });
    }

    /// Takes `id` off the stored list.
    pub fn remove(&mut self, lists: &Lists, id: &str) {
        self.ids = lists.change(self.visitor.id(), |ids| ids.retain(|watched| *watched != id));
    }

    /// `res` with the cookie that names the browser, when it had none (`Visitor::with_cookie`).
    pub fn with_cookie(&mut self, res: HtmlResponse) -> HtmlResponse {
        self.visitor.with_cookie(res)
    }
}

impl<S: Sync> FromRequestParts<S> for Watching
where
    Lists: FromRef<S>,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Infallible> {
        let visitor = Visitor::from_request_parts(parts, state).await?;
        let ids = visitor.known().map_or_else(|| FIRST_WATCHED.to_vec(), |id| Lists::from_ref(state).get(id));
        Ok(Watching { visitor, ids })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_made_at_once_both_land() {
        let lists = Lists::default();
        let (mut a, mut b) = (
            Watching { visitor: Visitor::of(Some("x")), ids: vec![] },
            Watching { visitor: Visitor::of(Some("x")), ids: vec![] },
        );
        // Both read the list before either changed it, as two requests in flight do.
        a.remove(&lists, "ACME");
        b.remove(&lists, "GLBX");
        assert_eq!(b.ids, ["INIT"]);
        assert_eq!(lists.get("x"), ["INIT"]);
    }

    #[test]
    fn the_list_used_longest_ago_is_forgotten_past_the_limit() {
        let lists = Lists::default();
        for n in 0..KEPT {
            lists.change(&n.to_string(), |ids| ids.clear());
        }
        // The first list made is read again, so the second is now the one used longest ago.
        assert!(lists.get("0").is_empty());
        lists.change("new", |ids| ids.clear());
        assert_eq!(lists.get("1"), FIRST_WATCHED, "the stalest starts again");
        assert!(lists.get("0").is_empty() && lists.get("new").is_empty());
        assert_eq!(lists.lock().len(), (KEPT, KEPT));
    }
}
