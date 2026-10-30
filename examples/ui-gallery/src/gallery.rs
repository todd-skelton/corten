//! The page: a seller's draft listing for one card, built from every corten-ui component, wrapped
//! and styled as a design system would.
use corten::i18n::RelativeStyle;
use corten::prelude::*;
use corten_ui::{
    CheckboxGroup, Choice, ComboBox, Copied, CopyButton, MenuButton, Number, NumberField, Picker, PrintButton, RelativeTime,
    RequiredWhen, Tab, TabGroup, Toast, Toaster, dialog,
};
use serde::Deserialize;
use std::time::SystemTime;

use crate::{
    app::{EXPIRES_IN, LISTED_AGO, Msg, NO_REGION, RenameForm, SellForm, url},
    field::Field,
};

/// The card the draft sells.
pub const CARD: &str = "Ashen Drake";

/// A card's condition, as card shops grade it: the form's `condition` field reads the word each
/// option posts (`UrlValue` writes what serde reads).
#[derive(Clone, Copy, PartialEq, Debug, UrlValue, Deserialize)]
pub enum Condition {
    #[serde(rename = "nm")]
    NearMint,
    #[serde(rename = "lp")]
    LightlyPlayed,
    #[serde(rename = "mp")]
    ModeratelyPlayed,
    #[serde(rename = "hp")]
    HeavilyPlayed,
    #[serde(rename = "dmg")]
    Damaged,
}

impl Condition {
    /// Every condition, best first.
    pub const ALL: [Condition; 5] = [
        Condition::NearMint,
        Condition::LightlyPlayed,
        Condition::ModeratelyPlayed,
        Condition::HeavilyPlayed,
        Condition::Damaged,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Condition::NearMint => "Near Mint",
            Condition::LightlyPlayed => "Lightly Played",
            Condition::ModeratelyPlayed => "Moderately Played",
            Condition::HeavilyPlayed => "Heavily Played",
            Condition::Damaged => "Damaged",
        }
    }

    /// The short grade a seller types for it.
    pub const fn grades(self) -> &'static [&'static str] {
        match self {
            Condition::NearMint => &["NM"],
            Condition::LightlyPlayed => &["LP"],
            Condition::ModeratelyPlayed => &["MP"],
            Condition::HeavilyPlayed => &["HP"],
            Condition::Damaged => &["DMG"],
        }
    }
}

/// The combo box's options: each condition with its label, found by the grade shops write too
/// (typing `mp` puts Moderately Played first).
const CONDITIONS: [Choice<Condition>; 5] = {
    let mut out = [Choice::new(Condition::NearMint, ""); 5];
    let mut i = 0;
    while i < 5 {
        let c = Condition::ALL[i];
        out[i] = Choice::new(c, c.label()).terms(c.grades());
        i += 1;
    }
    out
};

/// How the listing is priced: a fixed price, or offers down to a lowest one.
#[derive(Clone, Copy, PartialEq, Debug, UrlValue, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pricing {
    Fixed,
    Offers,
}

/// Where a seller ships.
#[derive(Clone, Copy, PartialEq, Debug, UrlValue, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Us,
    Ca,
    Eu,
}

/// The regions, each with its label, in the order the form lists them.
const REGIONS: [(Region, &str); 3] = [(Region::Us, "United States"), (Region::Ca, "Canada"), (Region::Eu, "European Union")];

/// The market's tabs, by the `tab` query value that selects each; the first is the default.
#[derive(Clone, Copy, PartialEq, Debug, UrlValue, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketTab {
    Listings,
    Offers,
    Sales,
}

/// The `sale` query value that opens the last sale's sheet.
#[derive(Clone, Copy, PartialEq, Debug, UrlValue, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sale {
    Last,
}

