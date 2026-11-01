// corten ui {{version}}: field
//! A label, the control it names and a note under them: a hint in text, or the field's error
//! from the server (`e.message(f.email)`, drawn in the destructive color). The control goes inside
//! the `<label>`, so it is named with no `id` to keep in step:
//!
//! ```rust,ignore
//! <Field label="Email" note={e.message(f.email)}>
//!     <Input><input type="email" name={f.email} {..e.aria(f.email)}></Input>
//! </Field>
//! ```
//!
//! `note=""` when there is none. A hint the control should announce takes an id
//! (`<span id={EmailHint}>..</span>`) named by `e.aria(f.email).hint(&EmailHint)`. A checkbox, a switch and a radio button put their label after
//! the control: `Checkbox`, `Switch` and `Radio` do that.
use corten::prelude::*;

/// A part without a rule does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[component(css)]
/// fn Field() -> impl Render {
///     html! { <div class={(Field, Inline)}></div> }
/// }
/// ```
#[component(css)]
pub fn Field<'a, C: Render, N: Render>(label: &'a str, note: N, children: C) -> impl Render {
    html! {
        <div class={Field}>
            <label><span class={Label}>{label}</span>{children}</label>
            <div class={Note}>{note}</div>
        </div>
    }
}
