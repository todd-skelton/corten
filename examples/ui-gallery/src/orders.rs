//! The seller's orders: the kit's other components on a page an app would have, a list of orders
//! with its pages, the shipping settings and the questions sellers ask.
use corten::i18n::Minor;
use corten::prelude::*;
use serde::Deserialize;

use crate::app::{Msg, USD, url};
use crate::ui::{
    accordion::Accordion,
    alert::{Alert, AlertVariant},
    avatar::Avatar,
    badge::{Badge, BadgeEmphasis, BadgeTone},
    breadcrumb::{Breadcrumb, Crumb},
    button::{Button, ButtonSize, ButtonVariant},
    card::{Card, CardContent, CardFooter, CardHeader},
    field::{Field, FieldHintId},
    meter::Meter,
    pagination::Pagination,
    popover::Popover,
    select::Select,
    separator::{Orientation, Separator},
    skeleton::{Skeleton, SkeletonShape},
    slider::Slider,
    stack::{Cluster, Gap, Stack},
    stats::{Stat, StatTone, Stats},
    status_dot::{StatusDot, StatusTone},
    switch::Switch,
    table::{Sort, SortHeader, Table, TableNumeric},
    textarea::Textarea,
};

/// Where an order is: each a badge of its own look.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Status {
    Paid,
    Shipped,
    Late,
    Refunded,
}

impl Status {
    const fn label(self) -> &'static str {
        match self {
            Status::Paid => "Paid",
            Status::Shipped => "Shipped",
            Status::Late => "Late",
            Status::Refunded => "Refunded",
        }
    }

    const fn look(self) -> Badge {
        let (tone, emphasis) = match self {
            Status::Paid => (BadgeTone::Success, BadgeEmphasis::Soft),
            Status::Shipped => (BadgeTone::Neutral, BadgeEmphasis::Solid),
            Status::Late => (BadgeTone::Destructive, BadgeEmphasis::Solid),
            Status::Refunded => (BadgeTone::Neutral, BadgeEmphasis::Outline),
        };
        Badge { tone, emphasis }
    }
}

/// The order the table is sorted in, by the `sort` query value; newest first when there is none.
#[derive(Clone, Copy, PartialEq, Debug, UrlValue, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderSort {
    TotalUp,
    TotalDown,
}

/// One order: who bought what, where it is and what it came to.
pub struct Order {
    pub buyer: &'static str,
    pub initials: &'static str,
    pub card: &'static str,
    pub status: Status,
    pub cents: u64,
    /// Whether the buyer has a picture: the others show their initials.
    pub pictured: bool,
}

const fn order(buyer: &'static str, initials: &'static str, card: &'static str, status: Status, cents: u64) -> Order {
    Order { buyer, initials, card, status, cents, pictured: false }
}

/// The seller's orders, newest first. A real app reads a page of them from its store.
pub const ORDERS: [Order; 10] = [
    order("Ada Lovelace", "AL", "Ashen Drake", Status::Late, 1250),
    Order { pictured: true, ..order("Grace Hopper", "GH", "Tidecaller", Status::Paid, 840) },
    order("Alan Turing", "AT", "Ashen Drake", Status::Shipped, 1195),
    order("Edsger Dijkstra", "ED", "Glass Warden", Status::Shipped, 2400),
    order("Barbara Liskov", "BL", "Ember Sprite", Status::Refunded, 310),
    order("Donald Knuth", "DK", "Tidecaller", Status::Shipped, 875),
    order("Frances Allen", "FA", "Ashen Drake", Status::Shipped, 1210),
    order("Ken Thompson", "KT", "Stormbinder", Status::Paid, 1960),
    order("Radia Perlman", "RP", "Glass Warden", Status::Shipped, 2350),
    order("John Backus", "JB", "Ember Sprite", Status::Shipped, 295),
];

/// Orders on a page.
pub const PER_PAGE: usize = 4;

/// The pages the orders fill.
pub const PAGES: usize = ORDERS.len().div_ceil(PER_PAGE);

/// Where the seller ships from, by the word the settings form posts.
#[derive(Clone, Copy, PartialEq, Debug, UrlValue, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Warehouse {
    Portland,
    Austin,
    Toronto,
}

impl Warehouse {
    const ALL: [Warehouse; 3] = [Warehouse::Portland, Warehouse::Austin, Warehouse::Toronto];

    pub const fn label(self) -> &'static str {
        match self {
            Warehouse::Portland => "Portland, OR",
            Warehouse::Austin => "Austin, TX",
            Warehouse::Toronto => "Toronto, ON",
        }
    }
}

#[derive(Deserialize, Fields)]
pub struct SettingsForm {
    pub warehouse: Warehouse,
    /// Business days from an order to its shipping: the slider's 1 to 5.
    pub handling: u8,
    /// The switch: absent when off, as a checkbox's.
    #[serde(default)]
    pub notify: bool,
    #[serde(default)]
    pub note: String,
}

