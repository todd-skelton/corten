//! The collection a player builds from, and the deck rules. The rules live here, on the server,
//! alone: the browser shows a change at once and sends it, and a change these rules refuse is
//! rolled back with the message they give.
use std::collections::BTreeMap;

/// A card's kind, which the deck counts by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Unit,
    Spell,
    Relic,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Unit, Kind::Spell, Kind::Relic];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Unit => "Unit",
            Kind::Spell => "Spell",
            Kind::Relic => "Relic",
        }
    }
}

/// How rare a card is: a mythic goes in a deck once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Mythic,
}

impl Rarity {
    pub fn as_str(self) -> &'static str {
        match self {
            Rarity::Common => "Common",
            Rarity::Uncommon => "Uncommon",
            Rarity::Rare => "Rare",
            Rarity::Mythic => "Mythic",
        }
    }
}

/// A card in the player's collection: how many copies they own, and what it costs to play.
#[derive(Clone, Debug)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub set: &'static str,
    /// What it costs to play, 0 to 7 (7 is "7 or more" on the curve).
    pub cost: u32,
    pub kind: Kind,
    pub rarity: Rarity,
    pub owned: u32,
}

/// How many cards a deck holds at most.
pub const DECK_SIZE: u32 = 40;
/// How many copies of one card a deck holds at most (a mythic, one).
pub const MAX_COPIES: u32 = 3;
/// The highest cost the curve shows on its own; higher costs join it.
pub const TOP_COST: u32 = 7;

/// The sets of the game card-market sells.
const SETS: [&str; 3] = ["Ember Reach", "Hollow Crown", "Tidebreak"];

/// card-market's singles, with what this player has of them. `(name, set, cost, kind, rarity, owned)`.
const KNOWN: [(&str, &str, u32, Kind, Rarity, u32); 11] = [
    ("Ashen Drake", "Ember Reach", 5, Kind::Unit, Rarity::Rare, 2),
    ("Lantern Warden", "Hollow Crown", 2, Kind::Unit, Rarity::Uncommon, 3),
    ("Salt Oracle", "Tidebreak", 6, Kind::Unit, Rarity::Mythic, 1),
    ("Gravel Sprite", "Ember Reach", 1, Kind::Unit, Rarity::Common, 4),
    ("Crown of Thorns and Glass", "Hollow Crown", 3, Kind::Relic, Rarity::Rare, 1),
    ("Undertow Serpent", "Tidebreak", 4, Kind::Unit, Rarity::Common, 3),
    ("Cinder Cartographer", "Ember Reach", 2, Kind::Unit, Rarity::Uncommon, 2),
    ("Moth Court Herald", "Hollow Crown", 1, Kind::Unit, Rarity::Common, 4),
    ("Pearl Diver", "Tidebreak", 2, Kind::Unit, Rarity::Common, 4),
    ("Kiln Colossus", "Ember Reach", 7, Kind::Unit, Rarity::Rare, 1),
    ("Regent of Ash", "Hollow Crown", 6, Kind::Unit, Rarity::Mythic, 1),
];

/// The rest of the collection's names: a word and a figure or a spell. No word is a set's, so a
/// filter for a set finds that set's cards.
const WORDS: [&str; 16] = [
    "Fallow", "Rime", "Sable", "Iron", "Glass", "Thorn", "Silt", "Brine", "Ivory", "Copper", "Lichen", "Storm", "Dusk",
    "Marsh", "Quarry", "Gilded",
];
const UNITS: [&str; 8] = ["Hound", "Witch", "Pilgrim", "Sentinel", "Weaver", "Scholar", "Courier", "Lancer"];
const SPELLS: [&str; 5] = ["Bolt", "Tide", "Ward", "Hymn", "Rite"];
const RELICS: [&str; 3] = ["Lantern", "Chalice", "Compass"];

/// A number drawn from `k`, the same every run.
fn draw(k: u32) -> u32 {
    k.wrapping_mul(2_654_435_761).rotate_left(13) ^ k
}

/// The collection: card-market's singles first, then `size` cards in all.
pub fn collection(size: usize) -> Vec<Card> {
    let mut cards: Vec<Card> = KNOWN
        .iter()
        .map(|&(name, set, cost, kind, rarity, owned)| Card { id: slug(name), name: name.into(), set, cost, kind, rarity, owned })
        .collect();
    let mut k = 0u32;
    while cards.len() < size {
        let word = WORDS[k as usize % WORDS.len()];
        let roll = draw(k);
        let kind = match roll % 10 {
            0..=5 => Kind::Unit,
            6..=8 => Kind::Spell,
            _ => Kind::Relic,
        };
        let names: &[&str] = match kind {
            Kind::Unit => &UNITS,
            Kind::Spell => &SPELLS,
            Kind::Relic => &RELICS,
        };
        let name = format!("{word} {}", names[(k as usize / WORDS.len()) % names.len()]);
        k += 1;
        if cards.iter().any(|c| c.name == name) {
            if k > 10_000 {
                break;
            }
            continue;
        }
        let rarity = match (roll >> 8) % 100 {
            0..=54 => Rarity::Common,
            55..=82 => Rarity::Uncommon,
            83..=95 => Rarity::Rare,
            _ => Rarity::Mythic,
        };
        let owned = match rarity {
            Rarity::Common => 1 + (roll >> 16) % 4,
            Rarity::Uncommon => 1 + (roll >> 16) % 3,
            Rarity::Rare => 1 + (roll >> 16) % 2,
            Rarity::Mythic => 1,
        };
        // Most cards cost 1 to 4.
        let cost = [0, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 5, 5, 6, 7][(roll >> 20) as usize % 15];
        let set = SETS[(roll >> 24) as usize % SETS.len()];
        cards.push(Card { id: slug(&name), name, set, cost, kind, rarity, owned });
    }
    cards
}

