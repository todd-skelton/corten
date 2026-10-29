use corten::prelude::*;

/// A button that opens the browser's print dialog for the page: a packing slip, a receipt, an
/// invoice, a ticket, a shipping label. What prints, and at what paper size, is the page's own
/// print styles (`@media print` and `@page` in its components' stylesheets; the
/// [styles guide](https://github.com/todd-skelton/corten/blob/main/docs/guide/styles.md#printing)).
///
/// The button is drawn `hidden` and its script (`print_button.js`) shows it: without a script
/// it could do nothing, and the browser's own Print command still prints the page.
///
/// Parts: the `button`.
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::PrintButton;
///
/// let html = html! { <PrintButton label="Print slips" /> }.to_html();
/// assert_eq!(
///     html,
///     r#"<button type="button" hidden data-on="connect:ui.print_button.connect($t) click:ui.print_button.print()">Print slips</button>"#
/// );
/// ```
#[component(js)]
pub fn PrintButton<L: Render>(label: L) -> impl Render {
    html! {
        <button type="button" hidden on:connect={js::connect(target)} on:click={js::print()}>{label}</button>
    }
}
