//! Copied from leptos/src/model.rs: same view model and rules as react-router/app/view-model.ts,
//! except the alert's price, read as the reader's locale writes money (`t.parse_money`) where
//! the bench apps read a float.
use corten::{Errors, Fields, FromJson, ToJson, i18n::Minor};

use crate::text::{Msg, USD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, ToJson)]
pub struct Category {
    pub name: String,
    pub slug: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, ToJson)]
pub struct Field {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, ToJson)]
pub struct Summary {
    pub lowest: Option<String>,
    pub count: i64,
    pub quantity: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, ToJson)]
pub struct Listing {
    pub id: String,
    pub seller: String,
    pub rating: Option<String>,
    pub reviews: i64,
    pub condition: String,
    pub ships_from: String,
    pub quantity: i64,
    pub price: String,
    pub price_value: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, ToJson)]
pub struct Offer {
    pub id: String,
    pub buyer: String,
    pub condition: String,
    pub quantity: i64,
    pub price: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, ToJson)]
pub struct ContentLine {
    pub id: String,
    pub item_id: String,
    pub title: String,
    pub quantity: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, ToJson)]
pub struct ItemView {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: String,
    pub badges: Vec<String>,
    pub tags: Vec<String>,
    pub categories: Vec<Category>,
    pub fields: Vec<Field>,
    pub images: Vec<String>,
    /// The store's own picture of the card (`static/cards/`), when the API sends none.
    #[serde(skip)]
    pub art: Option<&'static str>,
    pub summary: Option<Summary>,
    pub listings: Vec<Listing>,
    pub offers: Vec<Offer>,
    pub contents: Vec<ContentLine>,
    pub included_in: Vec<ContentLine>,
}

/// The store's own art for the cards of its demo catalog, a file in `static/cards/` (each one's
/// name is here, so a missing file is a test's failure, not a page's).
pub fn art(id: &str) -> Option<&'static str> {
    Some(match id {
        "item_1" => "cards/item_1.svg",
        "item_2" => "cards/item_2.svg",
        "item_3" => "cards/item_3.svg",
        "item_4" => "cards/item_4.svg",
        "item_5" => "cards/item_5.svg",
        "item_6" => "cards/item_6.svg",
        "item_7" => "cards/item_7.svg",
        "item_8" => "cards/item_8.svg",
        "item_9" => "cards/item_9.svg",
        "item_10" => "cards/item_10.svg",
        "item_11" => "cards/item_11.svg",
        "item_12" => "cards/item_12.svg",
        _ => return None,
    })
}

/// The listings by price, borrowed: sorting a page's rows copies no strings.
pub fn sort_listings(listings: &[Listing], descending: bool) -> Vec<&Listing> {
    let mut sorted: Vec<&Listing> = listings.iter().collect();
    sorted.sort_by(|a, b| a.price_value.partial_cmp(&b.price_value).unwrap_or(std::cmp::Ordering::Equal));
    if descending {
        sorted.reverse();
    }
    sorted
}

/// The price-alert form as it posts. `Fields` gives the template and the rules the field names.
#[derive(Deserialize, Fields)]
pub struct AlertInput {
    pub title: String,
    pub email: String,
    pub target_price: String,
}

/// The price-alert rules, shared by the form and the agent tool. Messages come from the catalog;
/// field names come from the form, and the agent tool's input uses the same names.
pub fn check_alert(t: Msg, email: &str, target: &str) -> Result<Minor<u64>, Errors> {
    let mut errors = Errors::new();
    if !email.contains('@') {
        errors.add(AlertInput::FIELDS.email, t.alert_invalid_email());
    }
    // The target as the reader's locale writes money (`12.50` in en, `12,50` in fr), in cents.
    let price = t.parse_money(target, USD).filter(|cents| cents.0 > 0);
    if price.is_none() {
        errors.add(AlertInput::FIELDS.target_price, t.alert_invalid_price());
    }
    errors.into_result(price.unwrap_or(Minor(0)))
}

/// What an alert set says, in the reader's locale, with the price as it writes money.
pub fn alert_set(t: Msg, title: &str, email: &str, price: Minor<u64>) -> String {
    t.alert_set(email, title, t.money(price, USD)).to_string()
}

