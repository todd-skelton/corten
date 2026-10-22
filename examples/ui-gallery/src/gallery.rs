//! The page: every corten-ui island, wrapped and styled as a design system would.
use corten::prelude::*;
use corten_ui::{Choice, ComboBox, MenuButton, MenuItem, Number, NumberField, Picker, Tab, TabGroup};

use crate::{
    app::{OrderForm, url},
    field::Field,
};

pub const SETS: [Choice; 4] = [
    Choice { value: "sv1", label: "Scarlet & Violet" },
    Choice { value: "pal", label: "Paldea Evolved" },
    Choice { value: "obf", label: "Obsidian Flames" },
    Choice { value: "151", label: "Scarlet & Violet 151" },
];

/// The tabs, in order, by the `tab` query value that selects each.
pub const TABS: [&str; 3] = ["listings", "offers", "sales"];

#[component(css)]
pub fn Gallery<'a>(tab: usize, result: &'a str) -> impl Render {
    let f = OrderForm::FIELDS;
    let qty = Number {
        value: Some(1.0),
        min: Some(1.0),
        max: Some(10.0),
        required: true,
        ..Number::new("qty", f.qty, "Fewer", "More")
    };
    let price = Number { value: Some(12.5), min: Some(0.0), step: 0.01, ..Number::new("price", f.price, "Lower", "Higher") };
    let set = Picker { required: true, ..Picker::new("set", f.set, "Choose a set", "No matching set", &SETS) };
    let items = [
        MenuItem { label: &"Help", link: &url::Help, post: false },
        MenuItem { label: &"Archive", link: &url::Archive, post: true },
        MenuItem { label: &"Watch", link: &url::Watch, post: true },
    ];
    html! {
        <h1>"corten-ui"</h1>
        <form method="post" action={url::Order} data-patch>
            <Field id="qty" label="Quantity"><NumberField field={&qty} /></Field>
            <Field id="price" label="Price"><span class={Currency}>"$"</span><NumberField field={&price} /></Field>
            <Field id="set" label="Set"><ComboBox field={&set} /></Field>
            <button>"Order"</button>
        </form>
        <OrderResult text={result} />
        // Built in the markup, so the panels' views live until the group has rendered them.
        <div class={Book}>
            <TabGroup id="book" label="Market" selected={tab} tabs={&[
                Tab { label: &"Listings", panel: &html! { <p>"Three listings from $11.80."</p> }, link: Some(&url::Home { tab: None }) },
                Tab { label: &"Offers", panel: &html! { <p>"Two offers, the best at $10.00."</p> }, link: Some(&url::Home { tab: Some(TABS[1]) }) },
                Tab { label: &"Sales", panel: &html! { <p>"Sold 14 times this month."</p> }, link: Some(&url::Home { tab: Some(TABS[2]) }) },
            ]} />
        </div>
        <div class={Actions}><MenuButton id="actions" label={&"Actions"} items={&items} /></div>
        <MenuResult text="" />
    }
}

/// The order's outcome: in the page, and on its own as the form's patch.
#[component]
pub fn OrderResult<'a>(text: &'a str) -> impl Render {
    html! { <p id="result" role="status">{text}</p> }
}

/// What the last menu action did.
#[component]
pub fn MenuResult<'a>(text: &'a str) -> impl Render {
    html! { <p id="menu-result" role="status">{text}</p> }
}
