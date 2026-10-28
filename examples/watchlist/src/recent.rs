//! What the server keeps per browser, bounded: past a limit, the browser whose things were used
//! longest ago is forgotten, so memory stays bounded however many clients post. Lists and alerts
//! both keep theirs here.
use std::collections::{BTreeMap, HashMap};

pub struct Recent<V> {
    /// Each browser's value and when it was last used.
    values: HashMap<String, (u64, V)>,
    /// The ids by when their values were last used, longest ago first.
    used: BTreeMap<u64, String>,
    /// The next use's number.
    clock: u64,
    limit: usize,
}

impl<V> Recent<V> {
    pub fn new(limit: usize) -> Self {
        Recent { values: HashMap::new(), used: BTreeMap::new(), clock: 0, limit }
    }

    /// `id`'s value, marked used now.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut V> {
        let (when, value) = self.values.get_mut(id)?;
        let id = self.used.remove(when).expect("every value has a use");
        *when = self.clock;
        self.used.insert(self.clock, id);
        self.clock += 1;
        Some(value)
    }

    /// `id`'s value, made by `make` if it has none (forgetting the one used longest ago when
    /// full), marked used now.
    pub fn entry(&mut self, id: &str, make: impl FnOnce() -> V) -> &mut V {
        if !self.values.contains_key(id) {
            if self.values.len() >= self.limit
                && let Some((_, stale)) = self.used.pop_first()
            {
                self.values.remove(&stale);
            }
            self.values.insert(id.to_string(), (self.clock, make()));
            self.used.insert(self.clock, id.to_string());
            self.clock += 1;
        }
        self.get_mut(id).expect("made above")
    }

    #[cfg(test)]
    /// How many values and uses it holds, which are always the same.
    pub fn sizes(&self) -> (usize, usize) {
        (self.values.len(), self.used.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_value_used_longest_ago_is_forgotten_past_the_limit() {
        let mut kept = Recent::new(3);
        for id in ["a", "b", "c"] {
            *kept.entry(id, || 0) += 1;
        }
        // "a" is used again, so "b" is now the one used longest ago.
        assert_eq!(kept.get_mut("a"), Some(&mut 1));
        kept.entry("d", || 0);
        assert!(kept.get_mut("b").is_none());
        assert!(kept.get_mut("a").is_some() && kept.get_mut("c").is_some() && kept.get_mut("d").is_some());
        assert_eq!(kept.sizes(), (3, 3));
    }
}
