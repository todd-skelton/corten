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
//! - A helper in the same module can share a stylesheet: `#[component(css = OfferList)]`.
//! - The component gets a constant with its own name, `OfferList: corten::Component`; list it
//!   once in [`crate::App::components`]. Corten joins every listed stylesheet, in order, into one
//!   file at a content-versioned URL, `components.css`, linked from `<head>` after the shell's
//!   own tags. In a debug build under `corten dev`, a page that renders a component the app did
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

use std::cell::RefCell;

/// A design-token file as CSS and Rust. See [the module](self) and `examples/item-page/tokens.json`.
///
/// `tokens!("tokens.json")` (the path from the crate's root) reads:
///
/// - `scales`: named values, `{"space": {"1": "4px", "2": "8px"}, "color": {"ink": "#1d1b26"}}`.
///   Each becomes custom properties (`--space-1`) and an enum (`Space::_1`, `Color::Ink`) with
///   `var()` (`"var(--space-1)"`) and `value()`.
/// - `themes` (optional): a selector and the values it overrides,
///   `{"[data-theme=dark]": {"color": {"ink": "#eee"}}}`. A token that is not in its scale is a
///   compile error.
/// - `breakpoints` (optional): `{"sm": "40rem", "md": "48rem"}`, smallest first.
/// - `utilities` (optional): classes for one property, from a scale
///   (`{"gap": {"property": "gap", "scale": "space"}}`) or from their own values
///   (`{"direction": {"property": "flex-direction", "values": {"row": "row", "column": "column"}}}`).
///   Each is a function, `gap(Space::_2)` or `gap(Responsive::new(Space::_2).md(Space::_4))`,
///   that returns its classes (`gap-2 md:gap-4`) for `class={..}`. Inline `style=` attributes are
///   blocked by the content security policy; these are the typed way to set one value per use.
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

/// Utility classes for one value per breakpoint, from a [`tokens!`] utility: `gap(Space::_2)`.
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
    let previous = RENDERED.with(|r| r.replace(Some(Vec::new())));
    let value = render();
    let list = RENDERED.with(|r| r.replace(previous)).unwrap_or_default();
    (value, list)
}

/// Used by the code `html!` and `#[component]` generate. Not a public API.
#[doc(hidden)]
pub mod __private {
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
        crate::style::tokens!("src/style/tokens.json");
    }
    use theme::{direction, gap, Direction, Responsive, Space};

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

    // Rendering records components in debug builds only.
    #[cfg(debug_assertions)]
    #[tokio::test]
    async fn under_corten_dev_a_page_with_an_unlisted_styled_component_fails_and_names_it() {
        use axum::{body::Body, http::Request, routing::get};
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let page = |req: crate::Req| async move { req.page("Badges", BadgeCount { n: 1 }) };
        let routes = || Router::new().route("/", get(page));
        let get_page = |app: Router| async move {
            let res = app.oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
            let status = res.status();
            (status, String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
        };
        let (status, body) = get_page(App::new(routes()).dev(true).router()).await;
        assert_eq!(status, 500);
        assert!(body.contains("renders BadgeList, which the app does not list"), "{body}");
        let (status, body) = get_page(App::new(routes()).dev(true).components(&[theme::TOKENS, BadgeList]).router()).await;
        assert_eq!(status, 200);
        let link = body.split_once(r#"<link rel="stylesheet" href=""#).expect("the stylesheet is linked").1;
        assert!(link.split_once('"').unwrap().0.ends_with("/components.css"), "{body}");
        let (status, _) = get_page(App::new(routes()).dev(false).router()).await;
        assert_eq!(status, 200, "outside corten dev nothing is checked");
    }

    #[tokio::test]
    async fn a_section_links_its_stylesheet_from_its_pages_only() {
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
        let page = |req: crate::Req| async move { req.page("Badges", BadgeCount { n: 1 }) };
        let routes = || {
            Router::new()
                .route("/", get(page))
                .route("/admin", get(page))
                .route("/admin/reports/{id}", get(page))
                .route("/administrator", get(page))
        };
        let app = |dev: bool| {
            App::new(routes())
                .dev(dev)
                .components(&[theme::TOKENS])
                .section("/admin", &[BadgeList])
                .section("/admin/reports", &[REPORT])
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
        assert_eq!(links(&body), ["components.css", "admin.css"], "the section's stylesheet after the shared one");
        assert!(body.contains(r#"/sections/admin.css" data-corten-style>"#), "{body}");
        let (_, body) = get_page(app(true), "/admin/reports/7", false).await;
        assert_eq!(links(&body), ["components.css", "admin.css", "reports.css"], "a page links every section it is under");
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
        assert!(status == 200 && links(&body).is_empty(), "no section, no link: {body}");

        #[cfg(debug_assertions)]
        {
            let (status, body) = get_page(app(true), "/", false).await;
            assert_eq!(status, 500);
            assert!(
                body.contains("/ renders BadgeList, whose stylesheet is in the section /admin, which is not linked here"),
                "{body}"
            );
        }

        for bad in ["admin", "/", "/admin/", "/admin/{id}", "/a b", "/admin//x"] {
            let given = std::panic::catch_unwind(|| App::new(Router::new()).section(bad, &[])).is_err();
            assert!(given, "{bad:?} is not a section prefix");
        }
        let twice = std::panic::catch_unwind(|| App::new(Router::new()).section("/a", &[]).section("/a", &[]));
        assert!(twice.is_err(), "a section is given once");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn rendering_records_listed_components_in_debug_builds() {
        let (html, used) = collect(|| BadgeCount { n: 1 }.to_html());
        assert!(html.contains("badge-list__badge"));
        assert_eq!(used.iter().map(|c| c.name).collect::<Vec<_>>(), ["BadgeList"], "a helper reports its stylesheet's owner");
    }
}
