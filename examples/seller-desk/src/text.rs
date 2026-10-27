//! The statement's text and the locale its amounts are written in. The desk's other screens keep
//! their few words in the views; an amount of money needs a locale, which a catalog brings.
corten::i18n::messages! {
    pub struct Msg {
        "en" => "locales/en.json",
    }
}

/// The desk's prices are in US dollars.
pub const USD: corten::i18n::Currency = corten::i18n::Currency::new("USD");
