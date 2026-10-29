use corten::prelude::*;

/// Checkboxes that share a name, of which at least one must be checked: the items a report is
/// about, the regions a seller ships to. HTML's `required` on a checkbox means that one box, so
/// the platform cannot say this about a group.
///
/// A `<fieldset>` with its `legend` and the boxes the app puts in it (`children`). Its script
/// (`checkbox_group.js`) keeps the first enabled box invalid with `missing` (the app's message,
/// in the page's language) while no enabled box is checked, so the browser stops the submit and
/// says so at that box, as it does for a required text field; a form reset checks again. The
/// message is shown at the first box, and `:user-invalid` styles that box alone. Without the script the form is sent with none
/// checked, so the handler checks too (`errors.add(f.ships, ..)` when the list is empty): it is
/// the authority, the script only saves the round trip.
///
/// Parts: the `fieldset`, its `legend`, and the app's boxes.
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::CheckboxGroup;
///
/// #[derive(serde::Deserialize, Fields)]
/// struct Pick {
///     #[serde(default)]
///     ids: Vec<String>,
/// }
///
/// let f = Pick::FIELDS;
/// let html = html! {
///     <CheckboxGroup legend="Affected items" missing="Choose at least one item.">
///         <label><input type="checkbox" name={f.ids} value="a">"A"</label>
///         <label><input type="checkbox" name={f.ids} value="b">"B"</label>
///     </CheckboxGroup>
/// }
/// .to_html();
/// assert!(html.starts_with(r#"<fieldset data-on="connect:ui.checkbox_group.connect($t,&quot;Choose at least one item.&quot;,$s) change:ui.checkbox_group.check($t,&quot;Choose at least one item.&quot;)"><legend>Affected items</legend>"#), "{html}");
/// ```
#[component(js)]
pub fn CheckboxGroup<'a, L: Render, C: Render>(legend: L, missing: &'a str, children: C) -> impl Render {
    html! {
        <fieldset on:connect={js::connect(target, missing, signal)} on:change={js::check(target, missing)}>
            <legend>{legend}</legend>
            {children}
        </fieldset>
    }
}
