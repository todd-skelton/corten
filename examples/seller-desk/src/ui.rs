// corten ui 0.1.0
//! The UI kit: the theme tokens (`tokens/ui.resolver.json`, light and dark) and the components
//! `corten ui add` copied into `src/ui/`. They are the app's to change; `corten ui list` names
//! the others.

use corten::Component;

// Each copy is `rustfmt::skip`: it stays laid out as the kit lays it out (rustfmt's default width),
// not at this repository's, so `corten ui diff` reports only the edits made to it.

#[rustfmt::skip]
pub mod button;
#[rustfmt::skip]
pub mod dialog;
#[rustfmt::skip]
pub mod input;
#[rustfmt::skip]
pub mod table;

corten::style::tokens!("tokens/ui.resolver.json");

/// The tokens, then each component with a stylesheet: `App::components(ui::COMPONENTS)`.
pub const COMPONENTS: &[Component] = &[TOKENS, button::Button, dialog::Dialog, input::Input, table::Table];
