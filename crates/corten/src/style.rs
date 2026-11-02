//! Styles: a stylesheet next to each component, design tokens, and class names that are checked.
//!
//! `#[component(css)]` gives a component the stylesheet with its name in
//! snake_case, next to its source file: `OfferList` has `offer_list.css`. At compile time every
//! class in the file is prefixed with the component's name, so `.title` is `.offer-list__title`
//! and two components can both have a `.title`. The markup names classes with constants:
//!
//! ```css
//! /* src/views/offer_list.css */
//! .list { display: grid; gap: var(--space-2); }
//! .row:hover { background: var(--color-surface-2); }
//! .row.selected { outline: 2px solid var(--color-accent); }
//! ```
//!
//! ```ignore
//! #[component(css)]
//! pub fn OfferList<'a>(offers: &'a [Offer], selected: Option<&'a str>) -> impl Render {
//!     html! {
//!         <ul class={List}>
//!             for o in offers {
//!                 <li class={(Row, (selected == Some(&o.id)).then_some(Selected))}>{o.price}</li>
//!             }
//!         </ul>
//!     }
//! }
//! ```
//!
//! - `class={List}` and `class={[List, Wide]}` are static markup: `class="offer-list__list"`.
//!   Anything else (a tuple, an `Option`, a variable) is written when the page renders; see
//!   [`ClassList`].
//! - A class the file does not have is a compile error, and so is a class of the file written as
//!   text (`class="list"`), which would not match the scoped rule.
//! - Every rule must start inside the component: a selector names one of the file's classes.
//!   `h2 { .. }` alone is a compile error; `.list h2 { .. }` is fine.
//!   Element defaults, resets and fonts belong in the app's `static/style.css`.
//! - Variants are Rust: `match tone { Tone::Danger => Danger, Tone::Neutral => Neutral }` picks
//!   a class, so a variant without a rule does not compile.
//! - A helper component shares another's stylesheet: `#[component(css = OfferList)]` in the same
//!   module, `#[component(css = super::offers::OfferList)]` from another (a screen's panels in
//!   files of their own). Any module of the same crate that can see the owner shares its sheet,
//!   naming it by its path (not through a `use` of it); its classes are in scope in the helper as
//!   in the owner, and its uses count for the owner. Classes are in scope only in a component: a
//!   plain `fn` that picks a class is a helper component, or a `match` in the markup.
//! - The component gets a constant with its own name, `OfferList: corten::Component`; list it
//!   once in [`crate::App::components`]. Corten joins every listed stylesheet, in order, into one
//!   file at a content-versioned URL, `components.css`, linked from `<head>` after the shell's
//!   own tags. The file starts with `[hidden] { display: none !important }` (except
//!   `hidden="until-found"`, in any case), so an element hidden with the attribute stays hidden whatever
//!   `display` a component's rule gives it. In a debug build under `corten dev`, a page that renders a component the app did
//!   not list is an error that names it.
//!
//! [`tokens!`] reads a design-token file (JSON) into CSS custom properties, theme overrides,
//! typed scales and responsive utility classes, so a design system has one source for its tokens.
//!
//! ```
//! use corten::prelude::*;
//!
//! /// Its stylesheet is badge_list.css, next to this file.
//! #[component(css)]
//! fn BadgeList<'a>(badges: &'a [&'a str], hot: bool) -> impl Render {
//!     html! {
//!         <ul class={List}>
//!             for b in badges { <li class={(Badge, hot.then_some(Hot))}>{b}</li> }
//!         </ul>
//!     }
//! }
//!
//! let html = BadgeList { badges: &["Foil"], hot: true }.to_html();
//! assert_eq!(html, r#"<ul class="badge-list__list"><li class="badge-list__badge badge-list__hot">Foil</li></ul>"#);
//! let style = BadgeList.style.unwrap();
//! assert!(style.css.starts_with(".badge-list__list{"));
//! assert_eq!(style.classes, ["list", "badge", "hot"]);
//! ```
//!
//! A screen whose views are in several modules shares one stylesheet by naming its owner by path:
//!
//! ```
//! mod lists {
//!     use corten::prelude::*;
//!
//!     #[component(css)]
//!     pub fn BadgeList<'a>(badges: &'a [&'a str]) -> impl Render {
//!         html! { <ul class={List}> for b in badges { <li class={Badge}>{b}</li> } </ul> }
//!     }
//! }
//!
//! mod panels {
//!     use corten::prelude::*;
//!
//!     #[component(css = super::lists::BadgeList)]
//!     pub fn HotBadge<'a>(label: &'a str) -> impl Render {
//!         html! { <b class={[Badge, Hot]}>{label}</b> }
//!     }
//! }
//!
//! fn main() {
//!     use corten::view::Render;
//!     assert_eq!(lists::BadgeList { badges: &["Foil"] }.to_html(), r#"<ul class="badge-list__list"><li class="badge-list__badge">Foil</li></ul>"#);
//!     assert_eq!(panels::HotBadge { label: "Foil" }.to_html(), r#"<b class="badge-list__badge badge-list__hot">Foil</b>"#);
//! }
//! ```
//!
//! A component named as the owner must have a stylesheet of its own:
//!
//! ```compile_fail
//! mod plain {
//!     use corten::prelude::*;
//!     #[component]
//!     pub fn Plain() -> impl Render {
//!         html! { <p></p> }
//!     }
//! }
//!
//! use corten::prelude::*;
//! #[component(css = plain::Plain)]
//! fn Helper() -> impl Render {
//!     html! { <p></p> }
//! }
//! # fn main() {}
//! ```
//!
//! nor can a module that cannot see the owner share its sheet:
//!
//! ```compile_fail,E0603
//! mod screen {
//!     use corten::prelude::*;
//!     #[component(css)]
//!     fn BadgeList() -> impl Render {
//!         html! { <ul class={(List, Badge, Hot)}></ul> }
//!     }
//! }
//!
//! use corten::prelude::*;
//! #[component(css = screen::BadgeList)]
//! fn Helper() -> impl Render {
//!     html! { <p class={Badge}></p> }
//! }
//! # fn main() {}
//! ```
//!
//! A class the stylesheet does not have does not compile:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[component(css)]
//! fn BadgeList() -> impl Render {
//!     html! { <ul class={Lsit}></ul> }
//! }
//! ```
//!
//! nor does one of its classes written as text:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[component(css)]
//! fn BadgeList() -> impl Render {
//!     html! { <ul class="list"></ul> }
//! }
//! ```
//!
//! Its classes are global to the app (`.badge-list__*`), so two components with one name in
//! different modules (each feature's `SettingsForm`) fail to build, at link, rather than the app
//! when it starts; the error names the symbol `corten__two_components_style__badge_list`. Rename
//! one (`ShippingSettingsForm`):
//!
//! ```compile_fail
//! mod shipping {
//!     use corten::prelude::*;
//!     #[component(css)]
//!     pub fn BadgeList() -> impl Render {
//!         html! { <ul class={(List, Badge, Hot)}></ul> }
//!     }
//! }
//! mod settings {
//!     use corten::prelude::*;
//!     #[component(css)]
//!     pub fn BadgeList() -> impl Render {
//!         html! { <ul class={(List, Badge, Hot)}></ul> }
//!     }
//! }
//! fn main() {
//!     let _ = (shipping::BadgeList, settings::BadgeList);
//! }
//! ```
//!
//! and a class belongs to its component; another one cannot use it:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[component(css)]
//! fn BadgeList() -> impl Render {
//!     html! { <ul class={List}></ul> }
//! }
//!
//! #[component]
//! fn Other() -> impl Render {
//!     html! { <ul class={List}></ul> }
//! }
//! ```
//!
//! A rule that reaches outside the component does not compile either (`leaky_list.css` has
//! `h2 { margin: 0 }`):
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[component(css)]
//! fn LeakyList() -> impl Render {
//!     html! { <ul></ul> }
//! }
//! ```
//!
//! # Values from data
//!
//! A size or position computed as the page renders (a meter's fill, a bar's length, a mark on a
//! timeline) is a custom property the stylesheet registers with `@property` and reads with
//! `var()`, set in the markup with `style={..}`:
//!
//! ```css
//! /* meter.css */
//! @property --fill { syntax: "<percentage>"; inherits: false; initial-value: 0%; }
//! .meter { background: linear-gradient(to right, currentColor var(--fill), transparent var(--fill)); }
//! ```
//!
//! ```
//! use corten::prelude::*;
//!
//! #[component(css)]
//! fn Meter(used: f64, nudge: Option<f64>) -> impl Render {
//!     html! { <div class={Meter} style={(FILL.pct(used), nudge.map(|px| NUDGE.px(px)))}></div> }
//! }
//!
//! assert_eq!(
//!     Meter { used: 42.5, nudge: None }.to_html(),
//!     r#"<div class="meter__meter" data-vars="--meter__fill:42.5%"></div>"#
//! );
//! assert!(Meter.style.unwrap().css.starts_with("@property --meter__fill{syntax:\"<percentage>\""));
//! ```
//!
//! - Each `@property` whose `syntax` is a number (`<number>`, `<integer>`, `<percentage>`,
//!   `<length>`, `<length-percentage>`, `<angle>`, `<time>`) is a constant named in capitals,
//!   `--fill` is `FILL` (a [`Var`]), whose methods are the units its syntax allows:
//!   `FILL.pct(42.0)`, `GAP.px(4.0)`, `SPAN.set(3)`. A value is a number and a unit, so no CSS
//!   text from data reaches the page, and a number that is not finite sets nothing.
//! - The property is the component's, as its classes are: `--fill` is `--meter__fill` everywhere
//!   in the file, so two components can both register a `--fill`. A rule the browser would ignore
//!   (no `syntax`, `inherits` or `initial-value`, or for a number syntax an `initial-value` that
//!   is not one, or depends on the font or a container, `1em`) does not compile.
//! - `style={..}` takes one value, an `Option` (`None` sets nothing) or a tuple or array of them,
//!   and writes `data-vars`. A full page gets one `<style>` at `<Corten />` with a rule for each
//!   distinct value (`[data-vars="--meter__fill:42.5%"]{--meter__fill:42.5%!important}`, which wins
//!   over the component's rules, as the runtime's inline `!important` values do; an `!important`
//!   declaration of a property markup sets does not compile), allowed by its
//!   hash in that response's CSP, so it paints with the sizes and no script, and no style
//!   attribute is allowed anywhere. A patch, a client navigation and a `<corten-live>` fragment
//!   arrive under the policy the document loaded with, so the runtime sets their values through
//!   the CSSOM, which the policy allows. A style attribute written as text (`style="width: 4px"`)
//!   stays blocked unless the area allows them.
//!
//! CSS text is not a value:
//!
//! ```compile_fail,E0277
//! use corten::prelude::*;
//!
//! #[component(css)]
//! fn Meter(used: f64) -> impl Render {
//!     html! { <div class={Meter} style={format!("width: {used}%")}></div> }
//! }
//! ```
//!
//! nor is a unit the property's syntax does not allow (`--fill` is a `<percentage>`):
//!
//! ```compile_fail,E0599
//! use corten::prelude::*;
//!
//! #[component(css)]
//! fn Meter(used: f64) -> impl Render {
//!     html! { <div class={Meter} style={FILL.px(used)}></div> }
//! }
//! ```
//!
//! and another component cannot set a component's property:
//!
//! ```compile_fail,E0425
//! use corten::prelude::*;
//!
//! #[component(css)]
//! fn Meter(used: f64) -> impl Render {
//!     html! { <div class={Meter} style={FILL.pct(used)}></div> }
//! }
//!
//! #[component]
//! fn Other(used: f64) -> impl Render {
//!     html! { <div style={FILL.pct(used)}></div> }
//! }
//! ```
//!
//! Markup's values win over the component's rules, so its stylesheet may not mark a property it
//! registers `!important` (`pinned_meter.css` has `.meter.full { --fill: 100% !important }`, which
//! would win on a full page and lose after a patch):
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[component(css)]
//! fn PinnedMeter(used: f64) -> impl Render {
//!     html! { <div class={Meter} style={FILL.pct(used)}></div> }
//! }
//! ```
//!
//! One `style={..}` per element (a browser keeps the first of two attributes); a tuple sets several:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[component(css)]
//! fn Meter(used: f64) -> impl Render {
//!     html! { <div class={Meter} style={FILL.pct(used)} style={NUDGE.px(2.0)}></div> }
//! }
//! ```
//!
//! `style` is checked only as written in lowercase, so `STYLE` does not compile either:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! let width = 4;
//! let view = html! { <div STYLE={format!("width: {width}px")}></div> };
//! ```
//!
//! `data-vars` is Corten's to write, in any case (HTML names are case-insensitive):
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! let view = html! { <div data-vars="--fill:4px"></div> };
//! ```
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! let view = html! { <div data-Vars="--fill:4px"></div> };
//! ```
//!
//! and only Corten's types are values, so no type of the app's writes CSS text there:
//!
//! ```compile_fail,E0277
//! struct Css(String);
//! impl corten::style::Vars for Css {
//!     fn write_vars(&self, out: &mut String) {
//!         out.push_str(&self.0)
//!     }
//! }
//! ```

