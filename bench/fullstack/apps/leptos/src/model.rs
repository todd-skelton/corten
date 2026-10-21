//! Same view model and rules as react-router/app/view-model.ts.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
    pub slug: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub lowest: Option<String>,
    pub count: i64,
    pub quantity: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    pub id: String,
    pub buyer: String,
    pub condition: String,
    pub quantity: i64,
    pub price: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContentLine {
    pub id: String,
    pub item_id: String,
    pub title: String,
    pub quantity: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    pub summary: Option<Summary>,
    pub listings: Vec<Listing>,
    pub offers: Vec<Offer>,
    pub contents: Vec<ContentLine>,
    pub included_in: Vec<ContentLine>,
}

pub fn sort_listings(listings: &[Listing], descending: bool) -> Vec<Listing> {
    let mut sorted = listings.to_vec();
    sorted.sort_by(|a, b| a.price_value.partial_cmp(&b.price_value).unwrap_or(std::cmp::Ordering::Equal));
    if descending {
        sorted.reverse();
    }
    sorted
}

pub fn alert_message(title: &str, email: &str, target: &str) -> String {
    let price: f64 = target.trim().parse().unwrap_or(f64::NAN);
    if !email.contains('@') {
        return "Enter a valid email address.".to_string();
    }
    if !(price > 0.0) {
        return "Enter a target price above zero.".to_string();
    }
    format!("Alert set: we will email {email} when {title} drops below ${price:.2}.")
}

#[cfg(feature = "ssr")]
mod from_api {
    use super::*;
    use serde_json::Value;

    fn text(v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        }
    }
    fn opt_text(v: &Value) -> Option<String> {
        v.as_str().map(str::to_string)
    }
    fn int(v: &Value) -> i64 {
        v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
    }
    fn money(amount: &Value, code: &Value) -> Option<String> {
        if amount.is_null() || code.is_null() {
            return None;
        }
        let amount = text(amount);
        let code = text(code);
        Some(if code == "USD" { format!("${amount}") } else { format!("{amount} {code}") })
    }
    fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
        v.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
    }
    fn line(c: &Value, id_key: &str, item_key: &str) -> ContentLine {
        let item_id = text(&c[id_key]);
        ContentLine {
            id: text(&c["line_id"]),
            title: c[item_key]["title"].as_str().map(str::to_string).unwrap_or_else(|| item_id.clone()),
            item_id,
            quantity: c["quantity"].as_i64().unwrap_or(1),
        }
    }

    impl ItemView {
        pub fn from_api(a: &Value) -> Self {
            let s = &a["market_summary"];
            ItemView {
                id: text(&a["catalog_item_id"]),
                title: text(&a["title"]),
                subtitle: opt_text(&a["subtitle"]),
                description: text(&a["description"]),
                badges: list(a, "display_badges").iter().map(|b| text(&b["label"])).collect(),
                tags: list(a, "tags").iter().map(text).collect(),
                categories: list(a, "categories")
                    .iter()
                    .map(|c| Category { name: text(&c["name"]), slug: text(&c["slug"]) })
                    .collect(),
                fields: list(a, "field_values")
                    .iter()
                    .map(|f| Field {
                        name: if f["fieldName"].is_null() { text(&f["fieldId"]) } else { text(&f["fieldName"]) },
                        value: match &f["value"] {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        },
                    })
                    .collect(),
                images: list(a, "image_urls").iter().take(2).map(text).collect(),
                summary: if s.is_null() {
                    None
                } else {
                    Some(Summary {
                        lowest: money(&s["lowest_price_amount"], &s["lowest_price_currency_code"]),
                        count: int(&s["active_listing_count"]),
                        quantity: int(&s["total_visible_quantity"]),
                    })
                },
                listings: list(a, "market_listings")
                    .iter()
                    .map(|l| Listing {
                        id: text(&l["listing_id"]),
                        seller: opt_text(&l["seller_display_name"]).unwrap_or_else(|| "Seller".into()),
                        rating: opt_text(&l["seller_average_rating"]),
                        reviews: int(&l["seller_review_count"]),
                        condition: text(&l["product_summary"]),
                        ships_from: text(&l["ship_from_code"]),
                        quantity: int(&l["visible_quantity"]),
                        price: money(&l["price_amount"], &l["price_currency_code"]).unwrap_or_default(),
                        price_value: text(&l["price_amount"]).parse().unwrap_or(f64::NAN),
                    })
                    .collect(),
                offers: list(a, "offer_demand_matches")
                    .iter()
                    .map(|o| Offer {
                        id: text(&o["offer_id"]),
                        buyer: opt_text(&o["buyer_display_name"]).unwrap_or_else(|| "Buyer".into()),
                        condition: text(&o["product_summary"]),
                        quantity: int(&o["quantity_requested"]),
                        price: money(&o["price_amount"], &o["price_currency_code"]).unwrap_or_default(),
                    })
                    .collect(),
                contents: list(a, "contents").iter().map(|c| line(c, "contained_catalog_item_id", "contained_item")).collect(),
                included_in: list(a, "included_in").iter().map(|c| line(c, "container_catalog_item_id", "container_item")).collect(),
            }
        }
    }
}
