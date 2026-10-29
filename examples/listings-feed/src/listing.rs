//! The listing row, stored as `listings.data` and sent on the feed, with deterministic samples.

use corten::feed::Row;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A seller's listing, what the feed sends.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Listing {
    pub id: String,
    pub title: String,
    pub category: String,
    pub condition: String,
    pub price_cents: i64,
    pub market_cents: i64,
    pub quantity: i32,
    pub status: String,
    /// Milliseconds since the Unix epoch; set by the write that last changed the row.
    pub updated_at: u64,
}

impl Row for Listing {
    fn id(&self) -> &str {
        &self.id
    }
}

const NAMES: [&str; 12] = [
    "Desk Lamp",
    "Notebook",
    "Water Bottle",
    "Backpack",
    "Headphones",
    "Coffee Grinder",
    "Office Chair",
    "Monitor Stand",
    "Keyboard",
    "Wall Clock",
    "Plant Pot",
    "Bookshelf",
];
const CATEGORIES: [&str; 6] = ["Home", "Office", "Kitchen", "Outdoor", "Audio", "Furniture"];
const CONDITIONS: [&str; 5] = ["new", "open box", "used", "refurbished", "for parts"];

/// xorshift64*: deterministic, no dependency.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Milliseconds since the Unix epoch.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn id(n: usize) -> String {
    format!("lst_{n:06}")
}

/// `count` listings, the same for the same count.
pub fn generate(count: usize, now: u64) -> Vec<Listing> {
    let mut rng = Rng::new(count as u64 * 7919);
    (1..=count)
        .map(|n| {
            let price = 25 + (rng.below(1_000) as i64).pow(3) / 20_000;
            Listing {
                id: id(n),
                title: NAMES[rng.below(NAMES.len())].to_string(),
                category: CATEGORIES[rng.below(CATEGORIES.len())].to_string(),
                condition: CONDITIONS[rng.below(CONDITIONS.len())].to_string(),
                price_cents: price,
                market_cents: (price as f64 * (0.8 + rng.below(400) as f64 / 1_000.0)).round().max(1.0) as i64,
                quantity: 1 + rng.below(12) as i32,
                status: if rng.below(10) == 0 { "paused" } else { "active" }.to_string(),
                updated_at: now,
            }
        })
        .collect()
}

/// Next market low for a listing: a drift of -3% to +3%.
pub fn drift(market_cents: i64, roll: u64) -> i64 {
    let factor = 0.97 + (roll % 61) as f64 / 1_000.0;
    ((market_cents as f64 * factor).round() as i64).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_deterministic_and_drift_stays_within_three_percent() {
        let rows = generate(20, 5);
        assert_eq!(rows, generate(20, 5));
        assert_eq!(rows[0].id, "lst_000001");
        assert!(rows.iter().all(|r| r.updated_at == 5 && r.market_cents >= 1));
        assert!((0..1000).all(|roll| (970..=1030).contains(&drift(1000, roll))));
    }
}
