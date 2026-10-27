//! Each browser's watchlist, kept on the server under a random id in the `list` cookie. A change
//! is made to the stored list, under a lock, so two changes sent at once (two rows removed in
//! quick succession) both land; a list kept in the cookie itself would be read, changed and
//! written back by each, and the last answer would undo the other. A real app keeps a signed-in
//! user's list in its database; this one keeps the newest lists in memory.
use std::{
    collections::{BTreeMap, HashMap},
    convert::Infallible,
    sync::{Arc, Mutex},
};

use corten::{
    axum::{
        extract::{FromRef, FromRequestParts},
        http::{HeaderValue, request::Parts},
    },
    security::{
        Nonce,
        cookie::{self, SetCookie},
    },
};

use crate::market::Market;

/// What a browser watches before it changes anything.
const FIRST_WATCHED: [&str; 3] = ["ACME", "GLBX", "INIT"];

/// Lists kept at most; past it the one used longest ago is forgotten, and its browser starts
/// again from `FIRST_WATCHED`. Memory stays bounded however many clients post, and a list in
/// use is not pushed out by ones made and never used again.
const KEPT: usize = 10_000;

#[derive(Clone, Default)]
pub struct Lists(Arc<Mutex<Kept>>);

#[derive(Default)]
struct Kept {
    /// Each list and when it was last used. Tickers are the market's own strings, so reading a
    /// list copies pointers alone.
    lists: HashMap<String, (u64, Vec<&'static str>)>,
    /// The ids by when their lists were last used, longest ago first.
    used: BTreeMap<u64, String>,
    /// The next use's number.
    clock: u64,
}

impl Kept {
    /// Marks `id`'s list used now, and returns it.
    fn touch(&mut self, id: &str) -> Option<&mut Vec<&'static str>> {
        let (when, list) = self.lists.get_mut(id)?;
        let id = self.used.remove(when).expect("every list has a use");
        *when = self.clock;
        self.used.insert(self.clock, id);
        self.clock += 1;
        Some(list)
    }
}

impl Lists {
    fn lock(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The tickers `id`'s list holds, in the order they were added.
    fn get(&self, id: &str) -> Vec<&'static str> {
        self.lock().touch(id).map_or_else(|| FIRST_WATCHED.to_vec(), |list| list.clone())
    }

    /// Changes `id`'s list (made from `FIRST_WATCHED` if it has none) and returns it.
    fn change(&self, id: &str, change: impl FnOnce(&mut Vec<&'static str>)) -> Vec<&'static str> {
        let mut kept = self.lock();
        if !kept.lists.contains_key(id) {
            if kept.lists.len() >= KEPT
                && let Some((_, stale)) = kept.used.pop_first()
            {
                kept.lists.remove(&stale);
            }
            let now = kept.clock;
            kept.lists.insert(id.to_string(), (now, FIRST_WATCHED.to_vec()));
            kept.used.insert(now, id.to_string());
            kept.clock += 1;
        }
        let list = kept.touch(id).expect("inserted above");
        change(list);
        list.clone()
    }
}

/// The list of the browser asking: its id, from the `list` cookie, and its tickers. A browser
/// without one gets an id when its list changes or its answer sets the cookie, not before, so a
/// page that only reads the list (News) makes nothing.
pub struct Watching {
    id: Option<String>,
    /// The id was made for this request, so the answer sets the cookie.
    new: bool,
    pub ids: Vec<&'static str>,
}

impl Watching {
    pub fn has(&self, id: &str) -> bool {
        self.ids.contains(&id)
    }

    /// The list's id, made now if the browser has none.
    fn id(&mut self) -> &str {
        if self.id.is_none() {
            self.new = true;
        }
        // 128 random bits as 32 hex digits, from Corten's pool of OS randomness: an id no one
        // can guess.
        self.id.get_or_insert_with(|| Nonce::generate().0.to_string())
    }

    /// Adds `id` to the end of the stored list, once, if the market lists it.
    pub fn add(&mut self, lists: &Lists, id: &str) {
        let Some(ticker) = Market::ticker(id) else { return };
        self.ids = lists.change(self.id(), |ids| {
            if !ids.contains(&ticker) {
                ids.push(ticker);
            }
        });
    }

    /// Takes `id` off the stored list.
    pub fn remove(&mut self, lists: &Lists, id: &str) {
        self.ids = lists.change(self.id(), |ids| ids.retain(|watched| *watched != id));
    }

    /// The cookie that names the list, for an answer to a browser that had none: the watchlist
    /// sets it before the first change, so two first changes sent at once go to one list.
    pub fn cookie(&mut self) -> Option<HeaderValue> {
        self.id();
        let id = self.id.as_deref().filter(|_| self.new)?;
        Some(SetCookie::new("list", id).max_age(365 * 24 * 60 * 60).header())
    }
}

impl<S: Sync> FromRequestParts<S> for Watching
where
    Lists: FromRef<S>,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Infallible> {
        // An id this app made: 32 hex digits. Anything else is ignored and replaced.
        let id = cookie::get(&parts.headers, "list").filter(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()));
        let ids = id.map_or_else(|| FIRST_WATCHED.to_vec(), |id| Lists::from_ref(state).get(id));
        Ok(Watching { id: id.map(str::to_string), new: false, ids })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_made_at_once_both_land() {
        let lists = Lists::default();
        let (mut a, mut b) = (
            Watching { id: Some("x".into()), new: false, ids: vec![] },
            Watching { id: Some("x".into()), new: false, ids: vec![] },
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
        let kept = lists.lock();
        assert_eq!((kept.lists.len(), kept.used.len()), (KEPT, KEPT));
    }

    #[test]
    fn a_browser_without_a_list_gets_an_id_only_when_asked() {
        let mut reader = Watching { id: None, new: false, ids: FIRST_WATCHED.to_vec() };
        assert!(reader.has("ACME") && reader.id.is_none());
        let cookie = reader.cookie().expect("a new id is set");
        let id = reader.id.clone().unwrap();
        assert!(id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()), "{id}");
        assert!(cookie.to_str().unwrap().starts_with(&format!("list={id};")));
        let mut known = Watching { id: Some(id), new: false, ids: vec![] };
        assert!(known.cookie().is_none(), "a browser with a list keeps its id");
    }
}