use std::cell::RefCell;

mod vars;
pub(crate) use vars::{capture as capture_vars, collect as collect_vars, replay as replay_vars, Written};
pub use vars::{syntax, Declaration, Var, Vars};

/// Design tokens as CSS and Rust, from W3C Design Tokens files (DTCG 2025.10). See [the module](self)
/// and `examples/card-market/tokens/`.
///
/// `tokens!("tokens/tokens.resolver.json")` (the path from the crate's root) reads a resolver
/// document and the token files it names; `tokens!("tokens/base.tokens.json")` reads one token file,
/// for tokens with no themes.
///
/// - Each top-level group's tokens are a scale (its `$root` is a custom property alone): `{"space": {"$type": "dimension", "1": {"$value": {"value":
///   4, "unit": "px"}}}}` is the custom property `--space-1: 4px` and the enum `Space`
///   (`Space::_1`), with `var()` (`"var(--space-1)"`) and `value()`. A nested group joins its
///   names with `-` (`--color-brand-primary`, `Color::BrandPrimary`), and a name is written as
///   CSS writes one (`Brand Primary` is `brand-primary`). A token at the top of the file is a
///   custom property with no enum.
/// - Values are written in CSS: colors in CSS Color 4 (an sRGB color whose `hex` agrees with its
///   components is the hex), dimensions and durations with their unit, shadows, borders,
///   transitions, cubic Béziers, font families and weights. A reference, `"{color.accent}"`, is
///   `var(--color-accent)`; a JSON pointer into a value is the value it points at, following a
///   token that is itself a reference to the one with a value (a chain of more than 256 is an
///   error, so a file cannot make the compiler work without end). A gradient
///   (no direction) and a typography token (several properties) have no one CSS value and are
///   errors; a stroke style of dashes and caps is `dashed`, the Format module's fallback, as CSS
///   sets no dash pattern.
/// - A resolver's modifiers are the themes. A modifier's `default` context (required here) is the
///   values on `:root`; each context's values apply under `[data-{modifier}="{context}"]` (the
///   default's for the tokens the others change, so it applies inside one of them). A context
///   that defines a token the defaults lack, or changes a token's type, does not compile:
///
/// ```compile_fail
/// // `dark` adds `color.link`, which would have no value when the page chooses no theme.
/// corten::style::tokens!("src/style/tokens/adds-a-token.resolver.json");
/// ```
///
/// Contexts of two modifiers on one element follow in resolutionOrder, the later winning; a file
/// whose rules would give, with two or more of them, other values than the resolver does not
/// compile:
///
/// ```compile_fail
/// // `roomy` sets `space.gap`, and `compact`'s and `normal`'s rules declare it `var(--space-1)`
/// // again (it reads `space.1`, which `compact` changes), so with `roomy` and either one chosen
/// // the gap would not be the 12px the resolver gives.
/// corten::style::tokens!("src/style/tokens/two-modifiers.resolver.json");
/// ```
///
/// ```compile_fail
/// // `dark` makes `color.ink` a dimension, which `Color::Ink` and every `var(--color-ink)` are not.
/// corten::style::tokens!("src/style/tokens/retypes-a-token.resolver.json");
/// ```
///
/// So do a reference to a group (its root token is `{brand.ratio.$root}`), references that go
/// round in a circle, two names that are one CSS name, breakpoints out
/// of order, a modifier with no default, a token with no one CSS value and a media condition for
/// the default context:
///
/// ```compile_fail
/// // `{brand.ratio}` is a group; a reference names a token.
/// corten::style::tokens!("src/style/tokens/reference-to-a-group.tokens.json");
/// ```
///
/// ```compile_fail
/// // `space.a` is `space.b`'s value, which is `space.a`'s: neither has a value.
/// corten::style::tokens!("src/style/tokens/circular-reference.tokens.json");
/// ```
///
/// ```compile_fail
/// // `font-size` and `FONT-SIZE` are both `--color-font-size`.
/// corten::style::tokens!("src/style/tokens/one-css-name.tokens.json");
/// ```
///
/// ```compile_fail
/// // `md` after `lg`: its rules would come later and win at every width `lg` covers.
/// corten::style::tokens!("src/style/tokens/breakpoints-out-of-order.tokens.json");
/// ```
///
/// ```compile_fail
/// // `theme` has no default, so a page that chooses no theme would have no values.
/// corten::style::tokens!("src/style/tokens/no-default.resolver.json");
/// ```
///
/// ```compile_fail
/// // A gradient token has no direction, which CSS needs.
/// corten::style::tokens!("src/style/tokens/gradient.tokens.json");
/// ```
///
/// ```compile_fail
/// // 300 pointers, each into the value before: past the 256 `tokens!` follows in a chain.
/// corten::style::tokens!("src/style/tokens/long-pointer-chain.tokens.json");
/// ```
///
/// ```compile_fail
/// // Each set names the next twice: 2^20 copies, past the 100,000 references, `$extends` and sets
/// // `tokens!` follows in all.
/// corten::style::tokens!("src/style/tokens/repeated-sets.resolver.json");
/// ```
///
/// ```compile_fail
/// // Two modifiers of 16 contexts: 17 x 17 combinations (each attribute absent or chosen), past
/// // the 256 `tokens!` resolves to check their CSS.
/// corten::style::tokens!("src/style/tokens/many-contexts.resolver.json");
/// ```
///
/// ```compile_fail
/// // Groups nested 131 deep: past the 128 levels of objects and arrays `tokens!` reads, so a
/// // file cannot overflow the compiler's stack.
/// corten::style::tokens!("src/style/tokens/too-deep.tokens.json");
/// ```
///
/// ```compile_fail
/// // 1e400 is past any number CSS holds; a browser would clamp it to a value of its own.
/// corten::style::tokens!("src/style/tokens/too-large.tokens.json");
/// ```
///
/// ```compile_fail
/// // A resolver with nothing in its resolutionOrder has no tokens to give.
/// corten::style::tokens!("src/style/tokens/empty-order.resolver.json");
/// ```
///
/// ```compile_fail
/// // A `$root` on the file itself has no group name to be its custom property's.
/// corten::style::tokens!("src/style/tokens/top-level-root.tokens.json");
/// ```
///
/// ```compile_fail
/// // A file is a path relative to the one naming it, not a URL or an absolute path.
/// corten::style::tokens!("src/style/tokens/absolute-path.resolver.json");
/// ```
///
/// ```compile_fail
/// // An sRGB color's `hex` is its components at 8 bits: `#ff0000` and `[0.1137, 0.1059, 0.149]`
/// // (`#1d1b26`) are two colors.
/// corten::style::tokens!("src/style/tokens/hex-disagrees.tokens.json");
/// ```
///
/// ```compile_fail
/// // A hex alone is not a color in the format; the error is the object to write.
/// corten::style::tokens!("src/style/tokens/hex-alone.tokens.json");
/// ```
///
/// ```compile_fail
/// // `.2xl\:gap-1` is no CSS class without escapes: a breakpoint or utility starts with a letter.
/// corten::style::tokens!("src/style/tokens/digit-breakpoint.tokens.json");
/// ```
///
/// ```compile_fail
/// // A typography token is five CSS properties, and a custom property holds one value.
/// corten::style::tokens!("src/style/tokens/typography.tokens.json");
/// ```
///
/// ```compile_fail
/// // `light` is the default, the values a page has when it chose no theme: no condition applies it.
/// corten::style::tokens!("src/style/tokens/media-for-the-default.resolver.json");
/// ```
///
/// Each fails for the reason given (a unit test reads the same files and checks the message).
///
/// Corten's own settings are under `$extensions`, key `"run.corten"`:
///
/// - On a token file's root group, `breakpoints` (`{"md": {"value": 48, "unit": "rem"}}`,
///   wider than 0, smallest first) and `utilities`: classes for one property, from a top-level
///   group of tokens named as the file writes it (`{"gap": {"property": "gap", "group": "space"}}`)
///   or from their own values
///   (`{"direction": {"property": "flex-direction", "values": {"row": "row", "column": "column"}}}`).
///   Each value is a function of its own, `gap_2()` and `direction_row()` (`gap-2`,
///   `direction-row`), for `class={..}`. The utility's function takes a value chosen as the page
///   renders, `gap(space)`, or one per breakpoint, `gap(Responsive::new(Space::_2).md(Space::_4))`
///   (`gap-2 md:gap-4`). Two that would have one name (a utility `border-b` and `border`'s value
///   `b`) are a compile error that names both. Inline `style=` attributes are blocked by the
///   content security policy; these are the typed way to set one value per use.
/// - On a modifier, `media`: a context the page follows under a media condition when it chose
///   none, `{"dark": "(prefers-color-scheme: dark)"}` (`:where(:root):not([data-theme])` in that
///   condition), so the system's dark mode needs no second copy of the values.
///
/// It defines `TOKENS: corten::Component` with the generated stylesheet; list it first in
/// `App::components` so the components' rules come after it.
pub use corten_macros::tokens;

