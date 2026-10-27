//! Each browser's watchlist, kept on the server under a random id in the `list` cookie. A change
//! is made to the stored list, under a lock, so two changes sent at once (two rows removed in
//! quick succession) both land; a list kept in the cookie itself would be read, changed and
//! written back by each, and the last answer would undo the other. A real app keeps a signed-in
//! user's list in its database; this one keeps the newest lists in memory.
use std::{
    collections::{HashMap, VecDeque},
    convert::Infallible,
    sync::{Arc, Mutex},
};

use corten::{
    axum::{
        extract::{FromRef, FromRequestParts},
        http::{HeaderValue, request::Parts},
    },
    security::cookie::{self, SetCookie},
};

use crate::market::Market;

/// What a browser watches before it changes anything.
const FIRST_WATCHED: [&str; 3] = ["ACME", "GLBX", "INIT"];

/// Lists kept at most; past it the oldest is forgotten, and its browser starts again from
/// `FIRST_WATCHED`. Memory stays bounded however many clients post.
const KEPT: usize = 10_000;

#[derive(Clone, Default)]
pub struct Lists(Arc<Mutex<Kept>>);

#[derive(Default)]
struct Kept {
    /// Tickers are the market's own strings, so reading a list copies pointers alone.
    lists: HashMap<String, Vec<&'static str>>,
    /// The ids in the order their lists were made, oldest first.
    order: VecDeque<String>,
}

impl Lists {
    fn lock(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The tickers `id`'s list holds, in the order they were added.
    fn get(&self, id: &str) -> Vec<&'static str> {
        self.lock().lists.get(id).cloned().unwrap_or_else(|| FIRST_WATCHED.to_vec())
    }

    /// Changes `id`'s list (made from `FIRST_WATCHED` if it has none) and returns it.
    fn change(&self, id: &str, change: impl FnOnce(&mut Vec<&'static str>)) -> Vec<&'static str> {
        let mut kept = self.lock();
        let kept = &mut *kept;
        if !kept.lists.contains_key(id) {
            if kept.order.len() >= KEPT
                && let Some(oldest) = kept.order.pop_front()
            {
                kept.lists.remove(&oldest);
            }
            kept.order.push_back(id.to_string());
            kept.lists.insert(id.to_string(), FIRST_WATCHED.to_vec());
        }
        let list = kept.lists.get_mut(id).expect("inserted above");
        change(list);
        list.clone()
    }
}

/// The list of the browser asking: its id, from the `list` cookie or made now, and its tickers.
pub struct Watching {
    id: String,
    /// The id was made for this request, so the answer sets the cookie.
    new: bool,
    pub ids: Vec<&'static str>,
}

impl Watching {
    pub fn has(&self, id: &str) -> bool {
        self.ids.contains(&id)
    }

    /// Adds `id` to the end of the stored list, once, if the market lists it.
    pub fn add(&mut self, lists: &Lists, id: &str) {
        let Some(ticker) = Market::ticker(id) else { return };
        self.ids = lists.change(&self.id, |ids| {
            if !ids.contains(&ticker) {
                ids.push(ticker);
            }
        });
    }

    /// Takes `id` off the stored list.
    pub fn remove(&mut self, lists: &Lists, id: &str) {
        self.ids = lists.change(&self.id, |ids| ids.retain(|watched| *watched != id));
    }

    /// The cookie that names the list, for an answer to a browser that had none: it is set
    /// before the first change, so two first changes sent at once go to one list.
    pub fn cookie(&self) -> Option<HeaderValue> {
        self.new.then(|| SetCookie::new("list", &self.id).max_age(365 * 24 * 60 * 60).header())
    }
}

impl<S: Sync> FromRequestParts<S> for Watching
where
    Lists: FromRef<S>,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Infallible> {
        // An id this app made: 32 hex digits. Anything else is ignored and replaced.
        let given = cookie::get(&parts.headers, "list").filter(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()));
        let (id, new) = match given {
            Some(id) => (id.to_string(), false),
            None => (random_id(), true),
        };
        let ids = Lists::from_ref(state).get(&id);
        Ok(Watching { id, new, ids })
    }
}

/// 128 random bits from the operating system, as 32 hex digits: an id no one can guess.
fn random_id() -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the OS random source is available");
    let mut id = String::with_capacity(32);
    for b in bytes {
        id.push(char::from(HEX[usize::from(b >> 4)]));
        id.push(char::from(HEX[usize::from(b & 15)]));
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_made_at_once_both_land() {
        let lists = Lists::default();
        let (mut a, mut b) =
            (Watching { id: "x".into(), new: false, ids: vec![] }, Watching { id: "x".into(), new: false, ids: vec![] });
        // Both read the list before either changed it, as two requests in flight do.
        a.remove(&lists, "ACME");
        b.remove(&lists, "GLBX");
        assert_eq!(b.ids, ["INIT"]);
        assert_eq!(lists.get("x"), ["INIT"]);
    }

    #[test]
    fn the_oldest_list_is_forgotten_past_the_limit() {
        let lists = Lists::default();
        for n in 0..=KEPT {
            lists.change(&n.to_string(), |ids| ids.clear());
        }
        assert_eq!(lists.get("0"), FIRST_WATCHED, "the oldest starts again");
        assert!(lists.get("1").is_empty() && lists.get(&KEPT.to_string()).is_empty());
        assert_eq!(lists.lock().lists.len(), KEPT);
    }
}
