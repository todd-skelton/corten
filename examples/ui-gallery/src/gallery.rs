//! The page: every corten-ui component, wrapped and styled as a design system would.
use corten::i18n::RelativeStyle;
use corten::prelude::*;
use corten_ui::{
    Choice, ComboBox, CopyButton, MenuButton, MenuItem, Number, NumberField, Picker, RelativeTime, Tab, TabGroup, Toast, Toaster,
    dialog,
};
use std::time::SystemTime;

use crate::{
    app::{EXPIRES_IN, LISTED_AGO, Msg, OrderForm, RenameForm, url},
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
pub fn Gallery<'a>(
    tab: usize,
    result: &'a str,
    toast: Option<&'a str>,
    sale: bool,
    times: Times,
    rename: RenameDialog<'a>,
) -> impl Render {
    let Times { t, now } = times;
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
        MenuItem { label: "Help", link: &url::Help, post: false },
        MenuItem { label: "Archive", link: &url::Archive, post: true },
        MenuItem { label: "Watch", link: &url::Watch, post: true },
    ];
    html! {
        <div class={Intro}>
            <h1>"corten-ui"</h1>
            <p>"Headless components that work as plain HTML first, styled here by a small design system."</p>
        </div>
        <div class={Grid}>
            <section class={(Card, Wide)}>
                <h2>"Number field and combobox"</h2>
                <form class={Order} method="post" action={url::Order} data-patch>
                    <Field id="qty" label="Quantity"><NumberField field={&qty} /></Field>
                    <Field id="price" label="Price">
                        <span class={Currency}>"$"</span>
                        <NumberField field={&price} />
                    </Field>
                    <Field id="color" label="Color"><ComboBox field={&color} /></Field>
                    <button class={Primary}>"Order"</button>
                </form>
                <OrderResult text={result} />
            </section>
            // Built in the markup, so the panels' views live until the group has rendered them.
            <section class={(Card, Book)}>
                <h2>"Tabs"</h2>
                <TabGroup
                    id="book"
                    label="Market"
                    selected={tab}
                    tabs={&[
                        Tab { label: "Listings", panel: &html! { <p>"Three listings from $11.80."</p> }, link: Some(&url::Home { tab: None, sale: None }) },
                        Tab { label: "Offers", panel: &html! { <p>"Two offers, the best at $10.00."</p> }, link: Some(&url::Home { tab: Some(TABS[1]), sale: None }) },
                        Tab { label: "Sales", panel: &html! { <p>"Sold 14 times this month."</p> }, link: Some(&url::Home { tab: Some(TABS[2]), sale: None }) },
                    ]}
                />
            </section>
            <section class={(Card, Actions)}>
                <h2>"Menu button"</h2>
                <MenuButton id="actions" label="Actions" items={&items} />
                <MenuResult text="" />
            </section>
            <section class={Card}>
                <h2>"Relative time"</h2>
                <p>
                    {t.offer_listed()}" "
                    <RelativeTime when={t.relative(now - LISTED_AGO, now, RelativeStyle::Short)} />
                </p>
                <p>
                    {t.offer_expires()}" "
                    <RelativeTime when={t.relative(now + EXPIRES_IN, now, RelativeStyle::Long)} />
                </p>
            </section>
            <section class={Card}>
                <h2>"Toast and dialog"</h2>
                <div class={Buttons}>
                    <form method="post" action={url::Save} data-patch><button>"Save draft"</button></form>
                    <button type="button" command="show-modal" commandfor={RenameId}>"Rename draft"</button>
                    <a href={url::Home { tab: None, sale: Some(LAST_SALE) }}>"Last sale"</a>
                </div>
            </section>
            <section class={Card}>
                <h2>"Copy"</h2>
                <OrderNumber number="ORD-1234" />
            </section>
        </div>
        {rename}
        if sale {
            <SaleSheet />
        }
        <div class={Toasts}>
            <Toaster close="Dismiss" seconds={2} toast={Toast { message: toast }} />
        </div>
    }
}

/// The offer's relative times: written as of `now`, in `t`'s locale.
#[derive(Clone, Copy)]
pub struct Times {
    pub t: Msg,
    pub now: SystemTime,
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

/// The order number's element, which its copy button names.
#[derive(Id)]
struct OrderNumberId;

/// An order number with a button that copies it: the number is on the page once, and the button
/// shows only where the script can write the clipboard.
#[component]
fn OrderNumber<'a>(number: &'a str) -> impl Render {
    html! {
        <p>
            "Order "
            <code id={OrderNumberId}>{number}</code>
            " "
            <CopyButton text={OrderNumberId} label="Copy" copied="Copied" />
        </p>
    }
}

/// The `sale` query value that opens the last sale's sheet.
pub const LAST_SALE: &str = "last";

/// The last sale, beside the gallery: the address opens it (`/?sale=last`), and the link back to
/// the gallery closes it, clicked or by Escape (`dialog::modalClosedBy`), so a reload shows what
/// the user last saw.
#[component]
pub fn SaleSheet() -> impl Render {
    let close = refs();
    html! {
        <dialog open aria-label="Last sale" on:connect={dialog::modalClosedBy(target, close, signal)}>
            <h2>"Last sale"</h2>
            <p>"Sold for $12.00 on May 3."</p>
            <a ref={close} href={url::Home { tab: None, sale: None }}>"Close"</a>
        </dialog>
    }
}

/// The rename dialog's id: the refused form's patch replaces the dialog by it.
#[derive(Id)]
pub struct RenameId;

/// Naming the draft, in a modal dialog. Refused, the server draws it open with the error, as the
/// form's patch or in the page a post without the runtime answers with, and `dialog::modal`
/// makes it modal again when it joins the page.
#[component]
pub fn RenameDialog<'a>(name: &'a str, errors: &'a Errors) -> impl Patch {
    let f = RenameForm::FIELDS;
    let e = errors.within(RenameId);
    html! {
        <dialog id={RenameId} open={!errors.is_empty()} aria-label="Rename the draft" on:connect={dialog::modal(target)}>
            <form method="post" action={url::Rename} data-patch>
                <label for="draft-name">"Name"</label>
                <input id="draft-name" name={f.name} value={name} {..e.aria(f.name)}>
                {e.message(f.name)}
                <button>"Rename"</button>
            </form>
            <form method="dialog"><button>"Cancel"</button></form>
        </dialog>
    }
}
