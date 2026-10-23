//! Where items come from: the Rust item API (`API_BASE`), or a JSON fixture of API responses
//! loaded into memory at startup (`ITEM_SOURCE=fixture`, `FIXTURE_PATH`), for benchmarks that
//! measure the web server alone.
use std::{collections::HashMap, sync::Arc};

use crate::model::{ApiItem, ItemView};

#[derive(Clone)]
pub enum Source {
    Api { client: reqwest::Client, base: Arc<str> },
    Fixture(Arc<HashMap<String, Arc<ItemView>>>),
}

impl Source {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        if std::env::var("ITEM_SOURCE").as_deref() == Ok("fixture") {
            let path = std::env::var("FIXTURE_PATH").unwrap_or_else(|_| "fixtures/items.json".into());
            return Self::fixture(&std::fs::read_to_string(path)?);
        }
        let base = std::env::var("API_BASE").unwrap_or_else(|_| "http://localhost:8080".into());
        Ok(Source::Api { client: reqwest::Client::new(), base: base.into() })
    }

    /// `{"item_1": <API response>, ...}`, converted one item at a time so the parsed responses for
    /// the whole file never sit in memory at once. Same typed parse as the API path.
    pub fn fixture(json: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let raw: HashMap<String, &serde_json::value::RawValue> = serde_json::from_str(json)?;
        let mut items = HashMap::with_capacity(raw.len());
        for (id, value) in raw {
            let item: ApiItem = serde_json::from_str(value.get())?;
            items.insert(id, Arc::new(ItemView::from(item)));
        }
        Ok(Source::Fixture(Arc::new(items)))
    }

    /// `None` when there is no such item. Shared, so a page borrows from it instead of copying.
    pub async fn item(&self, id: &str) -> Result<Option<Arc<ItemView>>, reqwest::Error> {
        match self {
            Source::Fixture(items) => Ok(items.get(id).cloned()),
            Source::Api { client, base } => {
                let res = client.get(format!("{base}/api/discovery/items/{id}")).send().await?;
                if res.status() == reqwest::StatusCode::NOT_FOUND {
                    return Ok(None);
                }
                // Straight into the fields the page reads; see `ApiItem`.
                let item: ApiItem = res.error_for_status()?.json().await?;
                Ok(Some(Arc::new(ItemView::from(item))))
            }
        }
    }
}
