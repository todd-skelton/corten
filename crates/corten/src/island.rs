//! Islands: components with behavior. The markup is Rust, the behavior is a script next to it, and
//! the markup names everything the script touches.
//!
//! ```ignore
//! // src/views/offer_list.rs
//! #[island]
//! pub fn OfferList<'a>(t: Msg, offers: &'a [Offer], rest: &'a [Offer]) -> impl Render {
//!     html! {
//!         <table><tbody ref={Rows}>for o in offers { <OfferRow o={o} /> }</tbody></table>
//!         if !rest.is_empty() {
//!             <template ref={Rest}>for o in rest { <OfferRow o={o} /> }</template>
//!             <button type="button" on:click={ShowAll}>{t.show_all()}</button>
//!         }
//!     }
//! }
//! ```
//!
//! ```js
//! // src/views/offer_list.js
//! import { island } from "corten";
//!
//! export default island("offer-list", {
//!   actions: {
//!     showAll({ refs, target }) {
//!       refs.rows.append(refs.rest.content);
//!       refs.rest.remove();
//!       target.remove();
//!     },
//!   },
//! });
//! ```
//!
//! - The component renders inside its tag, `<offer-list>` (its name in kebab-case), and works
//!   without the script: the server rendered everything the page shows first.
//! - `ref={Rows}` names an element (`refs.rows`, or `all.rows` for every one); `on:click={ShowAll}`
//!   runs the action `showAll` on a click. Names are UpperCamelCase in Rust and camelCase in the
//!   script. They exist only inside this component: `html!` prints no other name, and no string.
//! - Parameters marked `#[prop]` reach the script as `props`, as JSON (feature `json`).
//! - The script is `offer_list.js` (the name in snake_case) in the component's directory, compiled
//!   into the binary. List the island once, `App::islands(&[OfferList::ISLAND])`, and Corten serves
//!   it at a versioned URL and preloads it on pages that render the tag.
//! - `corten::types::Types::islands` writes what the markup declares, so `tsc --checkJs` fails
//!   when the script reads a ref the markup does not have, calls an element method the element
//!   does not have (`refs.rest` is an `HTMLTemplateElement`), or misses or misspells an action.
//!
//! ```
//! use corten::prelude::*;
//!
//! /// Its script is price_board.js, next to this file.
//! #[island]
//! fn PriceBoard(prices: &[u32]) -> impl Render {
//!     html! {
//!         <ul ref={Items}>for p in prices { <li>{p}</li> }</ul>
//!         <button type="button" on:click={Pick} on:keydown={Pick}>"Pick"</button>
//!     }
//! }
//!
//! let html = PriceBoard { prices: &[2, 1] }.to_html();
//! assert_eq!(
//!     html,
//!     r#"<price-board><ul data-ref="items"><li>2</li><li>1</li></ul><button type="button" data-on="click:pick keydown:pick">Pick</button></price-board>"#
//! );
//! assert_eq!(PriceBoard::ISLAND.tag, "price-board");
//! assert_eq!(PriceBoard::ISLAND.actions[0].events, &["click", "keydown"]);
//! ```
//!
//! A ref or action is a name, never a string:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! let view = html! { <ul ref={"items"}></ul> };
//! ```
//!
//! and `data-ref` and `data-on` are never written by hand:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! let view = html! { <button data-on="click:sort"></button> };
//! ```
//!
//! A name belongs to its island; another component cannot use it:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[island]
//! fn PriceBoard(prices: &[u32]) -> impl Render {
//!     html! { <ul ref={Items}>for p in prices { <li>{p}</li> }</ul><button on:click={Pick}>"Pick"</button> }
//! }
//!
//! #[component]
//! fn Other() -> impl Render {
//!     html! { <ul ref={Items}></ul> }
//! }
//! ```
//!
//! and one name is either a ref or an action:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[island]
//! fn PriceBoard(prices: &[u32]) -> impl Render {
//!     html! { <ul ref={Pick}></ul><button on:click={Pick}>"Pick"</button> }
//! }
//! ```

/// A component with behavior in a script next to it. See [the module](self).
pub use corten_macros::island;

/// What the server knows about one island, from `#[island]`: `Component::ISLAND`.
#[derive(Clone, Copy, Debug)]
pub struct Island {
    /// The custom element's name: `"offer-list"`.
    pub tag: &'static str,
    /// The script's source.
    pub script: &'static str,
    /// The script's file name, next to the component: `"offer_list.js"`.
    pub file: &'static str,
    /// The elements the markup names with `ref={..}`.
    pub refs: &'static [Name],
    /// The actions the markup runs with `on:event={..}`.
    pub actions: &'static [Name],
    /// The parameters marked `#[prop]`.
    pub props: &'static [Prop],
}

