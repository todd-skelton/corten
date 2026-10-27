//! The design tokens (`tokens.json`) and the utility classes made from them. Pages are styled
//! utility-first: `class={(display(Display::Flex), gap(Space::_3))}` prints `display-flex gap-3`,
//! and a class the file lacks, or a value outside its scale, does not compile. What utilities
//! cannot say (element defaults, hover and focus) is in `static/style.css`.

corten::style::tokens!("tokens.json");

use corten::style::ClassList;

/// A raised panel: the surface, its border and shadow, and room inside.
pub fn card() -> impl ClassList {
    (bg(Color::Surface), border(Border::Line), rounded(Radius::Lg), shadow(Shadow::Sm), p(Space::_5))
}

/// A row of items, vertically centered, `space` apart.
pub fn row(space: Space) -> impl ClassList {
    (display(Display::Flex), items(Items::Center), gap(space))
}

/// A column of blocks, `space` apart.
pub fn stack(space: Space) -> impl ClassList {
    (display(Display::Grid), gap(space))
}

/// A button that is not the page's main action.
pub fn secondary() -> impl ClassList {
    (bg(Color::Surface), fg(Color::Accent))
}

/// Secondary text.
pub fn muted() -> impl ClassList {
    (fg(Color::Muted), text(FontSize::Sm))
}
