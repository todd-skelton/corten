//! The design tokens (`tokens.json`) and the utility classes made from them. Pages are styled
//! utility-first: `class={(display_flex(), gap_3())}` prints `display-flex gap-3`,
//! and a class the file lacks, or a value outside its scale, does not compile. What utilities
//! cannot say (element defaults, hover and focus) is in `static/style.css`. The `flex` utility is
//! `flex-wrap` (`flex_wrap()`, `flex_nowrap()`, as Tailwind names them), so the `flex` property
//! itself would need another name here.

corten::style::tokens!("tokens.json");

use corten::style::ClassList;

/// A raised panel: the surface, its border and shadow, and room inside.
pub fn card() -> impl ClassList {
    (bg_surface(), border_line(), rounded_lg(), shadow_sm(), p_5())
}

/// A row of items, vertically centered, `space` apart.
pub fn row(space: Space) -> impl ClassList {
    (display_flex(), items_center(), gap(space))
}

/// A column of blocks, `space` apart.
pub fn stack(space: Space) -> impl ClassList {
    (display_grid(), gap(space))
}

/// A button that is not the page's main action.
pub fn secondary() -> impl ClassList {
    (bg_surface(), fg_accent())
}

/// Secondary text.
pub fn muted() -> impl ClassList {
    (fg_muted(), text_sm())
}