/// The parts of the item API's response the page reads, read straight from the body by
/// `corten::json::from_slice` (`FromJson`, serde_json where its reader gives up). Every other
/// field is skipped without building it, and those are most of the 55 KB (the listings', offers'
/// and contents' other columns), so this costs about a quarter of parsing the whole response into
/// a `serde_json::Value` and walking it. Fields the API may send as a string
/// or a number stay `Value` and are read by the helpers below.
mod api {
    use super::*;
    use serde_json::Value;

    #[derive(Deserialize, FromJson, Default)]
    #[serde(default)]
    pub struct ApiItem {
        catalog_item_id: Value,
        title: Value,
        subtitle: Value,
        description: Value,
        display_badges: Option<Vec<ApiBadge>>,
        tags: Option<Vec<Value>>,
        categories: Option<Vec<ApiCategory>>,
        field_values: Option<Vec<ApiField>>,
        image_urls: Option<Vec<Value>>,
        market_summary: Option<ApiSummary>,
        market_listings: Option<Vec<ApiListing>>,
        offer_demand_matches: Option<Vec<ApiOffer>>,
        contents: Option<Vec<ApiLine>>,
        included_in: Option<Vec<ApiLine>>,
    }

    #[derive(Deserialize, FromJson, Default)]
    #[serde(default)]
    struct ApiBadge {
        label: Value,
    }

    #[derive(Deserialize, FromJson, Default)]
    #[serde(default)]
    struct ApiCategory {
        name: Value,
        slug: Value,
    }

    #[derive(Deserialize, FromJson, Default)]
    #[serde(default, rename_all = "camelCase")]
    struct ApiField {
        field_id: Value,
        field_name: Value,
        value: Value,
    }

    #[derive(Deserialize, FromJson, Default)]
    #[serde(default)]
    struct ApiSummary {
        lowest_price_amount: Value,
        lowest_price_currency_code: Value,
        active_listing_count: Value,
        total_visible_quantity: Value,
    }

    #[derive(Deserialize, FromJson, Default)]
    #[serde(default)]
    struct ApiListing {
        listing_id: Value,
        seller_display_name: Value,
        seller_average_rating: Value,
        seller_review_count: Value,
        product_summary: Value,
        ship_from_code: Value,
        visible_quantity: Value,
        price_amount: Value,
        price_currency_code: Value,
    }

    #[derive(Deserialize, FromJson, Default)]
    #[serde(default)]
    struct ApiOffer {
        offer_id: Value,
        buyer_display_name: Value,
        product_summary: Value,
        quantity_requested: Value,
        price_amount: Value,
        price_currency_code: Value,
    }

    #[derive(Deserialize, FromJson, Default)]
    #[serde(default)]
    struct ApiLine {
        line_id: Value,
        contained_catalog_item_id: Value,
        container_catalog_item_id: Value,
        contained_item: Option<ApiLineItem>,
        container_item: Option<ApiLineItem>,
        quantity: Value,
    }

    #[derive(Deserialize, FromJson, Default)]
    #[serde(default)]
    struct ApiLineItem {
        title: Value,
    }

