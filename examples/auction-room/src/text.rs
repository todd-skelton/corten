//! What a literal cannot be: a count of bids with its plural, and the locale money and times are
//! written in. The auction has one locale, so its other text is written in the views.
corten::i18n::messages! {
    pub struct Msg {
        "en" => "locales/en.json",
    }
}

/// The auction's prices are in US dollars.
pub const USD: corten::i18n::Currency = corten::i18n::Currency::new("USD");