/// A component's stylesheet, scoped by `#[component(css)]`, or a token file's, from [`tokens!`].
#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// The class prefix, `"offer-list"`, or the token file's path.
    pub name: &'static str,
    /// `"offer_list.css"`.
    pub file: &'static str,
    /// The CSS, scoped and without comments.
    pub css: &'static str,
    /// The classes, as the file writes them.
    pub classes: &'static [&'static str],
}

impl Style {
    /// True for a token file's stylesheet: custom properties, themes and utilities that apply to
    /// every page, where a component's rules apply only to its own classes.
    pub(crate) fn global(&self) -> bool {
        self.name == self.file
    }
}

/// One class of a component's stylesheet: the constants `#[component(css)]` makes, `Title`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Class(&'static str);

impl Class {
    #[doc(hidden)]
    pub const fn __new(name: &'static str) -> Self {
        Class(name)
    }

    /// The scoped name, `"offer-list__title"`.
    pub const fn name(self) -> &'static str {
        self.0
    }
}

/// Utility classes for one value per breakpoint, from a [`tokens!`] utility:
/// `gap(Responsive::new(Space::_2).md(Space::_4))`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Classes<const N: usize>([Option<&'static str>; N]);

impl<const N: usize> Classes<N> {
    #[doc(hidden)]
    pub const fn __new(names: [Option<&'static str>; N]) -> Self {
        Classes(names)
    }

    /// The class names, smallest breakpoint first.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.0.iter().flatten().copied()
    }
}

