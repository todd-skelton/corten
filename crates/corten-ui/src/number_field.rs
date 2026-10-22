use corten::{prelude::*, view::Attrs, FieldName};

/// A number field's input and buttons. Start from [`Number::new`] and set what differs:
///
/// ```ignore
/// let qty = Number { value: Some(1.0), min: Some(1.0), max: Some(99.0), ..Number::new("qty", f.qty, t.fewer(), t.more()) };
/// html! { <NumberField field={&qty} /> }
/// ```
pub struct Number<'a> {
    /// The input's `id`, for the app's `<label for>`.
    pub id: &'a str,
    /// The form field's name: `Input::FIELDS.qty`.
    pub name: FieldName,
    pub value: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// The amount a button or arrow key adds; it also sets the decimals shown (`0.01` shows
    /// `12.50`). Page Up and Page Down add ten steps.
    pub step: f64,
    pub required: bool,
    pub disabled: bool,
    pub decrease: Step<'a>,
    pub increase: Step<'a>,
    /// More attributes for the input, such as `errors.aria(field)` or a placeholder.
    pub attrs: &'a dyn Attrs,
}

/// One of the two buttons.
pub struct Step<'a> {
    /// The button's accessible name, in the page's language: "Fewer".
    pub label: &'a str,
    /// What it shows: text or an icon.
    pub content: &'a dyn Render,
}

const MINUS: &str = "\u{2212}";
const PLUS: &str = "+";
static NO_ATTRS: [(&str, bool); 0] = [];

impl<'a> Number<'a> {
    /// A field with no value, no bounds, a step of 1 and buttons showing − and +.
    pub fn new(id: &'a str, name: FieldName, decrease: &'a str, increase: &'a str) -> Self {
        Number {
            id,
            name,
            value: None,
            min: None,
            max: None,
            step: 1.0,
            required: false,
            disabled: false,
            decrease: Step { label: decrease, content: &MINUS },
            increase: Step { label: increase, content: &PLUS },
            attrs: &NO_ATTRS,
        }
    }

    /// `value` with the decimals of `step`: `12.5` with a step of `0.01` is `12.50`.
    fn shown(&self, value: f64) -> String {
        format!("{value:.*}", decimals(self.step))
    }
}

/// Decimal places of a step: 1 has none, 0.25 has two.
fn decimals(step: f64) -> usize {
    let text = step.to_string();
    text.split_once('.').map_or(0, |(_, fraction)| fraction.len())
}

/// A number input between a decrease and an increase button.
///
/// The input is a native `<input type="number">`, so without the script it still steps with the
/// arrow keys, and the browser checks `min`, `max`, `step` and `required` when the form is
/// sent. The script (`number_field.js`) adds the buttons (press and hold repeats), Page Up and
/// Page Down, keeps the step's decimals, disables a button at its bound, and sends `input` and
/// `change` events like typing does.
///
/// Parts: a `div` around `input` and `button[aria-controls]` twice, the first decreasing;
/// `button:disabled` at a bound. Hide the input's own spin buttons in the design system's CSS
/// (`appearance: textfield` and `::-webkit-inner-spin-button`).
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::{Number, NumberField};
///
/// #[derive(Fields)]
/// struct Order { qty: u32 }
///
/// let qty = Number { value: Some(1.0), min: Some(1.0), ..Number::new("qty", Order::FIELDS.qty, "Fewer", "More") };
/// let html = NumberField { field: &qty }.to_html();
/// assert!(html.starts_with(r#"<div data-scope="number-field" data-on="connect:number_field.connect(@decrease,@input,@increase,$s)"><button data-ref="decrease" type="button" aria-controls="qty" aria-label="Fewer" tabindex="-1" disabled"#));
/// assert!(html.contains(r#"<input data-ref="input" type="number" inputmode="decimal" id="qty" name="qty" value="1" min="1" step="1""#));
/// ```
#[component(js)]
pub fn NumberField<'a>(field: &'a Number<'a>) -> impl Render {
    let f = field;
    let (input, decrease, increase) = refs();
    let at_min = matches!((f.value, f.min), (Some(v), Some(min)) if v <= min);
    let at_max = matches!((f.value, f.max), (Some(v), Some(max)) if v >= max);
    html! {
        <div on:connect={js::connect(decrease, input, increase, signal)}>
            <button
                ref={decrease}
                type="button"
                aria-controls={f.id}
                aria-label={f.decrease.label}
                tabindex="-1"
                disabled={f.disabled || at_min}
                on:pointerdown={js::press(event, target, input, -1)}
                on:click={js::click(input, -1)}
            >
                {f.decrease.content}
            </button>
            <input
                ref={input}
                type="number"
                inputmode="decimal"
                id={f.id}
                name={f.name}
                value={f.value.map(|v| f.shown(v))}
                min={f.min}
                max={f.max}
                step={f.step}
                required={f.required}
                disabled={f.disabled}
                on:keydown={js::key(event, input)}
                on:input={js::sync(decrease, input, increase)}
                on:change={js::sync(decrease, input, increase)}
                {..f.attrs}
            >
            <button
                ref={increase}
                type="button"
                aria-controls={f.id}
                aria-label={f.increase.label}
                tabindex="-1"
                disabled={f.disabled || at_max}
                on:pointerdown={js::press(event, target, input, 1)}
                on:click={js::click(input, 1)}
            >
                {f.increase.content}
            </button>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_keep_the_decimals_of_the_step() {
        assert_eq!(decimals(1.0), 0);
        assert_eq!(decimals(0.25), 2);
        assert_eq!(decimals(0.01), 2);
        #[derive(Fields)]
        #[allow(dead_code)]
        struct Form {
            p: f64,
        }
        let price = Number { value: Some(12.5), step: 0.01, ..Number::new("p", Form::FIELDS.p, "Less", "More") };
        assert!(NumberField { field: &price }.to_html().contains(r#"value="12.50""#));
    }
}
