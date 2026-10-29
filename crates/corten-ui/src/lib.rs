//! Headless components for the widgets the browser has no element for. Each renders accessible
//! markup on the server, works as plain HTML before its script loads, and ships no CSS: the
//! design system styles it.
//!
//! | Component | Pattern | Without JavaScript |
//! |---|---|---|
//! | [`NumberField`] | a number input with decrease and increase buttons, press and hold | a native number input |
//! | [`ComboBox`] | type to filter a list, pick one; the value is a native `<select>` | the `<select>` |
//! | [`TabGroup`] | tabs with arrow keys; tabs can be links, and the address follows | links to each tab's page |
//! | [`MenuButton`] | a button that opens a menu of links and form actions, arrow keys and typeahead | the menu opens (popover); items work |
//! | [`Toaster`] | a live region an action's patch puts a message in; gone after a while, kept while read | the message is in the page the post answers with |
//! | [`CopyButton`] | copies an element's text to the clipboard, says so politely, and tells the page ([`Copied`]) | no button; the text is on the page to select |
//! | [`RelativeTime`] | a time relative to now ("in 10 hours") that stays current, on one timer for the page | the time as the server wrote it |
//! | [`CheckboxGroup`] | checkboxes of one name of which one must be checked; the browser says so before sending | the form is sent, and the handler refuses it |
//! | [`RequiredWhen`] | fields required only for some values of another field, disabled for the rest | the fields are optional, and the handler checks the rule |
//! | [`dialog::modal`] | a `<dialog>` the server draws `open` (a refused form) is modal: focus in it, the page behind inert | the dialog is open, not modal |
//! | [`dialog::modalClosedBy`] | the same for a dialog the address opens (`?selected=it_1`), and a close request (Escape) goes to its close link's address | the dialog is open, and the link closes it |
//!
//! Each is tested in Chromium, Firefox and WebKit (`examples/ui-gallery/smoke.mjs`).
//!
//! Use the platform for everything else: `<dialog>` with `command="show-modal"` for dialogs and
//! alert dialogs (with [`dialog::modal`] on one the server can draw open), `popover` for popovers and toggletips (`popovertarget`: shown on a click or
//! tap), `<details name>` for accordions,
//! `<select>` (styled with `appearance: base-select` where supported) for selects, native
//! checkboxes, radios and `<input type="range">` for switches, toggle groups and sliders. A
//! tooltip shown on hover or focus is `interestfor` with `popover="hint"` once every browser has
//! them (Safari has neither in October 2026); this crate does not add one in the meantime.
//!
//! List the components once, with the app's own:
//!
//! ```ignore
//! App::new(routes).components(&[theme::TOKENS, views::Field]).components(corten_ui::COMPONENTS)
//! ```
//!
//! **Styling.** A design system wraps each component in its own element and styles the parts by
//! role and state, which are the accessibility contract and so cannot drift from what assistive
//! technology announces: `.field [role=option][aria-selected=true]`, `.field [role=tab][aria-selected=true]`,
//! `.field button:disabled`. Each component's docs list its parts. Each component's root is a
//! `div` (`[data-scope=ui-number-field]`) that holds its parts; the wrapper lays it out, as the
//! flex container itself or with `display: contents` so the parts join the wrapper's layout
//! (`order` places an adornment between them). Popups (`[popover]`) are in the top layer, and the script makes the control
//! that opens each one its anchor; the design system places it with CSS anchor positioning,
//! which the browser keeps in place on scroll and resize without script:
//! `inset: auto; position-area: block-end span-inline-end; position-try-fallbacks: flip-block;
//! min-width: anchor-size(width)`. Two rules the design system keeps:
//! the app's base CSS has `[hidden] { display: none !important }` (parts are hidden with the
//! `hidden` attribute, which a role's `display` would otherwise override), and a popup's
//! `display` is set under `:popover-open` only, or it shows while closed.
//!
//! **Text.** Every label comes from the app, in the page's language; there is no English default
//! except the `−` and `+` of [`NumberField`]'s buttons, which a design system replaces with
//! icons.
//!
//! **Links** are typed routes ([`Link`]): a menu item or tab cannot point at a URL the app does
//! not route.
//!
//! ```compile_fail
//! use corten::prelude::*;
//! use corten_ui::MenuItem;
//!
//! let item = MenuItem { label: "Orders", link: &"/orders", post: false };
//! ```
//!
//! **Dialogs the server draws open.** A form in a `<dialog>` that the server refuses comes back
//! with the dialog drawn `open` when the answer is a whole page (a post without the runtime). The
//! platform has no markup that opens a dialog as a modal, so `open` alone does not hold focus,
//! make the page inert or show the backdrop. [`dialog::modal`] does, when the dialog joins the
//! page (a load, a navigation or a patch); a dialog drawn closed is left closed:
//!
//! ```
//! use corten::prelude::*;
//! use corten_ui::dialog;
//!
//! #[derive(Id)]
//! struct RenameId; // ui-rename: ids in this crate start with its prefix
//!
//! let refused = true;
//! let html = html! {
//!     <button type="button" command="show-modal" commandfor={RenameId}>"Rename"</button>
//!     <dialog id={RenameId} open={refused} on:connect={dialog::modal(target)}>".."</dialog>
//! }
//! .to_html();
//! assert!(html.contains(r#"<dialog id="ui-rename" open data-on="connect:ui.dialog.modal($t)">"#), "{html}");
//! ```
//!
//! It takes the dialog itself, so on any other element it does not compile:
//!
//! ```compile_fail
//! use corten::prelude::*;
//! use corten_ui::dialog;
//!
//! let html = html! { <div open on:connect={dialog::modal(target)}></div> };
//! ```
//!
//! **Dialogs the address opens.** A record shown beside its list (`/items?selected=it_1`) is a
//! dialog the server draws `open` when the address names one, closed by a link to the address
//! without it. Escape on a modal dialog closes it where the address cannot see, so a reload or a
//! redraw would open it again. [`dialog::modalClosedBy`] makes it modal as [`dialog::modal`]
//! does, and a close request (Escape, Android's Back, a click outside a `closedby="any"` dialog)
//! goes to its close link's address without a new history entry (it does not click the link):
//! back, when the entry before is that address, else in place of the dialog's entry (always so in
//! a browser without the Navigation API, where Back after a dialog opened from the list shows the
//! list once more). Attributes on the link (`data-reload`, `data-keep-place`, an `on:click`) do
//! not apply. Android's Back on an open dialog cannot loop between the two:
//!
//! ```
//! use corten::prelude::*;
//! use corten_ui::dialog;
//!
//! #[derive(Route)]
//! #[route("/items")]
//! struct Items<'a> { selected: Option<&'a str> }
//!
//! #[component]
//! fn ItemSheet<'a>(name: &'a str) -> impl Render {
//!     let close = refs();
//!     html! {
//!         <dialog open aria-label={name} on:connect={dialog::modalClosedBy(target, close, signal)}>
//!             <h2>{name}</h2>
//!             <a ref={close} href={Items { selected: None }}>"Close"</a>
//!         </dialog>
//!     }
//! }
//!
//! let html = ItemSheet { name: "Lamp" }.to_html();
//! assert!(html.contains(r#"data-on="connect:ui.dialog.modalClosedBy($t,@close,$s)""#), "{html}");
//! ```
//!
//! The close is a link, put there by a ref, so a button (which changes no address) does not
//! compile:
//!
//! ```compile_fail
//! use corten::prelude::*;
//! use corten_ui::dialog;
//!
//! #[component]
//! fn ItemSheet() -> impl Render {
//!     let close = refs();
//!     html! {
//!         <dialog open on:connect={dialog::modalClosedBy(target, close, signal)}>
//!             <button ref={close} type="button">"Close"</button>
//!         </dialog>
//!     }
//! }
//! ```

