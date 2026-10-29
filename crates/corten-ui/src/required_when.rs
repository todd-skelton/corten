use corten::{form::FieldName, prelude::*};

/// Fields required only for some values of another field: a refund amount for a partial refund, a
/// tracking number when the seller ships, a description for "Other". HTML's `required` is fixed
/// when the page is drawn, so the platform cannot say this.
///
/// A `<fieldset>` around the app's fields (`children`). Its script (`required_when.js`) watches
/// the form's `control` field (named by its `FieldName`: a select, radios, a checkbox, also one
/// joined to the form with `form=`) and, while its value is one of `values`, makes every field in
/// the set `required` (checkboxes aside: `required` on a box means it must be checked); for any
/// other value it disables the set, so the browser neither checks nor sends what does not apply.
/// To hide the set while it does not apply, the app's stylesheet uses `:has` on the control
/// (`form:has(option[value="full"]:checked) .amount { display: none }`), which holds before the
/// script runs and without it, so nothing moves when the page loads.
/// Without the script the set is drawn enabled with nothing required, so a browser without
/// scripts can send every choice; the handler checks the rule as well (`errors.add(f.amount, ..)`
/// when the choice needs it and it is missing), and is the authority.
///
/// Every field in the set is required when it applies; one that stays optional goes outside it.
///
/// Parts: the `fieldset` (`:disabled` while the fields do not apply) and the app's fields.
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::RequiredWhen;
///
/// #[derive(serde::Deserialize, Fields)]
/// struct Refund {
///     kind: String,
///     amount: Option<f64>,
/// }
///
/// let f = Refund::FIELDS;
/// let html = html! {
///     <select name={f.kind}>
///         <option value="full">"Full refund"</option>
///         <option value="partial">"Partial refund"</option>
///     </select>
///     <RequiredWhen control={f.kind} values={&["partial"]}>
///         <label>"Amount " <input type="number" name={f.amount} step="0.01"></label>
///     </RequiredWhen>
/// }
/// .to_html();
/// assert!(html.contains(r#"<fieldset data-on="connect:ui.required_when.connect($t,&quot;kind&quot;,[&quot;partial&quot;],$s)"><label>"#), "{html}");
/// ```
///
/// The control is a field of the form struct, so a misspelled name does not compile:
///
/// ```compile_fail,E0308
/// use corten::prelude::*;
/// use corten_ui::RequiredWhen;
///
/// let html = html! { <RequiredWhen control="kind" values={&["partial"]}>"Amount"</RequiredWhen> };
/// ```
#[component(js)]
pub fn RequiredWhen<'a, C: Render>(control: FieldName, values: &'a [&'a str], children: C) -> impl Render {
    html! {
        <fieldset on:connect={js::connect(target, control.as_str(), values, signal)}>
            {children}
        </fieldset>
    }
}
