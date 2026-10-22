use corten::{prelude::*, view::Attrs, FieldName};

/// One option of a [`ComboBox`].
#[derive(Clone, Copy)]
pub struct Choice<'a> {
    /// What the form sends.
    pub value: &'a str,
    /// What the user reads and types to find it.
    pub label: &'a str,
}

/// A combobox's field and options. Start from [`Picker::new`] and set what differs:
///
/// ```ignore
/// let color = Picker { value: Some(&item.color), required: true, ..Picker::new("color", f.color, t.choose_color(), t.no_match(), &colors) };
/// html! { <ComboBox field={&color} /> }
/// ```
pub struct Picker<'a> {
    /// The `id` the app's `<label for>` points at. It names the `<select>` until the script
    /// runs and the text input after, so the label always labels the control that shows.
    pub id: &'a str,
    /// The form field's name, on the `<select>`: `Input::FIELDS.color`.
    pub name: FieldName,
    /// The selected option's value.
    pub value: Option<&'a str>,
    /// Shown when nothing is selected: the select's empty first option and the input's
    /// placeholder.
    pub placeholder: &'a str,
    /// Shown when no option matches what was typed: "No matches".
    pub empty: &'a str,
    pub options: &'a [Choice<'a>],
    pub required: bool,
    pub disabled: bool,
    /// More attributes for both controls, such as `errors.aria(field)`.
    pub attrs: &'a dyn Attrs,
}

static NO_ATTRS: [(&str, bool); 0] = [];

impl<'a> Picker<'a> {
    /// A field with nothing selected, optional and enabled.
    pub fn new(id: &'a str, name: FieldName, placeholder: &'a str, empty: &'a str, options: &'a [Choice<'a>]) -> Self {
        Picker { id, name, value: None, placeholder, empty, options, required: false, disabled: false, attrs: &NO_ATTRS }
    }
}

/// Type to filter a list of options, then pick one.
///
/// The value lives in a native `<select>`: without the script it is the control, so the form
/// works and `required` is checked by the browser. The script (`combo_box.js`) hides it, shows a
/// text input with the ARIA 1.2 combobox pattern instead, builds the listbox from the select's
/// options (they are not sent twice), filters as the user types, and writes the pick back to
/// the select, which sends `change`. Arrow keys move, Enter picks, Escape closes and restores
/// the text, and leaving the field picks an option whose label was typed exactly, clears an
/// emptied field, or restores the text.
///
/// Parts: `select`; `input[role=combobox]` (`[aria-expanded=true]` when open);
/// `[popover]`, the popup, whose anchor is the input (place it with `position-area`);
/// `[role=listbox]` and `[role=option]` (`[aria-selected=true]` for the value,
/// `[data-highlighted]` for the one Enter would pick, `[aria-disabled=true]`); `[role=status]`
/// for the no-match text.
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::{Choice, ComboBox, Picker};
///
/// #[derive(Fields)]
/// struct Order { color: String }
///
/// let colors = [Choice { value: "salt", label: "Salt & Pepper" }, Choice { value: "sand", label: "Desert Sand" }];
/// let field = Picker { value: Some("sand"), ..Picker::new("color", Order::FIELDS.color, "Choose a color", "No matches", &colors) };
/// let html = ComboBox { field: &field }.to_html();
/// assert!(html.contains(r#"<select id="color" name="color""#));
/// assert!(html.contains(r#"<option value="sand" selected>Desert Sand</option>"#));
/// assert!(html.contains(r#"role="combobox""#) && html.contains(r#"value="Desert Sand""#));
/// ```
#[island]
pub fn ComboBox<'a>(field: &'a Picker<'a>) -> impl Render {
    let f = field;
    let shown = f.value.and_then(|v| f.options.iter().find(|o| o.value == v)).map(|o| o.label);
    html! {
        <select id={f.id} name={f.name} required={f.required} disabled={f.disabled} ref={Select} on:invalid={Invalid} {..f.attrs}>
            <option value="">{f.placeholder}</option>
            for o in f.options { <option value={o.value} selected={f.value == Some(o.value)}>{o.label}</option> }
        </select>
        <input type="text" id={(f.id, "-input")} role="combobox" aria-autocomplete="list" aria-expanded="false"
            aria-controls={(f.id, "-list")} autocomplete="off" placeholder={f.placeholder} value={shown}
            disabled={f.disabled} hidden ref={Input}
            on:input={Filter} on:keydown={Key} on:click={Toggle} on:focusout={Leave} {..f.attrs}>
        <div popover="manual" ref={Popup} on:mousedown={Hold} on:click={Pick}>
            <div role="listbox" id={(f.id, "-list")} ref={List}></div>
            <p role="status" ref={Empty} hidden>{f.empty}</p>
        </div>
    }
}