/// `Ashen Drake` as `ashen-drake`.
fn slug(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

/// The deck a new player starts with, so the page is not empty: `(card id, copies)`.
pub const STARTER: [(&str, u32); 5] =
    [("gravel-sprite", 3), ("moth-court-herald", 2), ("lantern-warden", 2), ("cinder-cartographer", 2), ("ashen-drake", 1)];

/// Why a deck was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A card that is not in the collection.
    NoCard,
    /// More copies than a deck holds of one card.
    TooMany { name: String },
    /// A second copy of a mythic.
    OneMythic { name: String },
    /// More copies than the player owns.
    NotOwned { name: String, owned: u32 },
    /// More cards than a deck holds.
    Full,
    /// A count that is not a number of copies.
    NotANumber,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::NoCard => write!(f, "That card is not in your collection."),
            Refusal::TooMany { name } => write!(f, "A deck holds at most {MAX_COPIES} copies of {name}."),
            Refusal::OneMythic { name } => write!(f, "{name} is mythic: a deck holds one copy."),
            Refusal::NotOwned { name, owned: 1 } => write!(f, "You own 1 copy of {name}."),
            Refusal::NotOwned { name, owned } => write!(f, "You own {owned} copies of {name}."),
            Refusal::Full => write!(f, "A deck holds at most {DECK_SIZE} cards."),
            Refusal::NotANumber => write!(f, "Enter a number of copies, like 2."),
        }
    }
}

/// Checks a deck, `card id -> copies` (no zeros), against the rules and the collection.
pub fn check<'c>(deck: &BTreeMap<String, u32>, card: impl Fn(&str) -> Option<&'c Card>) -> Result<(), Refusal> {
    let mut total = 0u32;
    for (id, &copies) in deck {
        let card = card(id).ok_or(Refusal::NoCard)?;
        if card.rarity == Rarity::Mythic && copies > 1 {
            return Err(Refusal::OneMythic { name: card.name.clone() });
        }
        if copies > MAX_COPIES {
            return Err(Refusal::TooMany { name: card.name.clone() });
        }
        if copies > card.owned {
            return Err(Refusal::NotOwned { name: card.name.clone(), owned: card.owned });
        }
        total = total.saturating_add(copies);
    }
    if total > DECK_SIZE {
        return Err(Refusal::Full);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(pairs: &[(&str, u32)]) -> BTreeMap<String, u32> {
        pairs.iter().map(|&(id, n)| (id.to_string(), n)).collect()
    }

    #[test]
    fn the_collection_starts_with_card_markets_singles_and_names_each_card_once() {
        let cards = collection(240);
        assert_eq!(cards.len(), 240);
        assert_eq!((cards[0].id.as_str(), cards[0].owned), ("ashen-drake", 2));
        let mut ids: Vec<_> = cards.iter().map(|c| c.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 240, "no card twice");
        assert!(cards.iter().all(|c| c.cost <= TOP_COST && (1..=4).contains(&c.owned)));
        for kind in Kind::ALL {
            assert!(cards.iter().filter(|c| c.kind == kind).count() > 10, "{kind:?}");
        }
        for (id, copies) in STARTER {
            let card = cards.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("{id}"));
            assert!(copies <= card.owned, "{id}");
        }
    }

    #[test]
    fn the_rules_refuse_what_a_deck_cannot_hold_with_a_message() {
        let cards = collection(240);
        let card = |id: &str| cards.iter().find(|c| c.id == id);
        assert_eq!(check(&deck(&STARTER), card), Ok(()));
        assert_eq!(check(&deck(&[("no-such-card", 1)]), card), Err(Refusal::NoCard));
        let refused = check(&deck(&[("ashen-drake", 3)]), card).unwrap_err();
        assert_eq!(refused.to_string(), "You own 2 copies of Ashen Drake.");
        let refused = check(&deck(&[("gravel-sprite", 4)]), card).unwrap_err();
        assert_eq!(refused.to_string(), "A deck holds at most 3 copies of Gravel Sprite.");
        let refused = check(&deck(&[("salt-oracle", 2)]), card).unwrap_err();
        assert_eq!(refused.to_string(), "Salt Oracle is mythic: a deck holds one copy.");
        let refused = check(&deck(&[("kiln-colossus", 2)]), card).unwrap_err();
        assert_eq!(refused.to_string(), "You own 1 copy of Kiln Colossus.");
        let many: BTreeMap<String, u32> =
            cards.iter().filter(|c| c.owned >= 3 && c.rarity != Rarity::Mythic).take(14).map(|c| (c.id.clone(), 3)).collect();
        assert_eq!(check(&many, card), Err(Refusal::Full));
    }
}