use corten::{script::script, view::AttrValue, Component, Route};

mod checkbox_group;
mod combo_box;
mod copy_button;
mod menu_button;
mod number_field;
mod relative_time;
mod required_when;
mod tab_group;
mod toaster;

pub use checkbox_group::CheckboxGroup;
pub use combo_box::{Choice, ComboBox, Picker};
pub use copy_button::{Copied, CopyButton};
pub use menu_button::{MenuButton, MenuItem};
pub use number_field::{Number, NumberField, Step};
pub use relative_time::RelativeTime;
pub use required_when::RequiredWhen;
pub use tab_group::{Tab, TabGroup};
pub use toaster::{Toast, ToastId, Toaster};

script!(pub mod dialog = "dialog.js");

/// Every component in this crate with a script, and the [`dialog`] handlers, for
/// `App::components`.
pub const COMPONENTS: &[Component] = &[
    NumberField,
    ComboBox,
    TabGroup,
    MenuButton,
    Toaster,
    CopyButton,
    RelativeTime,
    CheckboxGroup,
    RequiredWhen,
    dialog::COMPONENT,
];

/// A typed route a component links to (`#[derive(Route)]`), as a trait object so a list of
/// items can hold different routes. A string is not a `Link`.
pub trait Link {
    /// Writes the URL, escaped for an attribute.
    fn write_link(&self, out: &mut String);
}

impl<R: Route> Link for R {
    fn write_link(&self, out: &mut String) {
        self.write_href(out, true)
    }
}

/// A [`Link`] as an attribute value: `href={Url(item.link)}`.
pub(crate) struct Url<'a>(pub &'a dyn Link);

impl AttrValue for Url<'_> {
    fn write_attr(&self, name: &str, out: &mut String) {
        out.push_str(name);
        out.push_str("=\"");
        self.0.write_link(out);
        out.push('"');
    }
}

/// `"true"` or `"false"`, for ARIA states, which are text.
pub(crate) fn state(on: bool) -> &'static str {
    if on {
        "true"
    } else {
        "false"
    }
}

#[cfg(test)]
mod tests;
