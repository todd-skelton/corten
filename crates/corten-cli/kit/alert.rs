// corten ui {{version}}: alert
//! A message the page calls out. Its heading and text are yours (`<h2>` to `<h6>` or `<strong>`,
//! then `<p>`). It is `role="alert"`, so a screen reader reads it when a patch brings it in, as
//! a form's error does.
//!
//! ```rust,ignore
//! <Alert variant={AlertVariant::Destructive}>
//!     <strong>"Payment failed"</strong>
//!     <p>"Your card was declined."</p>
//! </Alert>
//! ```
use corten::prelude::*;

/// Each variant is a class in `alert.css`, so one without a rule does not compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertVariant {
    Neutral,
    Destructive,
}

/// A variant without a rule in `alert.css` does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[component(css)]
/// fn Alert() -> impl Render {
///     html! { <div class={(Alert, Warning)}></div> }
/// }
/// ```
#[component(css)]
pub fn Alert<C: Render>(variant: AlertVariant, children: C) -> impl Render {
    let variant = match variant {
        AlertVariant::Neutral => Neutral,
        AlertVariant::Destructive => Destructive,
    };
    html! { <div class={(Alert, variant)} role="alert">{children}</div> }
}
