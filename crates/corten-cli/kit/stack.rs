// corten ui {{version}}: stack
//! The space between a screen's parts, so no component carries an outer margin: `Stack` puts the
//! children of the element you write in a column and `Cluster` in a row that wraps, `gap` apart:
//!
//! ```rust,ignore
//! <form class={Stack { gap: Gap::Lg }} method="post" action={url::Settings}>..</form>
//! <div class={Cluster { gap: Gap::Sm }}><h2>"Orders"</h2><span class={Badge { .. }}>"10"</span></div>
//! ```
//!
//! Each child takes the stack's width, a button too: put one in a `Cluster` to keep it its own
//! size. The browser's default block margins on the children (a paragraph's, a heading's) are
//! dropped, so the gap is the space between them; a rule of the app's own that sets a child's margin
//! still applies (the rule has no specificity). The gaps are the values the kit's components space their own parts with. An app
//! styled from its own token utilities spaces a screen with its `gap` utility instead (the styles
//! guide), so its screens follow its scale.
use corten::prelude::*;

/// The space between the children. Each is a class in `stack.css`, so one without a rule does
/// not compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gap {
    /// .25rem
    Xs,
    /// .5rem
    Sm,
    /// 1rem
    Md,
    /// 1.5rem
    Lg,
    /// 2rem
    Xl,
}

/// A gap without a rule in `stack.css` does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[component(css)]
/// fn Stack() -> impl ClassList {
///     (Stack, Xxl)
/// }
/// ```
#[component(css)]
pub fn Stack(gap: Gap) -> impl ClassList {
    let gap = match gap {
        Gap::Xs => Xs,
        Gap::Sm => Sm,
        Gap::Md => Md,
        Gap::Lg => Lg,
        Gap::Xl => Xl,
    };
    (Stack, gap)
}

/// A row of items, centered across it, that wraps onto the next line when it is full.
#[component(css = Stack)]
pub fn Cluster(gap: Gap) -> impl ClassList {
    let gap = match gap {
        Gap::Xs => Xs,
        Gap::Sm => Sm,
        Gap::Md => Md,
        Gap::Lg => Lg,
        Gap::Xl => Xl,
    };
    (Cluster, gap)
}