    /// A string as is, null as "", anything else as its JSON.
    fn text(v: Value) -> String {
        match v {
            Value::String(s) => s,
            Value::Null => String::new(),
            other => other.to_string(),
        }
    }
    fn opt_text(v: Value) -> Option<String> {
        match v {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
    /// A number, or a string that holds one; else 0.
    fn int(v: &Value) -> i64 {
        v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
    }
    fn scalar(v: &Value) -> Option<std::borrow::Cow<'_, str>> {
        match v {
            Value::Null => None,
            Value::String(s) => Some(s.into()),
            other => Some(other.to_string().into()),
        }
    }
    fn money(amount: &Value, code: &Value) -> Option<String> {
        let (Some(amount), Some(code)) = (scalar(amount), scalar(code)) else { return None };
        Some(if code == "USD" { format!("${amount}") } else { format!("{amount} {code}") })
    }
    fn line(line_id: Value, item_id: Value, item: Option<ApiLineItem>, quantity: Value) -> ContentLine {
        let item_id = text(item_id);
        ContentLine {
            id: text(line_id),
            title: item.and_then(|i| opt_text(i.title)).unwrap_or_else(|| item_id.clone()),
            item_id,
            quantity: quantity.as_i64().unwrap_or(1),
        }
    }
    fn each<T, U>(list: Option<Vec<T>>, f: impl FnMut(T) -> U) -> Vec<U> {
        list.unwrap_or_default().into_iter().map(f).collect()
    }

    impl From<ApiItem> for ItemView {
        fn from(a: ApiItem) -> Self {
            let mut view = ItemView {
                id: text(a.catalog_item_id),
                title: text(a.title),
                subtitle: opt_text(a.subtitle),
                description: text(a.description),
                badges: each(a.display_badges, |b| text(b.label)),
                tags: each(a.tags, text),
                categories: each(a.categories, |c| Category { name: text(c.name), slug: text(c.slug) }),
                fields: each(a.field_values, |f| Field {
                    name: if f.field_name.is_null() { text(f.field_id) } else { text(f.field_name) },
                    value: match f.value {
                        Value::String(s) => s,
                        other => other.to_string(),
                    },
                }),
                images: a.image_urls.unwrap_or_default().into_iter().take(2).map(text).collect(),
                art: None,
                summary: a.market_summary.map(|s| Summary {
                    lowest: money(&s.lowest_price_amount, &s.lowest_price_currency_code),
                    count: int(&s.active_listing_count),
                    quantity: int(&s.total_visible_quantity),
                }),
                listings: each(a.market_listings, |l| Listing {
                    price: money(&l.price_amount, &l.price_currency_code).unwrap_or_default(),
                    price_value: scalar(&l.price_amount).and_then(|p| p.parse().ok()).unwrap_or(f64::NAN),
                    reviews: int(&l.seller_review_count),
                    quantity: int(&l.visible_quantity),
                    id: text(l.listing_id),
                    seller: opt_text(l.seller_display_name).unwrap_or_else(|| "Seller".into()),
                    rating: opt_text(l.seller_average_rating),
                    condition: text(l.product_summary),
                    ships_from: text(l.ship_from_code),
                }),
                offers: each(a.offer_demand_matches, |o| Offer {
                    price: money(&o.price_amount, &o.price_currency_code).unwrap_or_default(),
                    quantity: int(&o.quantity_requested),
                    id: text(o.offer_id),
                    buyer: opt_text(o.buyer_display_name).unwrap_or_else(|| "Buyer".into()),
                    condition: text(o.product_summary),
                }),
                contents: each(a.contents, |c| line(c.line_id, c.contained_catalog_item_id, c.contained_item, c.quantity)),
                included_in: each(a.included_in, |c| line(c.line_id, c.container_catalog_item_id, c.container_item, c.quantity)),
            };
            // The API's pictures come first; the store's art stands in only where it sends none.
            if view.images.is_empty() {
                view.art = super::art(&view.id);
            }
            view
        }
    }
}
pub use api::ApiItem;

#[cfg(test)]
mod tests {
    use super::*;