/// What `class={..}` takes: a [`Class`], [`Classes`], text, an `Option` of one (`None` adds
/// nothing), or a tuple of them, written space-separated. With nothing to write, the attribute is
/// left out.
#[diagnostic::on_unimplemented(
    message = "class={{..}} takes classes: a class of the component's stylesheet (Title), utility classes, text, an Option or a tuple of them",
    label = "not a class list"
)]
pub trait ClassList {
    /// Writes each class with a space before it.
    fn write_classes(&self, out: &mut String);
}

impl ClassList for Class {
    #[inline]
    fn write_classes(&self, out: &mut String) {
        out.push(' ');
        out.push_str(self.0);
    }
}

impl<const N: usize> ClassList for Classes<N> {
    #[inline]
    fn write_classes(&self, out: &mut String) {
        for name in self.0.iter().flatten() {
            out.push(' ');
            out.push_str(name);
        }
    }
}

impl ClassList for str {
    fn write_classes(&self, out: &mut String) {
        if !self.trim().is_empty() {
            out.push(' ');
            crate::view::escape_into(self.trim(), out);
        }
    }
}

impl ClassList for String {
    fn write_classes(&self, out: &mut String) {
        self.as_str().write_classes(out)
    }
}

impl<T: ClassList + ?Sized> ClassList for &T {
    #[inline]
    fn write_classes(&self, out: &mut String) {
        (**self).write_classes(out)
    }
}

