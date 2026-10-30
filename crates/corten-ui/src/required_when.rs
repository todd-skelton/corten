use corten::{form::FieldName, prelude::*};

use crate::ValueOf;

/// Fields required only for some values of another field: a refund amount for a partial refund, a
/// tracking number when the seller ships, a description for "Other". HTML's `required` is fixed
/// when the page is drawn, so the platform cannot say this.
///
/// A `<fieldset>` around the app's fields (`children`). Its script (`required_when.js`) watches
/// the form's `control` field (named by its `FieldName`: a select, radios, a checkbox, also one
/// joined to the form with `form=`) and, while its value is one of `values` (the words of the
/// field's `UrlValue` enum, or any `AsRef<str>`), makes every field in
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
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// #[serde(rename_all = "snake_case")]
/// enum Kind { Full, Partial }
///
/// #[derive(serde::Deserialize, Fields)]
/// struct Refund {
///     kind: Kind,
///     amount: Option<f64>,
/// }
///
/// let f = Refund::FIELDS;
/// let html = html! {
///     <select name={f.kind}>
///         <option value={Kind::Full}>"Full refund"</option>
///         <option value={Kind::Partial}>"Partial refund"</option>
///     </select>
///     <RequiredWhen control={f.kind} values={&[Kind::Partial]}>
///         <label>"Amount " <input type="number" name={f.amount} step="0.01"></label>
///     </RequiredWhen>
/// }
/// .to_html();
/// assert!(html.contains(r#"<fieldset data-on="connect:ui.required_when.connect($t,&quot;kind&quot;,[&quot;partial&quot;],$s)"><label>"#), "{html}");
/// ```
///
/// The values are the control's own: a word of its enum, text for a text field, or the `value` a
/// checkbox (a `bool` field) sends, so a word of another field's enum does not compile:
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
/// use corten_ui::RequiredWhen;
///
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// enum Kind { Full, Partial }
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// enum Ship { Pickup, Post }
///
/// #[derive(serde::Deserialize, Fields)]
/// struct Refund { kind: Kind, ship: Ship }
///
/// let html = html! { <RequiredWhen control={Refund::FIELDS.kind} values={&[Ship::Post]}>"Amount"</RequiredWhen> };
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
pub fn RequiredWhen<'a, F, V: ValueOf<F> + AsRef<str>, C: Render>(
    control: FieldName<F>,
    values: &'a [V],
    children: C,
) -> impl Render {
    html! {
        <fieldset on:connect={js::connect(target, control.as_str(), values, signal)}>
            {children}
        </fieldset>
    }
}