/// The share popover's id, which its button opens.
#[derive(Id)]
struct ShareId;

/// The shipping form's controls, each named by its field's label.
#[derive(Id)]
struct WarehouseId;
#[derive(Id)]
struct HandlingId;
#[derive(Id)]
struct NoteId;

/// The selling limit's meter, which its field's label names.
#[derive(Id)]
struct LimitId;

/// The most a seller may sell in a month, in cents, until they have 50 sales.
const LIMIT_CENTS: u64 = 15_000;

/// The orders page: `page` counts from 1, `sort` is the table's order, and `photo` is the
/// picture of a buyer who has one.
#[component(css)]
pub fn Orders<'a>(t: Msg, page: usize, sort: Option<OrderSort>, photo: &'a str) -> impl Render {
    let f = SettingsForm::FIELDS;
    let mut sorted: Vec<&Order> = ORDERS.iter().collect();
    match sort {
        Some(OrderSort::TotalUp) => sorted.sort_by_key(|o| o.cents),
        Some(OrderSort::TotalDown) => sorted.sort_by_key(|o| std::cmp::Reverse(o.cents)),
        None => {}
    }
    // This month's figures: what sold (refunds taken out), what is to ship and what is late.
    let sold: u64 = ORDERS.iter().filter(|o| o.status != Status::Refunded).map(|o| o.cents).sum();
    let to_ship = ORDERS.iter().filter(|o| matches!(o.status, Status::Paid | Status::Late)).count();
    let late = ORDERS.iter().filter(|o| o.status == Status::Late).count();
    let shown = sorted.into_iter().skip((page - 1) * PER_PAGE).take(PER_PAGE);
    // The total's header: sorted up on the first click, then down and up in turn.
    let (by_total, next) = match sort {
        Some(OrderSort::TotalUp) => (Some(Sort::Ascending), OrderSort::TotalDown),
        Some(OrderSort::TotalDown) => (Some(Sort::Descending), OrderSort::TotalUp),
        None => (None, OrderSort::TotalUp),
    };
    html! {
        <div class={Stack { gap: Gap::Lg }}>
            <Breadcrumb
                label="Breadcrumb"
                crumbs={&[Crumb { label: "Draft listing", link: &url::Home { tab: None, sale: None, period: None } }]}
                current="Orders"
            />
            <div class={Cluster { gap: Gap::Sm }}>
                <h1>"Orders"</h1>
                <span class={Badge { tone: BadgeTone::Success, emphasis: BadgeEmphasis::Soft }}>
                    <StatusDot tone={StatusTone::Success} />
                    "Taking orders"
                </span>
            </div>
            <Alert variant={AlertVariant::Warning}>
                <strong>"One order is past its ship-by date"</strong>
                <p>"Ada Lovelace's Ashen Drake was due to ship yesterday. Ship it today or message the buyer."</p>
            </Alert>
            <dl class={Stats {}}>
                <Stat
                    label="Sales"
                    value={t.money(Minor(sold), USD)}
                    note="This month, refunds taken out"
                    tone={StatTone::Neutral}
                />
                <Stat label="Orders" value={ORDERS.len()} note="This month" tone={StatTone::Neutral} />
                <Stat label="To ship" value={to_ship} note="Paid, not shipped yet" tone={StatTone::Neutral} />
                <Stat label="Late" value={late} note="Past the ship-by date" tone={StatTone::Destructive} />
            </dl>
            <div class={Grid}>
                <section class={Card {}}>
                    <header class={CardHeader {}}>
                        <div class={(Cluster { gap: Gap::Md }, Toolbar)}>
                            <h2>"Recent orders"</h2>
                            <Separator orientation={Orientation::Vertical} />
                            <button
                                class={Button { variant: ButtonVariant::Outline, size: ButtonSize::Sm }}
                                type="button"
                                popovertarget={ShareId}
                            >
                                "Share the shop"
                            </button>
                            <Popover>
                                <div id={ShareId} popover>
                                    <p><strong>"Your shop's address"</strong></p>
                                    <p><code>"corten.run/shop/ashen"</code></p>
                                </div>
                            </Popover>
                        </div>
                    </header>
                    <div class={CardContent {}}>
                        <Table>
                            <table>
                                <caption>"Page "{page}" of "{PAGES}</caption>
                                <thead>
                                    <tr>
                                        <th scope="col">"Buyer"</th>
                                        <th scope="col">"Card"</th>
                                        <th scope="col">"Status"</th>
                                        <SortHeader
                                            label="Total"
                                            sort={by_total}
                                            link={&url::Orders { page: None, sort: Some(next) }}
                                            class={TableNumeric {}}
                                        />
                                    </tr>
                                </thead>
                                <tbody>
                                    for o in shown {
                                        <tr>
                                            <td>
                                                <span class={Buyer}>
                                                    <Avatar src={o.pictured.then_some(photo)} alt="" initials={o.initials} />
                                                    {o.buyer}
                                                </span>
                                            </td>
                                            <td>{o.card}</td>
                                            <td><span class={o.status.look()}>{o.status.label()}</span></td>
                                            <td class={TableNumeric {}}>{t.money(Minor(o.cents), USD)}</td>
                                        </tr>
                                    }
                                </tbody>
                            </table>
                        </Table>
                    </div>
                    <footer class={CardFooter {}}>
                        <Pagination
                            label="Pages"
                            previous_label="Previous"
                            next_label="Next"
                            pages={PAGES}
                            current={page}
                            link={&|page| Box::new(url::Orders { page: Some(page).filter(|p| *p > 1), sort })}
                        />
                    </footer>
                </section>
                <div class={Stack { gap: Gap::Md }}>
                    <section class={Card {}}>
                        <header class={CardHeader {}}><h2>"Selling limit"</h2></header>
                        <div class={CardContent {}}>
                            <Field
                                id={LimitId}
                                label="Sold this month"
                                hint={html! { {t.money(Minor(sold), USD)}" of "{t.money(Minor(LIMIT_CENTS), USD)}". It resets on the 1st." }}
                                error=""
                            >
                                // Lower is better: under two thirds of the limit is good, near it is a warning.
                                <meter
                                    class={Meter {}}
                                    id={LimitId}
                                    aria-describedby={FieldHintId(&LimitId)}
                                    aria-valuetext={t.money(Minor(sold), USD)}
                                    min="0"
                                    max={LIMIT_CENTS}
                                    low="10000"
                                    high="13500"
                                    optimum="0"
                                    value={sold}
                                >
                                    {t.money(Minor(sold), USD)}
                                </meter>
                            </Field>
                        </div>
                    </section>
                    <section class={Card {}}>
                        <header class={CardHeader {}}><h2>"Next payout"</h2></header>
                        <div class={CardContent {}}>
                            // The shape the payout's live region shows before its first value.
                            <div class={Payout} aria-busy="true">
                                <span class={Hidden}>"Working out your next payout."</span>
                                <Skeleton shape={SkeletonShape::Circle} />
                                <div class={Lines}>
                                    <Skeleton shape={SkeletonShape::Line} />
                                    <Skeleton shape={SkeletonShape::Line} />
                                </div>
                            </div>
                        </div>
                    </section>
                    <section class={Card {}}>
                        <header class={CardHeader {}}><h2>"Shipping"</h2></header>
                        <div class={CardContent {}}>
                            <form class={Stack { gap: Gap::Lg }} method="post" action={url::Settings}>
                                <Field id={WarehouseId} label="Ship from" hint="" error="">
                                    <select class={Select {}} id={WarehouseId} name={f.warehouse}>
                                        for warehouse in Warehouse::ALL {
                                            <option value={warehouse} selected={warehouse == Warehouse::Portland}>
                                                {warehouse.label()}
                                            </option>
                                        }
                                    </select>
                                </Field>
                                <Field id={HandlingId} label="Handling time (business days)" hint="From 1 to 5." error="">
                                    <input
                                        class={Slider {}}
                                        id={HandlingId}
                                        aria-describedby={FieldHintId(&HandlingId)}
                                        type="range"
                                        name={f.handling}
                                        min="1"
                                        max="5"
                                        value="2"
                                    >
                                </Field>
                                <Separator orientation={Orientation::Horizontal} />
                                <Switch label="Email me when an order ships">
                                    <input type="checkbox" role="switch" name={f.notify} checked>
                                </Switch>
                                <Field id={NoteId} label="Note to buyers" hint="Printed on each packing slip." error="">
                                    <textarea
                                        class={Textarea {}}
                                        id={NoteId}
                                        aria-describedby={FieldHintId(&NoteId)}
                                        name={f.note}
                                        rows="3"
                                    >
                                        "Thanks for your order!"
                                    </textarea>
                                </Field>
                                <p>"Changes apply to orders placed after you save."</p>
                                <div>
                                    <button class={Button { variant: ButtonVariant::Primary, size: ButtonSize::Md }}>
                                        "Save settings"
                                    </button>
                                </div>
                            </form>
                        </div>
                    </section>
                    <section class={Card {}}>
                        <header class={CardHeader {}}><h2>"Questions"</h2></header>
                        <div class={CardContent {}}>
                            <Accordion>
                                <details name="faq" open>
                                    <summary>"When do payouts go out?"</summary>
                                    <p>"Every Friday, for the orders shipped by Wednesday."</p>
                                </details>
                                <details name="faq">
                                    <summary>"What if an order ships late?"</summary>
                                    <p>"The buyer may cancel it after three days, and the sale is refunded."</p>
                                </details>
                                <details name="faq">
                                    <summary>"Can I change the note later?"</summary>
                                    <p>"Yes: each packing slip prints the note as it is when you ship."</p>
                                </details>
                            </Accordion>
                        </div>
                    </section>
                </div>
            </div>
        </div>
    }
}
