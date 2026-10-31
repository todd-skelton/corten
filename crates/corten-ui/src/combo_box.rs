use corten::{
    prelude::*,
    view::{escape_into, AttrValue, Attrs},
};

/// One option of a [`ComboBox`]: a value of the type the form's field reads, its label, and
/// other words that find it.
///
/// The value is written as its `Display`, as a route writes a field, and with the same contract:
/// what `Display` writes is what the field's `Deserialize` reads back. Numbers, text, a `Uuid` and
/// an enum deriving `UrlValue` keep it (`UrlValue` writes the word serde reads), so an `i32` row
/// id posts `42` to `line: i32` and `Choice::new(Condition::NearMint, "Near Mint")` posts `nm` to
/// `condition: Condition`. A type of the app's own whose `Display` writes something else (`$1`
/// for a field that reads `1`) has every pick refused (422), as a form with a value outside the
/// field's type is.
///
/// `terms` are matched as the label is, without being shown: a card number, a SKU, an
/// abbreviation. Typing a term whole puts its option first. [`Choice::terms`] sets them from a
/// slice of anything that is text: constant `&str`s, or the `String`s a row computed, borrowed
/// where they are with no second list:
///
/// ```
/// use corten_ui::Choice;
///
/// let charizard = Choice::new(4, "Charizard - Base Set").terms(&["4", "004/102"]);
///
/// struct Card { id: i32, label: String, terms: Vec<String> }
/// let cards = [Card { id: 4, label: "Charizard - Base Set".into(), terms: vec!["4/102".into(), "004/102".into()] }];
/// let options: Vec<_> = cards.iter().map(|c| Choice::new(c.id, &c.label).terms(&c.terms)).collect();
/// # let _ = (charizard, options);
/// ```
#[derive(Clone, Copy)]
pub struct Choice<'a, V, S = &'a str> {
    /// What the form sends: its `Display`.
    pub value: V,
    /// What the user reads and types to find it.
    pub label: &'a str,
    /// Other words that find it, each matched as the label is. A term is one line: it is written
    /// in one attribute, a line each, so a line break in a term splits it in two.
    pub terms: &'a [S],
}

impl<'a, V> Choice<'a, V> {
    /// An option found by its label alone.
    pub const fn new(value: V, label: &'a str) -> Self {
        Choice { value, label, terms: &[] }
    }
}

impl<'a, V: Copy, S> Choice<'a, V, S> {
    /// The same option, also found by `terms`: `&str`s, `String`s, or any `AsRef<str>` a
    /// [`ComboBox`] writes.
    pub const fn terms<T>(self, terms: &'a [T]) -> Choice<'a, V, T> {
        Choice { value: self.value, label: self.label, terms }
    }
}

/// A choice's terms as one `data-terms` attribute, a line each, or nothing when there are none.
struct Terms<'a, S>(&'a [S]);

impl<S: AsRef<str>> AttrValue for Terms<'_, S> {
    fn write_attr(&self, name: &str, out: &mut String) {
        let Some((first, rest)) = self.0.split_first() else { return };
        out.push_str(name);
        out.push_str("=\"");
        escape_into(first.as_ref(), out);
        for term in rest {
            out.push('\n');
            escape_into(term.as_ref(), out);
        }
        out.push('"');
    }
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
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// enum Color { Salt, Sand }
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// enum Size { Small }
/// #[derive(serde::Deserialize, Fields)]
/// struct Order { color: Color }
///
/// let colors = [Choice::new(Color::Salt, "Salt"), Choice::new(Color::Sand, "Sand")];
/// let field = Picker { value: Some(Size::Small), ..Picker::new("color", Order::FIELDS.color, "Choose", "None", &colors) };
/// ```
///
/// And the field it names reads that type, so a picker of colors on another field does not
/// compile either:
///
/// ```compile_fail,E0277
/// use corten::prelude::*;
/// use corten_ui::{Choice, Picker};
///
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// enum Color { Salt, Sand }
/// #[derive(Clone, Copy, PartialEq, UrlValue, serde::Deserialize)]
/// enum Size { Small }
/// #[derive(serde::Deserialize, Fields)]
/// struct Order { color: Color, size: Size }
///
/// let colors = [Choice::new(Color::Salt, "Salt"), Choice::new(Color::Sand, "Sand")];
/// let field = Picker::new("size", Order::FIELDS.size, "Choose", "None", &colors);
/// ```
pub struct Picker<'a, V, S = &'a str> {
    /// The `id` the app's `<label for>` points at. It names the `<select>` until the script
    /// runs and the text input after, so the label always labels the control that shows.
    pub id: &'a str,
    /// The form field's name, on the `<select>`: `Input::FIELDS.color`. Set by [`Picker::new`],
    /// which checks that the field reads the options' values.
    pub name: FieldName,
    /// The selected option's value.
    pub value: Option<V>,
    /// Shown when nothing is selected: the select's empty first option and the input's
    /// placeholder.
    pub placeholder: &'a str,
    /// Shown when no option matches what was typed: "No matches".
    pub empty: &'a str,
    pub options: &'a [Choice<'a, V, S>],
    pub required: bool,
    pub disabled: bool,
    /// More attributes for both controls, such as `errors.within(form).aria(field)`.
    pub attrs: &'a dyn Attrs,
}

static NO_ATTRS: [(&str, bool); 0] = [];

impl<'a, V, S> Picker<'a, V, S> {
    /// A field with nothing selected, optional and enabled.
    pub fn new<F>(id: &'a str, name: FieldName<F>, placeholder: &'a str, empty: &'a str, options: &'a [Choice<'a, V, S>]) -> Self
    where
        V: crate::ValueOf<F>,
    {
        Picker {
            id,
            name: name.untyped(),
            value: None,
            placeholder,
            empty,
            options,
            required: false,
            disabled: false,
            attrs: &NO_ATTRS,
        }
    }
}

/// Type to filter a list of options, then pick one.
///
/// The value lives in a native `<select>`: without the script it is the control, so the form
/// works and `required` is checked by the browser. The script (`combo_box.js`) hides it, shows a
/// text input with the ARIA 1.2 combobox pattern instead, builds the listbox from the select's
/// options (they are not sent twice), filters as the user types, and writes the pick back to
/// the select, which sends `change`. An option matches when its label or one of its terms holds
/// the typed text, and the matches are listed by how well they match, each kind in the options'
/// order: a label or term typed whole, then one that starts with the text, then the rest. Arrow keys move, Enter picks, Escape closes and restores
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
/// const COLORS: [Choice<Color>; 2] = [Choice::new(Color::Salt, "Salt & Pepper"), Choice::new(Color::Sand, "Desert Sand").terms(&["tan"])];
/// let field = Picker { value: Some(Color::Sand), ..Picker::new("color", Order::FIELDS.color, "Choose a color", "No matches", &COLORS) };
/// let html = ComboBox { field: &field }.to_html();
/// assert!(html.contains(r#"<select data-ref="select" id="color" name="color""#));
/// assert!(html.contains(r#"<option value="sand" selected data-terms="tan">Desert Sand</option>"#));
/// assert!(html.contains(r#"role="combobox""#) && html.contains(r#"value="Desert Sand""#));
/// ```
#[component(js)]
pub fn ComboBox<'a, V: Copy + PartialEq + std::fmt::Display, S: AsRef<str>>(field: &'a Picker<'a, V, S>) -> impl Render {
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
                    <option value={o.value} selected={f.value == Some(o.value)} data-terms={Terms(o.terms)}>{o.label}</option>
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