    /// Every card of the demo catalog has its art, and every art file is one the app serves.
    #[test]
    fn each_demo_card_has_art_the_app_serves() {
        let demo: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(include_str!("../fixtures/demo.json")).unwrap();
        for id in demo.keys() {
            let art = art(id).unwrap_or_else(|| panic!("{id} has no art"));
            let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("static").join(art);
            let svg = std::fs::read_to_string(&file).unwrap_or_else(|_| panic!("{} is missing", file.display()));
            assert!(svg.starts_with("<svg") && svg.contains(r#"viewBox="0 0 250 350""#), "{art}: a card, 5 by 7");
        }
    }

    /// The typed parse keeps the page's rules: null text is empty, counts may come as strings,
    /// money needs both an amount and a code, missing names fall back, two images at most, and a
    /// content line without its item's title shows the item id.
    #[test]
    fn api_item_reads_the_fields_the_page_uses() {
        let body = serde_json::json!({
            "catalog_item_id": "item_1",
            "title": "Box",
            "subtitle": null,
            "description": null,
            "unused": { "deep": [1, 2, 3] },
            "display_badges": [{ "label": "New", "color": "red" }],
            "tags": ["a", 7],
            "categories": [{ "name": "Cards", "slug": "cards", "depth": 1 }],
            "field_values": [
                { "fieldId": "f1", "fieldName": null, "value": 3 },
                { "fieldId": "f2", "fieldName": "Set", "value": "Base" }
            ],
            "image_urls": ["1.png", "2.png", "3.png"],
            "market_summary": { "lowest_price_amount": 1.5, "lowest_price_currency_code": "EUR", "active_listing_count": "4", "total_visible_quantity": 9 },
            "market_listings": [
                { "listing_id": "l1", "seller_display_name": null, "seller_review_count": "12", "price_amount": "2.50", "price_currency_code": "USD", "visible_quantity": 1, "extra": [null] },
                { "listing_id": "l2", "seller_display_name": "Ann", "seller_average_rating": "4.9", "product_summary": "NM", "ship_from_code": "US", "price_amount": null, "price_currency_code": "USD" }
            ],
            "offer_demand_matches": [{ "offer_id": "o1", "quantity_requested": 2, "price_amount": "1.00", "price_currency_code": null }],
            "contents": [
                { "line_id": "c1", "contained_catalog_item_id": "item_2", "contained_item": { "title": "Pack" }, "quantity": 3 },
                { "line_id": "c2", "contained_catalog_item_id": "item_3", "quantity": "2" }
            ],
            "included_in": [{ "line_id": "i1", "container_catalog_item_id": "item_4", "container_item": { "title": null } }]
        });
        let text = body.to_string();
        let item: ApiItem = serde_json::from_value(body).unwrap();
        let view = ItemView::from(item);
        // Compared as text: a NaN price is not equal to itself.
        let fast = ItemView::from(corten::json::from_str::<ApiItem>(&text).unwrap());
        assert_eq!(format!("{fast:?}"), format!("{view:?}"), "FromJson reads it as serde does");
        let line = |id: &str, item_id: &str, title: &str, quantity| ContentLine {
            id: id.into(),
            item_id: item_id.into(),
            title: title.into(),
            quantity,
        };
        assert_eq!(view.subtitle, None);
        assert_eq!(view.description, "");
        assert_eq!(view.badges, ["New"]);
        assert_eq!(view.tags, ["a", "7"]);
        assert_eq!(view.categories, [Category { name: "Cards".into(), slug: "cards".into() }]);
        assert_eq!(
            view.fields,
            [Field { name: "f1".into(), value: "3".into() }, Field { name: "Set".into(), value: "Base".into() }]
        );
        assert_eq!(view.images, ["1.png", "2.png"]);
        assert_eq!(view.art, None, "the API's pictures, not the store's art");
        assert_eq!(view.summary, Some(Summary { lowest: Some("1.5 EUR".into()), count: 4, quantity: 9 }));
        let (l1, l2) = (&view.listings[0], &view.listings[1]);
        assert_eq!((l1.seller.as_str(), l1.reviews, l1.price.as_str(), l1.price_value), ("Seller", 12, "$2.50", 2.5));
        assert_eq!(
            (l2.rating.as_deref(), l2.condition.as_str(), l2.ships_from.as_str(), l2.quantity),
            (Some("4.9"), "NM", "US", 0)
        );
        assert!(l2.price.is_empty() && l2.price_value.is_nan());
        assert_eq!(
            view.offers,
            [Offer { id: "o1".into(), buyer: "Buyer".into(), condition: String::new(), quantity: 2, price: String::new() }]
        );
        assert_eq!(view.contents, [line("c1", "item_2", "Pack", 3), line("c2", "item_3", "item_3", 1)]);
        assert_eq!(view.included_in, [line("i1", "item_4", "item_4", 1)]);
    }

    #[test]
    fn api_item_without_lists_or_summary_is_empty() {
        let view = ItemView::from(serde_json::from_str::<ApiItem>(r#"{"catalog_item_id":"x","market_summary":null}"#).unwrap());
        assert_eq!((view.id.as_str(), view.title.as_str()), ("x", ""));
        assert!(view.summary.is_none() && view.listings.is_empty() && view.contents.is_empty() && view.images.is_empty());
    }
}