impl<T: ClassList> ClassList for Option<T> {
    #[inline]
    fn write_classes(&self, out: &mut String) {
        if let Some(value) = self {
            value.write_classes(out)
        }
    }
}

impl<T: ClassList> ClassList for [T] {
    fn write_classes(&self, out: &mut String) {
        for value in self {
            value.write_classes(out)
        }
    }
}

impl<T: ClassList, const N: usize> ClassList for [T; N] {
    fn write_classes(&self, out: &mut String) {
        self.as_slice().write_classes(out)
    }
}

macro_rules! tuples {
    ($(($($name:ident),+)),*) => {$(
        impl<$($name: ClassList),+> ClassList for ($($name,)+) {
            #[allow(non_snake_case)]
            #[inline]
            fn write_classes(&self, out: &mut String) {
                let ($($name,)+) = self;
                $($name.write_classes(out);)+
            }
        }
    )*};
}
tuples!((A, B), (A, B, C), (A, B, C, D), (A, B, C, D, E), (A, B, C, D, E, F), (A, B, C, D, E, F, G), (A, B, C, D, E, F, G, H));

thread_local! {
    static RENDERED: RefCell<Option<Vec<&'static crate::Component>>> = const { RefCell::new(None) };
}

/// Runs `render` and returns the components with a script or stylesheet it rendered (debug
/// builds of the app only; release builds record nothing).
pub(crate) fn collect<T>(render: impl FnOnce() -> T) -> (T, Vec<&'static crate::Component>) {
    let (value, list) = crate::gather(&RENDERED, Vec::new(), render);
    (value, list.unwrap_or_default())
}

