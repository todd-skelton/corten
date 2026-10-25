//! The page: every corten-ui component, wrapped and styled as a design system would.
use corten::prelude::*;
use corten_ui::{Choice, ComboBox, MenuButton, MenuItem, Number, NumberField, Picker, Tab, TabGroup, Toast, Toaster};

use crate::{
    app::{OrderForm, url},
    field::Field,
};

pub const COLORS: [Choice; 4] = [
    Choice { value: "salt", label: "Salt & Pepper" },
    Choice { value: "sand", label: "Desert Sand" },
    Choice { value: "forest", label: "Forest Green" },
    Choice { value: "gray", label: "Salt & Pepper Gray" },
];

/// The tabs, in order, by the `tab` query value that selects each.
pub const TABS: [&str; 3] = ["listings", "offers", "sales"];

#[component(css)]
pub fn Gallery<'a>(tab: usize, result: &'a str, toast: Option<&'a str>) -> impl Render {
    let f = OrderForm::FIELDS;
    let qty = Number {
        value: Some(1.0),
        min: Some(1.0),
        max: Some(10.0),
        required: true,
        ..Number::new("qty", f.qty, "Fewer", "More")
    };
    let price = Number { value: Some(12.5), min: Some(0.0), step: 0.01, ..Number::new("price", f.price, "Lower", "Higher") };
    let color = Picker { required: true, ..Picker::new("color", f.color, "Choose a color", "No matching color", &COLORS) };
    let items = [
        MenuItem { label: &"Help", link: &url::Help, post: false },
        MenuItem { label: &"Archive", link: &url::Archive, post: true },
        MenuItem { label: &"Watch", link: &url::Watch, post: true },
    ];
    html! {
        <h1>"corten-ui"</h1>
        <form method="post" action={url::Order} data-patch>
            <Field id="qty" label="Quantity"><NumberField field={&qty} /></Field>
            <Field id="price" label="Price">
                <span class={Currency}>"$"</span>
                <NumberField field={&price} />
            </Field>
            <Field id="color" label="Color"><ComboBox field={&color} /></Field>
            <button>"Order"</button>
        </form>
        <OrderResult text={result} />
        // Built in the markup, so the panels' views live until the group has rendered them.
        <div class={Book}>
            <TabGroup
                id="book"
                label="Market"
                selected={tab}
                tabs={&[
                    Tab { label: &"Listings", panel: &html! { <p>"Three listings from $11.80."</p> }, link: Some(&url::Home { tab: None }) },
                    Tab { label: &"Offers", panel: &html! { <p>"Two offers, the best at $10.00."</p> }, link: Some(&url::Home { tab: Some(TABS[1]) }) },
                    Tab { label: &"Sales", panel: &html! { <p>"Sold 14 times this month."</p> }, link: Some(&url::Home { tab: Some(TABS[2]) }) },
                ]}
            />
        </div>
        <div class={Actions}><MenuButton id="actions" label={&"Actions"} items={&items} /></div>
        <MenuResult text="" />
        <form method="post" action={url::Save} data-patch><button>"Save draft"</button></form>
        <div class={Toasts}>
            <Toaster close={&"Dismiss"} seconds={2} toast={Toast { message: toast.as_ref().map(|t| t as &dyn Render) }} />
        </div>
    }
}

/// The order's outcome: in the page, and on its own as the form's patch.
#[component]
pub fn OrderResult<'a>(text: &'a str) -> impl Patch {
    html! { <p id={ResultId} role="status">{text}</p> }
}

#[derive(Id)]
struct ResultId;

/// What the last menu action did.
#[component]
pub fn MenuResult<'a>(text: &'a str) -> impl Patch {
    html! { <p id={MenuResultId} role="status">{text}</p> }
}

#[derive(Id)]
struct MenuResultId;