#[component(css)]
pub fn Gallery<'a>(
    tab: MarketTab,
    result: &'a str,
    toast: Option<&'a str>,
    sale: bool,
    times: Times,
    rename: RenameDialog<'a>,
) -> impl Render {
    let Times { t, now } = times;
    let f = SellForm::FIELDS;
    let qty = Number {
        value: Some(1.0),
        min: Some(1.0),
        max: Some(10.0),
        required: true,
        ..Number::new("qty", f.qty, "Fewer", "More")
    };
    // A measure with decimals; the price is money, a text input read by `t.parse_money`.
    let weight = Number { value: Some(0.5), min: Some(0.1), step: 0.1, ..Number::new("weight", f.weight, "Lighter", "Heavier") };
    let condition = Picker {
        required: true,
        ..Picker::new("condition", f.condition, "Choose a condition", "No matching condition", &CONDITIONS)
    };
    let at = |tab| url::Home { tab, sale: None };
    html! {
        <div class={Intro}>
            <h1>"Sell "{CARD}</h1>
            <p>
                "A draft listing, built from corten-ui's headless components: they work as plain HTML first, and a small design system styles them."
            </p>
        </div>
        <div class={Grid}>
            <section class={Card}>
                <h2>"List for sale"</h2>
                <form class={Order} method="post" action={url::Sell} data-patch>
                    <div class={Row}>
                        <Field id="qty" label="Quantity"><NumberField field={&qty} /></Field>
                        <Field id="price" label="Price (USD)">
                            <input id="price" name={f.price} inputmode="decimal" value="12.50" required>
                        </Field>
                        <Field id="weight" label="Shipping weight">
                            <span class={Unit}>"oz"</span>
                            <NumberField field={&weight} />
                        </Field>
                    </div>
                    <Field id="condition" label="Condition"><ComboBox field={&condition} /></Field>
                    <div class={Choices}>
                        <CheckboxGroup legend="Ships to" missing={NO_REGION}>
                            for (region, label) in REGIONS {
                                <label>
                                    <input type="checkbox" name={f.ships} value={region} checked={region == Region::Us}>{label}
                                </label>
                            }
                        </CheckboxGroup>
                    </div>
                    <div class={Choices}>
                        <fieldset>
                            <legend>"Pricing"</legend>
                            <label><input type="radio" name={f.pricing} value={Pricing::Fixed} checked>"Fixed price"</label>
                            <label><input type="radio" name={f.pricing} value={Pricing::Offers}>"Accept offers"</label>
                        </fieldset>
                    </div>
                    <RequiredWhen control={f.pricing} values={&[Pricing::Offers]}>
                        <div class={Floor}>
                            <Field id="floor" label="Lowest offer">
                                <input id="floor" name={f.floor} inputmode="decimal">
                            </Field>
                        </div>
                    </RequiredWhen>
                    <button class={Primary}>"List for sale"</button>
                </form>
                <OrderResult text={result} />
            </section>
            <div class={Side}>
                // Built in the markup, so the panels' views live until the group has rendered them.
                <section class={(Card, Book)}>
                    <h2>"Market"</h2>
                    <TabGroup
                        id="book"
                        label="Market"
                        selected={tab as usize}
                        tabs={&[
                            Tab { label: "Listings", panel: &html! { <p>"Three listings from $11.80."</p> }, link: Some(&at(None)) },
                            Tab { label: "Offers", panel: &html! { <p>"Two offers, the best at $10.00."</p> }, link: Some(&at(Some(MarketTab::Offers))) },
                            Tab { label: "Sales", panel: &html! { <p>"Sold 14 times this month."</p> }, link: Some(&at(Some(MarketTab::Sales))) },
                        ]}
                    />
                </section>
                <section class={Card}>
                    <h2>"Activity"</h2>
                    <p>
                        "Listed "
                        <RelativeTime when={t.relative(now - LISTED_AGO, now, RelativeStyle::Short)} />
                    </p>
                    <p>
                        "The best offer expires "
                        <RelativeTime when={t.relative(now + EXPIRES_IN, now, RelativeStyle::Long)} />
                    </p>
                </section>
                <section class={(Card, Actions)}>
                    <h2>"Draft"</h2>
                    <div class={Buttons}>
                        <form method="post" action={url::Save} data-patch><button>"Save draft"</button></form>
                        <button type="button" command="show-modal" commandfor={RenameId}>"Rename draft"</button>
                        <MenuButton id="actions" label="More actions">
                            <a role="menuitem" href={url::Help}>"Help"</a>
                            <form method="post" action={url::Archive} data-patch><button role="menuitem">"Archive"</button></form>
                            <button type="button" role="menuitem" disabled>"Delete"</button>
                            <button type="button" role="menuitem" command="show-modal" commandfor={RenameId}>"Rename…"</button>
                            <form method="post" action={url::Watch} data-patch><button role="menuitem">"Watch"</button></form>
                        </MenuButton>
                    </div>
                    <MenuResult text="" />
                    <a href={url::Home { tab: None, sale: Some(Sale::Last) }}>"Last sale"</a>
                </section>
                <section class={Card}>
                    <h2>"Stock"</h2>
                    <Sku sku="ASH-DRAKE-NM" />
                    <PrintButton label="Print label" />
                </section>
            </div>
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
/// shared link's, and shows the count from the first.
#[component(js)]
pub fn Sku<'a>(sku: &'a str) -> impl Render {
    let (copied, copies) = refs();
    html! {
        <p on:Copied={js::counted(event, copied, copies)}>
            "SKU "
            <code id={SkuId}>{sku}</code>
            " "
            <CopyButton text={SkuId} label="Copy" copied="Copied" />
            " "
            <span ref={copied} hidden>"Copies: "<output ref={copies} aria-label="Copies">"0"</output></span>
        </p>
    }
}

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