/// A ref or an action, as the script sees it.
#[derive(Clone, Copy, Debug)]
pub struct Name {
    /// `"showAll"`.
    pub name: &'static str,
    /// The elements it is on: `["button"]`.
    pub tags: &'static [&'static str],
    /// For an action, its events: `["click"]`.
    pub events: &'static [&'static str],
}

/// A `#[prop]` parameter.
#[derive(Clone, Copy, Debug)]
pub struct Prop {
    /// The parameter's name, the key in `props`.
    pub name: &'static str,
    #[doc(hidden)]
    pub ts: __private::TsType,
}

/// Used by the code `#[island]` and `html!` generate. Not a public API.
#[doc(hidden)]
pub mod __private {
    /// A ref name: an enum `#[island]` makes from the component's `ref={..}` values.
    #[diagnostic::on_unimplemented(
        message = "ref={{..}} takes a name, like ref={{Rows}}, inside the #[island] component whose script uses it",
        label = "not a ref of this island"
    )]
    pub trait IslandRef {
        fn name(&self) -> &'static str;
    }

    /// An action name: an enum `#[island]` makes from the component's `on:event={..}` values.
    #[diagnostic::on_unimplemented(
        message = "on:event={{..}} takes an action name, like on:click={{ShowAll}}, inside the #[island] component whose script has it",
        label = "not an action of this island"
    )]
    pub trait IslandAction {
        fn name(&self) -> &'static str;
    }

    pub fn ref_name<R: IslandRef>(r: &R) -> &'static str {
        r.name()
    }

    pub fn action_name<A: IslandAction>(a: &A) -> &'static str {
        a.name()
    }

    /// `html!` prints a bare name as static markup; these keep its type checked.
    #[inline(always)]
    pub fn check_ref<R: IslandRef>(_: &R) {}

    #[inline(always)]
    pub fn check_action<A: IslandAction>(_: &A) {}

    /// A `#[prop]` value, written as JSON.
    #[cfg(feature = "json")]
    pub trait Json {
        fn write_json(&self, out: &mut String);
    }

    #[cfg(feature = "json")]
    impl<T: serde::Serialize + ?Sized> Json for T {
        fn write_json(&self, out: &mut String) {
            serde_json::to_writer(Escaped(out), self).expect("island props serialize");
        }
    }

    /// Without feature `json`, nothing is a prop.
    #[cfg(not(feature = "json"))]
    #[diagnostic::on_unimplemented(
        message = "#[prop] values are sent as JSON: enable corten's `json` feature and derive serde::Serialize",
        label = "a #[prop]"
    )]
    pub trait Json {
        fn write_json(&self, out: &mut String);
    }

    /// Streams serde_json's output into the page, writing `<` as `\u003c` (it only occurs inside
    /// JSON strings), so no value can close the script element.
    #[cfg(feature = "json")]
    struct Escaped<'a>(&'a mut String);

    #[cfg(feature = "json")]
    impl std::io::Write for Escaped<'_> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            // serde_json writes whole UTF-8 sequences per call.
            let text = std::str::from_utf8(buf).map_err(std::io::Error::other)?;
            for (i, part) in text.split('<').enumerate() {
                if i > 0 {
                    self.0.push_str("\\u003c");
                }
                self.0.push_str(part);
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// `<script type="application/json" data-props>{"name": value, ...}</script>`, which the
    /// runtime reads into `props`.
    pub fn props(out: &mut String, props: &[(&str, &dyn Json)]) {
        out.push_str(r#"<script type="application/json" data-props>{"#);
        for (i, (name, value)) in props.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('"');
            out.push_str(name);
            out.push_str("\":");
            value.write_json(out);
        }
        out.push_str("}</script>");
    }

    /// How `corten::types` describes a prop's type.
    #[cfg(feature = "types")]
    pub type TsType = fn(&mut schemars::SchemaGenerator) -> serde_json::Value;
    #[cfg(not(feature = "types"))]
    pub type TsType = ();

    #[cfg(feature = "types")]
    pub const fn ts<T: schemars::JsonSchema + ?Sized>() -> TsType {
        fn schema<T: schemars::JsonSchema + ?Sized>(generator: &mut schemars::SchemaGenerator) -> serde_json::Value {
            generator.subschema_for::<T>().to_value()
        }
        schema::<T>
    }

    #[cfg(not(feature = "types"))]
    pub const fn ts<T: ?Sized>() -> TsType {}
}

#[cfg(test)]
mod tests;
