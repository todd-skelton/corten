//! Typed `data-ref` names: the elements an island finds, named once in Rust.
//!
//! ```ignore
//! #[derive(Refs)]
//! #[island("listing-sort")]
//! pub enum ListingSortRef { Toggle, Rows }
//!
//! html! {
//!     <listing-sort>
//!         <button data-ref={ListingSortRef::Toggle}>"Sort"</button>     // data-ref="toggle"
//!         <tbody data-ref={ListingSortRef::Rows}>...</tbody>
//!     </listing-sort>
//! }
//! ```
//!
//! Variants print in kebab-case (`SortButton` is `sort-button`). Both ends are checked:
//!
//! - In `html!`, `data-ref={ListingSortRef::Rows}` inside `<listing-sort>` compiles only when
//!   `ListingSortRef` is that island's enum, so a ref cannot land in the wrong island. A misspelled
//!   variant does not compile at all.
//! - `corten::types::Types::refs::<ListingSortRef>()` writes `export type ListingSortRef = "toggle" | "rows"`,
//!   and the island declares it: `@extends {Island<unknown, ListingSortRef>}`. `this.ref("row")` and
//!   `this.on("click", "togle", ..)` are then type errors in `tsc --checkJs`, and an island that declares
//!   no refs can use none.
//!
//! ```
//! use corten::prelude::*;
//!
//! #[derive(Refs)]
//! #[island("offer-list")]
//! enum OfferListRef { Rows, More }
//!
//! let view = html! { <offer-list><button data-ref={OfferListRef::More}>"More"</button></offer-list> };
//! assert_eq!(view.to_html(), r#"<offer-list><button data-ref="more">More</button></offer-list>"#);
//! ```
//!
//! A ref of another island does not compile:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[derive(Refs)]
//! #[island("offer-list")]
//! enum OfferListRef { Rows, More }
//!
//! let view = html! { <listing-sort><tbody data-ref={OfferListRef::Rows}></tbody></listing-sort> };
//! ```
//!
//! Nor does a name the island does not have:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[derive(Refs)]
//! #[island("offer-list")]
//! enum OfferListRef { Rows, More }
//!
//! let view = html! { <offer-list><tbody data-ref={OfferListRef::Row}></tbody></offer-list> };
//! ```

/// Derives [`Refs`](trait@Refs) for an enum of unit variants: `#[derive(Refs)] #[island("tag")]`.
pub use corten_macros::Refs;

/// The `data-ref` names of one island.
pub trait Refs {
    /// The island's tag: `"listing-sort"`.
    const ISLAND: &'static str;
    /// The enum's name, used for the TypeScript union.
    const TYPE: &'static str;
    /// Every name, in declaration order.
    const NAMES: &'static [&'static str];
    /// This ref's name, as `data-ref` prints it.
    fn name(&self) -> &'static str;
}

/// Used by the code `html!` generates. Not a public API.
#[doc(hidden)]
pub mod __private {
    use super::Refs;

    const fn same(a: &str, b: &str) -> bool {
        let (a, b) = (a.as_bytes(), b.as_bytes());
        if a.len() != b.len() {
            return false;
        }
        let mut i = 0;
        while i < a.len() {
            if a[i] != b[i] {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Evaluated in a constant: fails the build unless `R` belongs to one of `islands`.
    pub const fn check_ref<R: Refs>(islands: &[&str]) {
        let mut i = 0;
        while i < islands.len() {
            if same(islands[i], R::ISLAND) {
                return;
            }
            i += 1;
        }
        panic!(
            "this data-ref belongs to another island: its Refs enum names a different #[island(..)] than the elements around it"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::Refs as _;
    use crate::{self as corten, view::Render};

    #[derive(corten::island::Refs, Clone, Copy)]
    #[island("listing-sort")]
    enum ListingSortRef {
        Toggle,
        SortRows,
    }

    #[test]
    fn refs_print_kebab_case_and_check_their_island() {
        assert_eq!(ListingSortRef::NAMES, &["toggle", "sort-rows"]);
        assert_eq!(ListingSortRef::Toggle.name(), "toggle");
        let view = crate::html! {
            <listing-sort><div><button data-ref={ListingSortRef::Toggle}></button></div><tbody data-ref={ListingSortRef::SortRows}></tbody></listing-sort>
        };
        assert_eq!(
            view.to_html(),
            r#"<listing-sort><div><button data-ref="toggle"></button></div><tbody data-ref="sort-rows"></tbody></listing-sort>"#
        );
    }
}