/// Used by the code `html!` and `#[component]` generate. Not a public API.
/// The custom properties `css` declares (`--gap: 4px` in a block, `@property --gap`) and those
/// it reads with `var(--gap)` and no fallback, comments and strings skipped. A `var()` with a
/// fallback is skipped whole: the fallback is what it is for.
pub(crate) fn custom_properties(css: &str) -> (Vec<&str>, Vec<&str>) {
    let b = css.as_bytes();
    let (mut declared, mut needed) = (Vec::new(), Vec::new());
    let ident = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_') || c >= 0x80;
    let name_end = |mut i: usize| {
        while i < b.len() && (ident(b[i]) || b[i] == b'\\') {
            i += if b[i] == b'\\' { 2 } else { 1 };
        }
        i.min(b.len())
    };
    let space = |mut i: usize| {
        while b.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        i
    };
    let string_end = |mut i: usize, quote: u8| {
        i += 1;
        while i < b.len() && b[i] != quote {
            i += if b[i] == b'\\' { 2 } else { 1 };
        }
        (i + 1).min(b.len())
    };
    let mut i = 0;
    // The last byte that is not whitespace or in a comment: a declaration follows `{` or `;`.
    let mut last = b'{';
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = css[i + 2..].find("*/").map_or(b.len(), |end| i + 2 + end + 2);
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'"' || c == b'\'' {
            i = string_end(i, c);
            last = c;
            continue;
        }
        if c == b'-' && b.get(i + 1) == Some(&b'-') && matches!(last, b'{' | b';') {
            let end = name_end(i + 2);
            if end > i + 2 && b.get(space(end)) == Some(&b':') {
                declared.push(&css[i..end]);
            }
            (i, last) = (end, b'-');
            continue;
        }
        if c == b'@' && css.get(i..i + 9).is_some_and(|at| at.eq_ignore_ascii_case("@property")) {
            let start = space(i + "@property".len());
            if css[start..].starts_with("--") {
                let end = name_end(start + 2);
                declared.push(&css[start..end]);
                (i, last) = (end, b'-');
                continue;
            }
        }
        if (c | 0x20) == b'v' && css.get(i..i + 4).is_some_and(|s| s.eq_ignore_ascii_case("var(")) && (i == 0 || !ident(b[i - 1]))
        {
            let start = space(i + 4);
            if css[start..].starts_with("--") {
                let end = name_end(start + 2);
                match b.get(space(end)) {
                    Some(b')') => needed.push(&css[start..end]),
                    Some(b',') => {
                        // To the `)` that closes this `var(`.
                        let (mut j, mut depth) = (space(end) + 1, 1);
                        while j < b.len() && depth > 0 {
                            match b[j] {
                                b'(' => depth += 1,
                                b')' => depth -= 1,
                                q @ (b'"' | b'\'') => {
                                    j = string_end(j, q);
                                    continue;
                                }
                                _ => {}
                            }
                            j += 1;
                        }
                        (i, last) = (j, b')');
                        continue;
                    }
                    _ => {}
                }
                (i, last) = (end, b'-');
                continue;
            }
        }
        last = c;
        i += 1;
    }
    (declared, needed)
}

#[doc(hidden)]
pub mod __private {
    pub use super::vars::__private::vars_attr;
    use super::{ClassList, RENDERED};

    /// ` class="a b"`, or nothing when the list is empty.
    #[inline]
    pub fn class_attr<T: ClassList + ?Sized>(classes: &T, out: &mut String) {
        let start = out.len();
        out.push_str(" class=\"");
        let names = out.len();
        classes.write_classes(out);
        if out.len() == names {
            out.truncate(start);
        } else {
            out.remove(names);
            out.push('"');
        }
    }

