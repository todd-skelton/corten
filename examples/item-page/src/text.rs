//! The app's text. `messages!` reads the catalogs when the crate compiles: every locale must have
//! every key of the first with the same arguments, and each key is a method on `Msg`
//! (`item.listing_count` is `t.item_listing_count(count)`).
corten::i18n::messages! {
    pub struct Msg {
        "en" => "locales/en.json",
        "fr" => "locales/fr.json",
    }
}
