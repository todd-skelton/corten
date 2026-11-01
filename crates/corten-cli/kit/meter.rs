// corten ui {{version}}: meter
//! A measure in a known range (quota used, storage, a budget), as the classes of the `<meter>` you
//! write, so a screen reader reads it as a value, not loose text:
//!
//! ```rust,ignore
//! <Field id={LimitId} label="Selling limit" hint="Resets on the 1st." error="">
//!     <meter class={Meter {}} id={LimitId} min="0" max="5000" low="3500" high="4500" optimum="0" value={sold}>
//!         {t.money(Minor(sold_cents), USD)}
//!     </meter>
//! </Field>
//! ```
//!
//! With no `low` or `high` it is a plain measure, in the primary color. With them, its color is the
//! state the browser works out from `low`, `high` and `optimum`: the success color in the good
//! range, the warning color one range off it, the destructive color two off. Progress through a
//! task is a `<progress>`, not a meter.
use corten::prelude::*;

/// A part without a rule in `meter.css` does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[component(css)]
/// fn Meter() -> impl ClassList {
///     (Meter, Thick)
/// }
/// ```
#[component(css)]
pub fn Meter() -> impl ClassList {
    Meter
}
