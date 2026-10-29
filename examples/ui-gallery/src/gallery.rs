//! The page: a seller's draft listing for one card, built from every corten-ui component, wrapped
//! and styled as a design system would.
use corten::prelude::*;
use corten_ui::{
    CheckboxGroup, Choice, ComboBox, Copied, CopyButton, MenuButton, MenuItem, Number, NumberField, Picker, RequiredWhen, Tab,
    TabGroup, Toast, Toaster, dialog,
};

use crate::{
    app::{NO_REGION, OFFERS, RenameForm, SellForm, url},
    field::Field,
};

/// The card the draft sells.
pub const CARD: &str = "Ashen Drake";

/// A card's condition, as card shops grade it.
pub const CONDITIONS: [Choice; 5] = [
    Choice { value: "nm", label: "Near Mint" },
    Choice { value: "lp", label: "Lightly Played" },
    Choice { value: "mp", label: "Moderately Played" },
    Choice { value: "hp", label: "Heavily Played" },
    Choice { value: "dmg", label: "Damaged" },
];

/// The tabs, in order, by the `tab` query value that selects each.
pub const TABS: [&str; 3] = ["listings", "offers", "sales"];

#[component(css)]
pub fn Gallery<'a>(tab: usize, result: &'a str, toast: Option<&'a str>, sale: bool, rename: RenameDialog<'a>) -> impl Render {
    let f = SellForm::FIELDS;
    let qty = Number {
        value: Some(1.0),
        min: Some(1.0),
        max: Some(10.0),
        required: true,
        ..Number::new("qty", f.qty, "Fewer", "More")
    };
    let price = Number { value: Some(12.5), min: Some(0.0), step: 0.01, ..Number::new("price", f.price, "Lower", "Higher") };
    let condition = Picker {
        required: true,
        ..Picker::new("condition", f.condition, "Choose a condition", "No matching condition", &CONDITIONS)
    };
    let items = [
        MenuItem { label: "Help", link: &url::Help, post: false },
        MenuItem { label: "Archive", link: &url::Archive, post: true },
        MenuItem { label: "Watch", link: &url::Watch, post: true },
    ];
    html! {
        <div class={Intro}>
            <h1>"Sell "{CARD}</h1>
            <p>
                "A draft listing, built from corten-ui's headless components: they work as plain HTML first, and a small design system styles them."
            </p>
        </div>
        <div class={Grid}>
            <section class={(Card, Wide)}>
                <h2>"List for sale"</h2>
                <form class={Order} method="post" action={url::Sell} data-patch>
                    <Field id="qty" label="Quantity"><NumberField field={&qty} /></Field>
                    <Field id="price" label="Price">
                        <span class={Currency}>"$"</span>
                        <NumberField field={&price} />
                    </Field>
                    <Field id="condition" label="Condition"><ComboBox field={&condition} /></Field>
                    <CheckboxGroup legend="Ships to" missing={NO_REGION}>
                        for (value, label) in REGIONS {
                            <label><input type="checkbox" name={f.ships} value={value} checked={value == "us"}>{label}</label>
                        }
                    </CheckboxGroup>
                    <Field id="pricing" label="Pricing">
                        <select id="pricing" name={f.pricing}>
                            <option value="fixed">"Fixed price"</option>
                            <option value={OFFERS}>"Accept offers"</option>
                        </select>
                    </Field>
                    <RequiredWhen control={f.pricing} values={&[OFFERS]}>
                        <div class={Floor}>
                            <Field id="floor" label="Lowest offer">
                                <input id="floor" type="number" name={f.floor} min="0" step="0.01">
                            </Field>
                        </div>
                    </RequiredWhen>
                    <button class={Primary}>"List for sale"</button>
                </form>
                <OrderResult text={result} />
            </section>
            // Built in the markup, so the panels' views live until the group has rendered them.
            <section class={(Card, Book)}>
                <h2>"Market"</h2>
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
                <h2>"Manage"</h2>
                <MenuButton id="actions" label="Actions" items={&items} />
                <MenuResult text="" />
            </section>
            <section class={Card}>
                <h2>"Draft"</h2>
                <div class={Buttons}>
                    <form method="post" action={url::Save} data-patch><button>"Save draft"</button></form>
                    <button type="button" command="show-modal" commandfor={RenameId}>"Rename draft"</button>
                    <a href={url::Home { tab: None, sale: Some(LAST_SALE) }}>"Last sale"</a>
                </div>
            </section>
            <section class={Card}>
                <h2>"Stock"</h2>
                <Sku sku="ASH-DRAKE-NM" />
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

/// The listing's outcome: in the page, and on its own as the form's patch.
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

/// The SKU's element, which its copy button names.
#[derive(Id)]
struct SkuId;

/// The draft's SKU, which a seller copies into their own stock sheet, with a button that copies
/// it: the SKU is on the page once, and the button shows only where the script can write the
/// clipboard. The paragraph counts the copies the button reports (`Copied`), as an app counts a
/// shared link's.
#[component(js)]
pub fn Sku<'a>(sku: &'a str) -> impl Render {
    let copies = refs();
    html! {
        <p on:Copied={js::counted(event, copies)}>
            "SKU "
            <code id={SkuId}>{sku}</code>
            " "
            <CopyButton text={SkuId} label="Copy" copied="Copied" />
            " "
            <output ref={copies} aria-label="Copies">"0"</output>
        </p>
    }
}

/// The regions a seller can ship to: value and label.
const REGIONS: [(&str, &str); 3] = [("us", "United States"), ("ca", "Canada"), ("eu", "European Union")];

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
            <p>"A Near Mint copy sold for $12.00 on May 3."</p>
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
