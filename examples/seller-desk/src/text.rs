//! The statement's text, the count of unsaved prices the grid's script writes, and the locale
//! amounts are written in. The desk's other screens keep their few words in the views; an amount
//! of money needs a locale and a count needs its plural forms, which a catalog brings.
corten::i18n::messages! {
    pub struct Msg {
        "en" => "locales/en.json",
    }
}

/// The desk's prices are in US dollars.
pub const USD: corten::i18n::Currency = corten::i18n::Currency::new("USD");
