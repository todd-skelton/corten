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
    pagination::Pagination,
    popover::Popover,
    select::Select,
    separator::{Orientation, Separator},
    skeleton::{Skeleton, SkeletonShape},
    slider::Slider,
    switch::Switch,
    table::Table,
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

/// The orders page: `page` counts from 1, and `photo` is the picture of a buyer who has one.
#[component(css)]
pub fn Orders<'a>(t: Msg, page: usize, photo: &'a str) -> impl Render {
    let f = SettingsForm::FIELDS;
    let shown = ORDERS.iter().skip((page - 1) * PER_PAGE).take(PER_PAGE);
    html! {
        <Breadcrumb
            label="Breadcrumb"
            crumbs={&[Crumb { label: "Draft listing", link: &url::Home { tab: None, sale: None, period: None } }]}
            current="Orders"
        />
        <div class={Heading}>
            <h1>"Orders"</h1>
            <span class={Badge { tone: BadgeTone::Neutral, emphasis: BadgeEmphasis::Solid }}>{ORDERS.len()}" this month"</span>
        </div>
        <Alert variant={AlertVariant::Warning}>
            <strong>"One order is past its ship-by date"</strong>
            <p>"Ada Lovelace's Ashen Drake was due to ship yesterday. Ship it today or message the buyer."</p>
        </Alert>
        <div class={Grid}>
            <section class={Card {}}>
                <header class={CardHeader {}}>
                    <div class={Toolbar}>
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
                                    <th scope="col">"Total"</th>
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
                                        <td>{t.money(Minor(o.cents), USD)}</td>
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
                        link={&|page| Box::new(url::Orders { page: Some(page).filter(|p| *p > 1) })}
                    />
                </footer>
            </section>
            <div class={Aside}>
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
                        <form class={Settings} method="post" action={url::Settings}>
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
                                <input class={Slider {}} id={HandlingId} aria-describedby={FieldHintId(&HandlingId)} type="range" name={f.handling} min="1" max="5" value="2">
                            </Field>
                            <Separator orientation={Orientation::Horizontal} />
                            <Switch label="Email me when an order ships">
                                <input type="checkbox" role="switch" name={f.notify} checked>
                            </Switch>
                            <Field id={NoteId} label="Note to buyers" hint="Printed on each packing slip." error="">
                                <textarea class={Textarea {}} id={NoteId} aria-describedby={FieldHintId(&NoteId)} name={f.note} rows="3">"Thanks for your order!"</textarea>
                            </Field>
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
    }
}
