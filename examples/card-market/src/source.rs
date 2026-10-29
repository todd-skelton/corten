//! Where items come from: the item API (`API_BASE`), a JSON fixture of API responses loaded
//! into memory at startup (`ITEM_SOURCE=fixture`, `FIXTURE_PATH`), for benchmarks that measure
//! the web server alone, or, with neither set, the demo catalog built into the app
//! (`fixtures/demo.json`), so `corten dev` shows a store with no item API running.
use std::{collections::HashMap, sync::Arc};

use crate::model::{ApiItem, ItemView};

#[derive(Clone)]
pub enum Source {
    Api { client: reqwest::Client, base: Arc<str> },
    Fixture(Arc<Catalog>),
}

/// Items held in memory: by id, and in id order (`item_2` before `item_10`) for the pages that
/// list them.
pub struct Catalog {
    items: HashMap<String, Arc<ItemView>>,
    order: Vec<Arc<ItemView>>,
}

/// What the home page lists: the items, when the source holds them, or else the first ids (the
/// item API looks items up one at a time and has no list).
pub enum Shelf<'a> {
    Items(&'a [Arc<ItemView>]),
    Ids(std::ops::RangeInclusive<u32>),
}

/// Items the home page lists at most.
const SHELF: usize = 50;

impl Source {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        if std::env::var("ITEM_SOURCE").as_deref() == Ok("fixture") {
            let path = std::env::var("FIXTURE_PATH").unwrap_or_else(|_| "fixtures/items.json".into());
            return Self::fixture(&std::fs::read_to_string(path)?);
        }
        match std::env::var("API_BASE") {
            Ok(base) => Ok(Source::Api { client: reqwest::Client::new(), base: base.into() }),
            Err(_) => Ok(Self::demo()),
        }
    }

    /// The demo catalog built into the app, with the store's own art for each of its cards
    /// (`model::art`). Only here: another source's `item_3` is another card.
    pub fn demo() -> Self {
        Self::load(include_str!("../fixtures/demo.json"), crate::model::art).expect("the demo catalog parses")
    }

    /// `{"item_1": <API response>, ...}`, converted one item at a time so the parsed responses for
    /// the whole file never sit in memory at once. Same typed parse as the API path.
    pub fn fixture(json: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Self::load(json, |_| None)
    }

    /// A catalog from `json`, with `art` for each item the responses give no pictures of.
    fn load(json: &str, art: fn(&str) -> Option<&'static str>) -> Result<Self, Box<dyn std::error::Error>> {
        let raw: HashMap<String, &serde_json::value::RawValue> = serde_json::from_str(json)?;
        let mut items = HashMap::with_capacity(raw.len());
        for (id, value) in raw {
            let item: ApiItem = corten::json::from_str(value.get())?;
            let mut view = ItemView::from(item);
            if view.images.is_empty() {
                view.art = art(&view.id);
            }
            items.insert(id, Arc::new(view));
        }
        let mut order: Vec<_> = items.iter().collect();
        order.sort_by(|(a, _), (b, _)| (a.len(), a).cmp(&(b.len(), b)));
        let order = order.into_iter().map(|(_, item)| item.clone()).collect();
        Ok(Source::Fixture(Arc::new(Catalog { items, order })))
    }

    /// The items the home page lists.
    pub fn shelf(&self) -> Shelf<'_> {
        match self {
            Source::Fixture(catalog) => Shelf::Items(&catalog.order[..catalog.order.len().min(SHELF)]),
            Source::Api { .. } => Shelf::Ids(1..=SHELF as u32),
        }
    }

    /// The items in a category, in id order; none from the item API, which has no list.
    pub fn in_category(&self, slug: &str) -> Vec<Arc<ItemView>> {
        let order = match self {
            Source::Fixture(catalog) => &catalog.order[..],
            Source::Api { .. } => &[],
        };
        order.iter().filter(|item| item.categories.iter().any(|c| c.slug == slug)).cloned().collect()
    }

    /// Whether items can be read: the fixture always, the item API when it answers at all (any
    /// status). What `/health/ready` reports, so a load balancer sends no traffic to an instance
    /// that would fail every item page.
    pub async fn reachable(&self) -> bool {
        match self {
            Source::Fixture(_) => true,
            Source::Api { client, base } => client.get(&**base).timeout(std::time::Duration::from_secs(2)).send().await.is_ok(),
        }
    }

    /// `None` when there is no such item. Shared, so a page borrows from it instead of copying.
    pub async fn item(&self, id: &str) -> Result<Option<Arc<ItemView>>, SourceError> {
        match self {
            Source::Fixture(catalog) => Ok(catalog.items.get(id).cloned()),
            Source::Api { client, base } => {
                let res = client.get(format!("{base}/api/discovery/items/{id}")).send().await?;
                if res.status() == reqwest::StatusCode::NOT_FOUND {
                    return Ok(None);
                }
                // Straight into the fields the page reads; see `ApiItem`.
                let item: ApiItem = corten::json::from_slice(&res.error_for_status()?.bytes().await?)?;
                Ok(Some(Arc::new(ItemView::from(item))))
            }
        }
    }
}

/// The item API could not be reached or answered with an error, or its answer is not an item.
#[derive(Debug)]
pub enum SourceError {
    Http(reqwest::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceError::Http(e) => write!(f, "item API: {e}"),
            SourceError::Json(e) => write!(f, "item API response: {e}"),
        }
    }
}

impl std::error::Error for SourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SourceError::Http(e) => Some(e),
            SourceError::Json(e) => Some(e),
        }
    }
}

impl From<reqwest::Error> for SourceError {
    fn from(e: reqwest::Error) -> Self {
        SourceError::Http(e)
    }
}

impl From<serde_json::Error> for SourceError {
    fn from(e: serde_json::Error) -> Self {
        SourceError::Json(e)
    }
}
