//! The app's text. `messages!` reads the catalogs when the crate compiles: every locale must have
//! every key of the first with the same arguments, and each key is a method on `Msg`
//! (`item.listing_count` is `t.item_listing_count(count)`).
corten::i18n::messages! {
    pub struct Msg {
        "en" => "locales/en.json",
        "fr" => "locales/fr.json",
    }
}

/// The market's prices are in US dollars.
pub const USD: corten::i18n::Currency = corten::i18n::Currency::new("USD");

/// A condition in the reader's language when the catalog names it, else as the item API wrote it.
pub fn condition(t: Msg, grade: Option<crate::model::Grade>, sent: &str) -> &str {
    use crate::model::Grade;
    match grade {
        Some(Grade::NearMint) => t.grade_near_mint(),
        Some(Grade::LightlyPlayed) => t.grade_lightly_played(),
        Some(Grade::ModeratelyPlayed) => t.grade_moderately_played(),
        Some(Grade::HeavilyPlayed) => t.grade_heavily_played(),
        Some(Grade::Damaged) => t.grade_damaged(),
        Some(Grade::Sealed) => t.grade_sealed(),
        None => sent,
    }
}

/// A detail's label in the reader's language when the catalog has it, else the item API's.
pub fn detail(t: Msg, field: &crate::model::Field) -> &str {
    use crate::model::Detail;
    match field.detail {
        Some(Detail::Set) => t.detail_set(),
        Some(Detail::Number) => t.detail_number(),
        Some(Detail::Rarity) => t.detail_rarity(),
        Some(Detail::Finish) => t.detail_finish(),
        Some(Detail::Packs) => t.detail_packs(),
        None => &field.name,
    }
}
