//! The page's few words that need a locale: the count of listings, and money.
corten::i18n::messages! {
    pub struct Msg {
        "en" => "locales/en.json",
    }
}

/// The listings' prices are in US dollars.
pub const USD: corten::i18n::Currency = corten::i18n::Currency::new("USD");
