// corten ui {{version}}: stats
//! Labelled figures: a dashboard's summary. The list is the `<dl>` you write, with the `Stats`
//! look; each `Stat` is a term (its label) and its descriptions (the value, then a note), so a
//! screen reader reads the label with the figure:
//!
//! ```rust,ignore
//! <dl class={Stats {}}>
//!     <Stat label="Sales" value={t.money(Minor(cents), USD)} note="This month" tone={StatTone::Neutral} />
//!     <Stat label="Late" value={late} note="Ship today" tone={StatTone::Destructive} />
//! </dl>
//! ```
//!
//! `note=""` draws nothing. A tone colors the value; say what it means in the label or the note,
//! since color alone does not tell everyone.
use corten::prelude::*;

/// The color of a figure's value. Each is a class in `stats.css`, so one without a rule does not
/// compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatTone {
    Neutral,
    Info,
    Success,
    Warning,
    Destructive,
}

/// A part without a rule in `stats.css` does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[component(css)]
/// fn Stats() -> impl ClassList {
///     (Stats, Dense)
/// }
/// ```
#[component(css)]
pub fn Stats() -> impl ClassList {
    Stats
}

/// One figure: its label, its value in tabular figures, and a note under it.
#[component(css = Stats)]
pub fn Stat<'a, V: Render, N: Render>(label: &'a str, value: V, note: N, tone: StatTone) -> impl Render {
    let tone = match tone {
        StatTone::Neutral => None,
        StatTone::Info => Some(Info),
        StatTone::Success => Some(Success),
        StatTone::Warning => Some(Warning),
        StatTone::Destructive => Some(Destructive),
    };
    html! {
        <div class={(Stat, tone)}>
            <dt>{label}</dt>
            <dd class={Value}>{value}</dd>
            <dd class={Note}>{note}</dd>
        </div>
    }
}