    /// A component with a script or stylesheet rendered (debug builds only).
    pub fn rendered(component: &'static crate::Component) {
        RENDERED.with(|r| {
            if let Some(list) = r.borrow_mut().as_mut() {
                if !list.iter().any(|c| c.name == component.name) {
                    list.push(component);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    mod theme {
        crate::style::tokens!("src/style/tokens/tokens.resolver.json");
    }
    use theme::{direction, direction_column, gap, gap_2, Direction, Responsive, Space};

    #[test]
    fn a_stylesheet_names_the_custom_properties_it_declares_and_reads() {
        let css = "/* var(--commented) */ :root{--ink:#111;--space-1 : 4px}.a--b:hover{color:var(--ink)}
.x { padding: var( --space-1 ); margin: var(--gap, var(--nope)); content: \"var(--quoted)\"; background: VAR(--bg) }
@PROPERTY --angle { syntax: '<angle>'; inherits: false; initial-value: 0deg }
@supports (--x: 1) { .y { width: calc(var(--w) * 2); --é-ä: 1 } }";
        let (declared, needed) = custom_properties(css);
        assert_eq!(declared, ["--ink", "--space-1", "--angle", "--é-ä"]);
        assert_eq!(needed, ["--ink", "--space-1", "--bg", "--w"]);
    }

    #[test]
    fn tokens_are_typed_scales_and_utilities_are_classes_per_breakpoint() {
        assert_eq!(Space::_4.var(), "var(--space-4)");
        assert_eq!(theme::Color::Accent.value(), "#6b3fd4");
        assert_eq!(Space::ALL.len(), 3);
        let classes = (gap(Space::_2), direction(Responsive::new(Direction::Column).md(Direction::Row)));
        let html = html! { <div class={classes}></div> }.to_html();
        assert_eq!(html, r#"<div class="gap-2 direction-column md:direction-row"></div>"#);
        let css = theme::TOKENS.style.unwrap().css;
        assert!(css.starts_with(":root{--color-ink:#1d1b26;"), "{css}");
        assert!(css.contains(".md\\:direction-row{flex-direction:row}"), "{css}");
        assert!(html! { <p class={gap(Responsive::default())}></p> }.to_html() == "<p></p>", "no value, no attribute");
        // A value at every width is its own function: the class as static text.
        assert_eq!(
            html! { <p class={(gap_2(), direction_column())}></p> }.to_html(),
            r#"<p class="gap-2 direction-column"></p>"#
        );
        assert_eq!(gap_2().name(), gap(Space::_2).names().next().unwrap());
    }

    #[component(css)]
    fn BadgeList<'a>(badges: &'a [&'a str], hot: bool, extra: Option<&'a str>) -> impl Render {
        html! {
            <ul class={[List, Hot]}>
                for b in badges { <li class={(Badge, hot.then_some(Hot), extra)}>{b}</li> }
            </ul>
            <p class={None::<Class>}>"none"</p>
        }
    }

    /// A helper with the stylesheet of BadgeList.
    #[component(css = BadgeList)]
    fn BadgeCount(n: usize) -> impl Render {
        html! { <span class={Badge}>{n}</span> }
    }

    #[test]
    fn classes_are_scoped_static_where_possible_and_lists_skip_empty_values() {
        let html = BadgeList { badges: &["a", "b"], hot: false, extra: Some("x<y") }.to_html();
        assert_eq!(
            html,
            r#"<ul class="badge-list__list badge-list__hot"><li class="badge-list__badge x&lt;y">a</li><li class="badge-list__badge x&lt;y">b</li></ul><p>none</p>"#
        );
        assert_eq!(BadgeCount { n: 2 }.to_html(), r#"<span class="badge-list__badge">2</span>"#);
        let style = BadgeList.style.expect("own stylesheet");
        assert_eq!((style.name, style.file), ("badge-list", "badge_list.css"));
        assert!(BadgeList.script.is_none());
    }

    /// `Card` with `.card`: inside the markup `{Card}` is the class.
    #[component(css)]
    fn Card<'a>(title: &'a str) -> impl Render {
        html! { <section class={Card}><h2 class={Title}>{title}</h2></section> }
    }

    #[test]
    fn a_class_can_be_named_like_its_component() {
        let html = html! { <Card title="Policies" /> }.to_html();
        assert_eq!(html, r#"<section class="card__card"><h2 class="card__title">Policies</h2></section>"#);
        assert_eq!(Card.style.expect("own stylesheet").file, "card.css");
    }

    // Rendering records components in debug builds only.
    #[cfg(debug_assertions)]
    #[tokio::test]
    async fn under_corten_dev_a_page_with_an_unlisted_styled_component_fails_and_names_it() {
        use axum::{body::Body, http::Request, routing::get, Router};
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let page = |req: crate::Req| async move { crate::tests::TestPage::test_page(&req, "Badges", BadgeCount { n: 1 }) };
        let routes = || Router::new().route("/", get(page));
        let get_page = |app: Router| async move {
            let res = app.oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
            let status = res.status();
            (status, String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
        };
        let (status, body) = get_page(App::new(Routes::from(routes())).dev(true).router()).await;
        assert_eq!(status, 500);
        assert!(body.contains("renders BadgeList, which the app does not list"), "{body}");
        let (status, body) =
            get_page(App::new(Routes::from(routes())).dev(true).components(&[theme::TOKENS, BadgeList]).router()).await;
        assert_eq!(status, 200);
        let link = body.split_once(r#"<link rel="stylesheet" href=""#).expect("the stylesheet is linked").1;
        assert!(link.split_once('"').unwrap().0.ends_with("/components.css"), "{body}");
        let (status, _) = get_page(App::new(Routes::from(routes())).dev(false).router()).await;
        assert_eq!(status, 200, "outside corten dev nothing is checked");
    }

    #[derive(crate::Route)]
    #[route("/admin")]
    struct Admin;

    #[derive(crate::Route)]
    #[route("/admin/reports")]
    struct Reports;

    #[derive(crate::Route)]
    #[route("/admin/reports/{id}")]
    struct Report<'a> {
        id: &'a str,
    }

    #[tokio::test]
    async fn an_area_links_its_stylesheet_from_its_pages_only() {
        use axum::{body::Body, http::Request, routing::get, Router};
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        const REPORT: crate::Component = crate::Component {
            name: "Report",
            script: None,
            style: Some(Style { name: "report", file: "report.css", css: ".report__a{}", classes: &["a"] }),
            event: None,
            view: None,
        };
        let page = |req: crate::Req| async move { crate::tests::TestPage::test_page(&req, "Badges", BadgeCount { n: 1 }) };
        let app = |dev: bool| {
            App::new(Routes::from(Router::new().route("/", get(page)).route("/administrator", get(page))))
                .dev(dev)
                .components(&[theme::TOKENS])
                .area::<Admin>(|admin| admin.route::<Admin>(crate::Methods::reads(get(page))).components(&[BadgeList]))
                .area::<Reports>(|reports| reports.route::<Report>(crate::Methods::reads(get(page))).components(&[REPORT]))
                .router()
        };
        let get_page = |app: Router, path: &str, nav: bool| {
            let mut req = Request::get(path);
            if nav {
                req = req.header("corten", "nav");
            }
            let req = req.body(Body::empty()).unwrap();
            async move {
                let res = app.oneshot(req).await.unwrap();
                let status = res.status();
                (status, String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
            }
        };
        let links = |body: &str| -> Vec<String> {
            body.split(r#"<link rel="stylesheet" href=""#)
                .skip(1)
                .map(|rest| rest.split_once('"').unwrap().0.rsplit_once('/').unwrap().1.to_string())
                .collect()
        };

        let (status, body) = get_page(app(true), "/admin", false).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(links(&body), ["components.css", "admin.css"], "the area's stylesheet after the shared one");
        assert!(body.contains(r#"/sections/admin.css" data-corten-style>"#), "{body}");
        let (_, body) = get_page(app(true), "/admin/reports/7", false).await;
        assert_eq!(links(&body), ["components.css", "admin.css", "reports.css"], "a page links every area it is under");
        let (status, body) = get_page(app(true), "/admin/reports/7", true).await;
        assert_eq!(status, 200);
        assert!(body.starts_with("<title>Badges</title><link"), "a navigation carries the links: {body}");
        assert_eq!(links(&body), ["admin.css", "reports.css"], "and not the shared stylesheet, which the page has");

        let (status, body) = get_page(app(false), "/administrator", false).await;
        assert_eq!(
            (status, links(&body)),
            (axum::http::StatusCode::OK, vec!["components.css".to_string()]),
            "a prefix is whole segments"
        );
        let (status, body) = get_page(app(false), "/", true).await;
        assert!(status == 200 && links(&body).is_empty(), "no area, no link: {body}");

        #[cfg(debug_assertions)]
        {
            let (status, body) = get_page(app(true), "/", false).await;
            assert_eq!(status, 500);
            assert!(body.contains("/ renders BadgeList, whose stylesheet is linked from the area /admin only"), "{body}");
        }

        let twice = std::panic::catch_unwind(|| App::new(Routes::new()).area::<Admin>(|a| a).area::<Admin>(|a| a));
        assert!(twice.is_err(), "an area is given once");
    }

    #[derive(crate::Route)]
    #[route("/shop")]
    struct Shop;

    /// One screen at two addresses (`/search` and `/categories/{slug}`) is two areas listing the
    /// same components: their rules are in a file the two share, linked from their pages only.
    #[tokio::test]
    async fn areas_that_list_the_same_component_share_its_stylesheet() {
        use axum::{body::Body, http::Request, routing::get};
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        const REPORT: crate::Component = crate::Component {
            name: "Report",
            script: None,
            style: Some(Style { name: "report", file: "report.css", css: ".report__a{}", classes: &["a"] }),
            event: None,
            view: None,
        };
        let page = |req: crate::Req| async move { crate::tests::TestPage::test_page(&req, "Badges", BadgeCount { n: 1 }) };
        let app = || {
            App::new(Routes::from(axum::Router::new().route("/", get(|| async { "home" }))))
                .dev(true)
                .components(&[theme::TOKENS])
                .area::<Admin>(|admin| admin.route::<Admin>(crate::Methods::reads(get(page))).components(&[BadgeList]))
                .area::<Shop>(|shop| shop.route::<Shop>(crate::Methods::reads(get(page))).components(&[BadgeList, REPORT]))
                .router()
        };
        let get_page = |path: &str| {
            let req = Request::get(path).body(Body::empty()).unwrap();
            async move {
                let res = app().oneshot(req).await.unwrap();
                let status = res.status();
                (status, String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
            }
        };
        let links = |body: &str| -> Vec<String> {
            body.split(r#"<link rel="stylesheet" href=""#)
                .skip(1)
                .map(|rest| {
                    rest.split_once('"').unwrap().0.split_once("/sections/").map_or("components.css", |s| s.1).to_string()
                })
                .collect()
        };
        let (status, admin) = get_page("/admin").await;
        assert_eq!(status, 200, "a page of either area renders the shared component: {admin}");
        assert_eq!(links(&admin), ["components.css", "admin+shop.css"]);
        let (status, shop) = get_page("/shop").await;
        assert_eq!(status, 200, "{shop}");
        assert_eq!(links(&shop), ["components.css", "admin+shop.css", "shop.css"], "its own after the shared one");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn rendering_records_listed_components_in_debug_builds() {
        let (html, used) = collect(|| BadgeCount { n: 1 }.to_html());
        assert!(html.contains("badge-list__badge"));
        assert_eq!(used.iter().map(|c| c.name).collect::<Vec<_>>(), ["BadgeList"], "a helper reports its stylesheet's owner");
    }
}
