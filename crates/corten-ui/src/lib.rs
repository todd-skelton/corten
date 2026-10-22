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
//!
//! Use the platform for everything else: `<dialog>` with `command="show-modal"` for dialogs and
//! alert dialogs, `popover` for popovers and tooltips, `<details name>` for accordions,
//! `<select>` (styled with `appearance: base-select` where supported) for selects, native
//! checkboxes, radios and `<input type="range">` for switches, toggle groups and sliders.
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
//! `div` (`[data-scope=number-field]`) that holds its parts; the wrapper lays it out, as the
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
//! let item = MenuItem { label: &"Orders", link: &"/orders", post: false };
//! ```

use corten::{view::AttrValue, Component, Route};

mod combo_box;
mod menu_button;
mod number_field;
mod tab_group;

pub use combo_box::{Choice, ComboBox, Picker};
pub use menu_button::{MenuButton, MenuItem};
pub use number_field::{Number, NumberField, Step};
pub use tab_group::{Tab, TabGroup};

/// Every component in this crate with a script, for `App::components`.
pub const COMPONENTS: &[Component] = &[NumberField, ComboBox, TabGroup, MenuButton];

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
