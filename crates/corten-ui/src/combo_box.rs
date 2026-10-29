use corten::{prelude::*, view::Attrs};

/// One option of a [`ComboBox`]: a value of the type the form's field reads, and its label.
///
/// The value is written as the text its `AsRef<str>` gives, so an enum deriving `UrlValue` and
/// `Deserialize` offers the words serde reads back: the field is `condition: Condition`, the
/// options are `Choice { value: Condition::NearMint, label: "Near Mint" }`, and a form with a word
/// outside the enum is refused (422) as one with a value of the wrong kind is.
#[derive(Clone, Copy)]
pub struct Choice<'a, V> {
    /// What the form sends: the text its `AsRef<str>` gives.
    pub value: V,
    /// What the user reads and types to find it.
    pub label: &'a str,
}

/// A combobox's field and options. Start from [`Picker::new`] and set what differs:
///
/// ```ignore
/// let color = Picker { value: Some(item.color), required: true, ..Picker::new("color", f.color, t.choose_color(), t.no_match(), &COLORS) };
/// html! { <ComboBox field={&color} /> }
/// ```
///
/// The selected value and the options are the same type, so a value of another field's type
/// does not compile:
///
/// ```compile_fail,E0308
/// use corten::prelude::*;
/// use corten_ui::{Choice, Picker};
///
/// #[derive(Clone, Copy, PartialEq, UrlValue)]
/// enum Color { Salt, Sand }
/// #[derive(Clone, Copy, PartialEq, UrlValue)]
/// enum Size { Small }
/// #[derive(Fields)]
/// struct Order { color: String }
///
/// let colors = [Choice { value: Color::Salt, label: "Salt" }, Choice { value: Color::Sand, label: "Sand" }];
/// let field = Picker { value: Some(Size::Small), ..Picker::new("color", Order::FIELDS.color, "Choose", "None", &colors) };
/// ```
pub struct Picker<'a, V> {
    /// The `id` the app's `<label for>` points at. It names the `<select>` until the script
    /// runs and the text input after, so the label always labels the control that shows.
    pub id: &'a str,
    /// The form field's name, on the `<select>`: `Input::FIELDS.color`.
    pub name: FieldName,
    /// The selected option's value.
    pub value: Option<V>,
    /// Shown when nothing is selected: the select's empty first option and the input's
    /// placeholder.
    pub placeholder: &'a str,
    /// Shown when no option matches what was typed: "No matches".
    pub empty: &'a str,
    pub options: &'a [Choice<'a, V>],
    pub required: bool,
    pub disabled: bool,
    /// More attributes for both controls, such as `errors.within(form).aria(field)`.
    pub attrs: &'a dyn Attrs,
}

static NO_ATTRS: [(&str, bool); 0] = [];

impl<'a, V> Picker<'a, V> {
    /// A field with nothing selected, optional and enabled.
    pub fn new(id: &'a str, name: FieldName, placeholder: &'a str, empty: &'a str, options: &'a [Choice<'a, V>]) -> Self {
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
/// Parts: a `div` around `select`; `input[role=combobox]` (`[aria-expanded=true]` when open);
/// `[popover]`, the popup, whose anchor is the input (place it with `position-area`);
/// `[role=listbox]` and `[role=option]` (`[aria-selected=true]` for the value,
/// `[data-highlighted]` for the one Enter would pick, `[aria-disabled=true]`); `[role=status]`
/// for the no-match text.
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::{Choice, ComboBox, Picker};
///
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// #[serde(rename_all = "snake_case")]
/// enum Color { Salt, Sand }
///
/// #[derive(serde::Deserialize, Fields)]
/// struct Order { color: Color }
///
/// const COLORS: [Choice<Color>; 2] = [Choice { value: Color::Salt, label: "Salt & Pepper" }, Choice { value: Color::Sand, label: "Desert Sand" }];
/// let field = Picker { value: Some(Color::Sand), ..Picker::new("color", Order::FIELDS.color, "Choose a color", "No matches", &COLORS) };
/// let html = ComboBox { field: &field }.to_html();
/// assert!(html.contains(r#"<select data-ref="select" id="color" name="color""#));
/// assert!(html.contains(r#"<option value="sand" selected>Desert Sand</option>"#));
/// assert!(html.contains(r#"role="combobox""#) && html.contains(r#"value="Desert Sand""#));
/// ```
#[component(js)]
pub fn ComboBox<'a, V: Copy + PartialEq + AsRef<str>>(field: &'a Picker<'a, V>) -> impl Render {
    let f = field;
    let (select, input, popup, list, empty) = refs();
    let shown = f.value.and_then(|v| f.options.iter().find(|o| o.value == v)).map(|o| o.label);
    html! {
        <div on:connect={js::connect(scope, select, input, popup, list, empty)}>
            <select
                ref={select}
                id={f.id}
                name={f.name}
                required={f.required}
                disabled={f.disabled}
                on:invalid={js::invalid(event, scope)}
                {..f.attrs}
            >
                <option value="">{f.placeholder}</option>
                for o in f.options {
                    <option value={o.value.as_ref()} selected={f.value == Some(o.value)}>{o.label}</option>
                }
            </select>
            <input
                ref={input}
                type="text"
                id={(f.id, "-input")}
                role="combobox"
                aria-autocomplete="list"
                aria-expanded="false"
                aria-controls={(f.id, "-list")}
                autocomplete="off"
                placeholder={f.placeholder}
                value={shown}
                disabled={f.disabled}
                hidden
                on:input={js::filter(scope)}
                on:keydown={js::key(event, scope)}
                on:click={js::toggle(scope)}
                on:focusout={js::leave(scope)}
                {..f.attrs}
            >
            <div ref={popup} popover="manual" on:mousedown={js::hold(event)} on:click={js::pick(event, scope)}>
                <div ref={list} role="listbox" id={(f.id, "-list")}></div>
                <p ref={empty} role="status" hidden>{f.empty}</p>
            </div>
        </div>
    }
}
