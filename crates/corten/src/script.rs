//! Behavior in the browser: handlers, plain JavaScript functions that the markup calls with the
//! elements and values they use.
//!
//! ```
//! use corten::prelude::*;
//!
//! // src/views/cart_line.rs
//! #[component(js)]
//! pub fn CartLine<'a>(name: &'a str, quantity: u32) -> impl Render {
//!     let (save, dialog) = refs();
//!     html! {
//!         <li>
//!             <input value={quantity} on:input={js::enable(save)}>
//!             <button ref={save} disabled>"Save"</button>
//!             <button on:click={js::confirm(event, dialog, name)}>"Remove"</button>
//!             <dialog ref={dialog}>"Remove it?"</dialog>
//!         </li>
//!     }
//! }
//!
//! let html = CartLine { name: "Lamp", quantity: 2 }.to_html();
//! assert!(html.contains(r#"<button data-on="click:cart_line.confirm($e,@dialog,&quot;Lamp&quot;)">"#));
//! ```
//!
//! ```js
//! // src/views/cart_line.js
//! /** @param {HTMLButtonElement} save */
//! export function enable(save) {
//!   save.disabled = false;
//! }
//!
//! /** @param {MouseEvent} event @param {HTMLDialogElement} dialog @param {string} name */
//! export function confirm(event, dialog, name) {
//!   event.preventDefault();
//!   dialog.dataset.name = name;
//!   dialog.showModal();
//! }
//! ```
//!
//! - `#[component(js)]` reads the script next to the component (`cart_line.js`, its name in
//!   snake_case) when the crate compiles, and gives the component a module `js` with one Rust
//!   function per `export function`. A call checks the arguments against the handler's JSDoc:
//!   `js::enable(save)` compiles only if `save` is a `<button>` (or the JSDoc says a wider
//!   type), and a missing or misspelled handler does not compile.
//! - `let (save, dialog) = refs();` names elements. Each ref's element type is inferred from the
//!   tag it is put on (`ref={save}` on a `<button>`), and a ref put on elements in a `for` loop is
//!   every one of them, an array in the script. A component with refs has one root element, which
//!   marks where its refs end.
//! - Inside `on:...={..}`, four names are the browser's: `event`, `target` (the element with the
//!   `on:`), `signal` (aborts when that element leaves the page) and `scope` (the root element of
//!   the component that made the handler). Their types come from the event and the tag.
//! - Other arguments are values, sent as JSON: strings, numbers, booleans, typed routes (their
//!   URL), and with feature `json` anything `Serialize`.
//! - `on:connect` runs when the element joins the page (its `signal` aborts when it leaves).
//! - A handler is a value: a component takes one as a prop (`on_remove: Handler`) and puts it on
//!   an element (`on:click={on_remove}`); the parent sets it with `on:remove={js::submit(form)}`.
//! - [`script!`] declares a module shared by several components, and [`event`] a typed DOM
//!   event a script dispatches and markup listens for (`on:RowToggled={..}`).
//!
//! The browser gets plain HTML: `data-ref="save"` and `data-on="click:cart_line.enable(@save)"`.
//! One listener per event type on the document runs the handlers; nothing runs per element until
//! an event reaches it.
//!
//! What does not compile:
//!
//! A ref on an element the handler does not take (`enable` takes a `<button>`):
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[component(js)]
//! fn CartLine<'a>(name: &'a str, quantity: u32) -> impl Render {
//!     let save = refs();
//!     html! { <li><input ref={save} on:input={js::enable(save)}></li> }
//! }
//! ```
//!
//! A handler the script does not export:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[component(js)]
//! fn CartLine<'a>(name: &'a str, quantity: u32) -> impl Render {
//!     let save = refs();
//!     html! { <li><button ref={save} on:click={js::enabel(save)}></button></li> }
//! }
//! ```
//!
//! A value of the wrong type (`name` is a `string`):
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[component(js)]
//! fn CartLine<'a>(name: &'a str, quantity: u32) -> impl Render {
//!     let dialog = refs();
//!     html! { <li><button on:click={js::confirm(event, dialog, quantity)}></button><dialog ref={dialog}></dialog></li> }
//! }
//! ```
//!
//! A ref or handler written as text:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[component(js)]
//! fn CartLine<'a>(name: &'a str, quantity: u32) -> impl Render {
//!     html! { <li><button ref="save"></button></li> }
//! }
//! ```
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! #[component(js)]
//! fn CartLine<'a>(name: &'a str, quantity: u32) -> impl Render {
//!     html! { <li><button data-on="click:cart_line.enable(@save)"></button></li> }
//! }
//! ```
//!
//! and refs outside a component's body:
//!
//! ```compile_fail
//! use corten::prelude::*;
//!
//! fn helper() -> Ref {
//!     refs()
//! }
//! ```

use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
};

pub use corten_macros::{event, script};

use crate::Component;

/// A script module: the handlers of a `#[component(js)]` or a [`script!`] module.
#[derive(Clone, Copy, Debug)]
pub struct Script {
    /// The module's name in the page, its file's stem: `"cart_line"`.
    pub name: &'static str,
    /// The file, next to the component: `"cart_line.js"`.
    pub file: &'static str,
    /// Its source, compiled into the binary.
    pub source: &'static str,
    /// The functions it exports, which the markup may call.
    pub handlers: &'static [&'static str],
}

/// A typed DOM event, from [`event`]: its name in the page and its fields, for the class the
/// browser imports from `corten/events`.
#[derive(Clone, Copy, Debug)]
pub struct EventDef {
    /// The Rust name, also the JavaScript class: `"RowToggled"`.
    pub class: &'static str,
    /// The event's type in the page: `"row-toggled"`.
    pub name: &'static str,
    /// The fields, in order.
    pub fields: &'static [Field],
}

/// A field of an [`EventDef`].
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub name: &'static str,
    #[doc(hidden)]
    pub ts: crate::script::__private::TsType,
}

/// A Rust type that is a DOM event: [`event`] implements it.
pub trait DomEvent {
    /// Its type in the page, `"row-toggled"`.
    const NAME: &'static str;
}

/// A named element of a component: `let (rows, rest) = refs();`. `T` is the element's DOM
/// interface, inferred from the tag it is put on. Pass it to a handler (a parameter typed `T` gets
/// the first element with it, `T[]` every one), or to a child component that puts it on one of
/// its elements.
pub struct Ref<T = el::HTMLElement> {
    scope: &'static str,
    name: &'static str,
    kind: PhantomData<fn() -> T>,
}

impl<T> Clone for Ref<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Ref<T> {}

impl<T> std::fmt::Debug for Ref<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ref({}/{})", self.scope, self.name)
    }
}

/// `let (rows, rest) = refs();` at the top of a component's body names the elements its script
/// uses. `#[component]` replaces the call with each ref; anywhere else it does not compile.
pub fn refs<T: __private::Refs>() -> T {
    unreachable!("#[component] replaces refs()")
}

/// A handler call the markup puts on an element: what `js::enable(save)` returns. Components
/// take one as a prop with the type [`Handler`].
pub trait Bind {
    #[doc(hidden)]
    fn write_bind(&self, out: &mut String);
    /// False for a handler that is not there (`None`): its `on:` is left out.
    #[doc(hidden)]
    fn present(&self) -> bool {
        true
    }
}

/// A handler passed to a component: `#[component] fn Confirm(on_confirm: Handler) -> impl Render`,
/// set by the parent with `<Confirm on:confirm={js::submit(form)} />`.
pub type Handler<'a> = &'a dyn Bind;

impl<T: Bind + ?Sized> Bind for &T {
    fn write_bind(&self, out: &mut String) {
        (**self).write_bind(out)
    }
    fn present(&self) -> bool {
        (**self).present()
    }
}

impl<T: Bind> Bind for Option<T> {
    fn write_bind(&self, out: &mut String) {
        if let Some(bind) = self {
            bind.write_bind(out)
        }
    }
    fn present(&self) -> bool {
        self.as_ref().is_some_and(Bind::present)
    }
}

/// An argument a handler parameter of type `P` accepts. `P` is what the handler's JSDoc says:
/// an element interface (`el::HTMLButtonElement`), `ts::Array<..>` for an array of them, an
/// event interface (`ev::KeyboardEvent`), or one of [`ts`]'s.
#[diagnostic::on_unimplemented(
    message = "this argument does not match the handler's JSDoc type {P}",
    label = "the handler's JSDoc says {P}",
    note = "refs are typed by the tag they are on, event and target by the on: attribute, values by their Rust type"
)]
pub trait Arg<P> {
    #[doc(hidden)]
    fn write_arg(&self, ctx: &__private::Ctx, out: &mut String);
}

/// DOM element interfaces, named as in TypeScript's lib.dom.
#[allow(clippy::upper_case_acronyms)]
pub mod el {
    /// An element interface.
    pub trait Elem {
        /// Its name in TypeScript: `"HTMLButtonElement"`.
        const TS: &'static str;
    }

    /// `Self` is a `P`: every `HTMLButtonElement` is an `HTMLElement` and an `Element`.
    #[diagnostic::on_unimplemented(
        message = "the handler takes an {P}, and this is an {Self}",
        label = "an {Self}, from the tag its ref is on",
        note = "put the ref on an element the handler takes, or widen the JSDoc type (HTMLElement takes any HTML element)"
    )]
    pub trait SubOf<P> {}

    macro_rules! elements {
        ($name:ident: $($parent:ident),+) => {
            #[doc = concat!("`", stringify!($name), "`.")]
            pub enum $name {}
            impl Elem for $name {
                const TS: &'static str = stringify!($name);
            }
            $(impl SubOf<$parent> for $name {})+
        };
    }

    elements!(Element: Element);
    elements!(HTMLElement: HTMLElement, Element);
    elements!(HTMLMediaElement: HTMLMediaElement, Element, HTMLElement);
    elements!(SVGElement: SVGElement, Element);
    elements!(SVGSVGElement: SVGSVGElement, Element, SVGElement);
    elements!(HTMLAnchorElement: HTMLAnchorElement, Element, HTMLElement);
    elements!(HTMLAreaElement: HTMLAreaElement, Element, HTMLElement);
    elements!(HTMLAudioElement: HTMLAudioElement, Element, HTMLElement, HTMLMediaElement);
    elements!(HTMLBRElement: HTMLBRElement, Element, HTMLElement);
    elements!(HTMLBaseElement: HTMLBaseElement, Element, HTMLElement);
    elements!(HTMLBodyElement: HTMLBodyElement, Element, HTMLElement);
    elements!(HTMLButtonElement: HTMLButtonElement, Element, HTMLElement);
    elements!(HTMLCanvasElement: HTMLCanvasElement, Element, HTMLElement);
    elements!(HTMLDataElement: HTMLDataElement, Element, HTMLElement);
    elements!(HTMLDataListElement: HTMLDataListElement, Element, HTMLElement);
    elements!(HTMLDetailsElement: HTMLDetailsElement, Element, HTMLElement);
    elements!(HTMLDialogElement: HTMLDialogElement, Element, HTMLElement);
    elements!(HTMLDivElement: HTMLDivElement, Element, HTMLElement);
    elements!(HTMLDListElement: HTMLDListElement, Element, HTMLElement);
    elements!(HTMLEmbedElement: HTMLEmbedElement, Element, HTMLElement);
    elements!(HTMLFieldSetElement: HTMLFieldSetElement, Element, HTMLElement);
    elements!(HTMLFormElement: HTMLFormElement, Element, HTMLElement);
    elements!(HTMLHeadingElement: HTMLHeadingElement, Element, HTMLElement);
    elements!(HTMLHeadElement: HTMLHeadElement, Element, HTMLElement);
    elements!(HTMLHRElement: HTMLHRElement, Element, HTMLElement);
    elements!(HTMLHtmlElement: HTMLHtmlElement, Element, HTMLElement);
    elements!(HTMLIFrameElement: HTMLIFrameElement, Element, HTMLElement);
    elements!(HTMLImageElement: HTMLImageElement, Element, HTMLElement);
    elements!(HTMLInputElement: HTMLInputElement, Element, HTMLElement);
    elements!(HTMLLabelElement: HTMLLabelElement, Element, HTMLElement);
    elements!(HTMLLegendElement: HTMLLegendElement, Element, HTMLElement);
    elements!(HTMLLIElement: HTMLLIElement, Element, HTMLElement);
    elements!(HTMLLinkElement: HTMLLinkElement, Element, HTMLElement);
    elements!(HTMLMapElement: HTMLMapElement, Element, HTMLElement);
    elements!(HTMLMenuElement: HTMLMenuElement, Element, HTMLElement);
    elements!(HTMLMetaElement: HTMLMetaElement, Element, HTMLElement);
    elements!(HTMLMeterElement: HTMLMeterElement, Element, HTMLElement);
    elements!(HTMLModElement: HTMLModElement, Element, HTMLElement);
    elements!(HTMLObjectElement: HTMLObjectElement, Element, HTMLElement);
    elements!(HTMLOListElement: HTMLOListElement, Element, HTMLElement);
    elements!(HTMLOptGroupElement: HTMLOptGroupElement, Element, HTMLElement);
    elements!(HTMLOptionElement: HTMLOptionElement, Element, HTMLElement);
    elements!(HTMLOutputElement: HTMLOutputElement, Element, HTMLElement);
    elements!(HTMLParagraphElement: HTMLParagraphElement, Element, HTMLElement);
    elements!(HTMLPictureElement: HTMLPictureElement, Element, HTMLElement);
    elements!(HTMLPreElement: HTMLPreElement, Element, HTMLElement);
    elements!(HTMLProgressElement: HTMLProgressElement, Element, HTMLElement);
    elements!(HTMLQuoteElement: HTMLQuoteElement, Element, HTMLElement);
    elements!(HTMLScriptElement: HTMLScriptElement, Element, HTMLElement);
    elements!(HTMLSelectElement: HTMLSelectElement, Element, HTMLElement);
    elements!(HTMLSlotElement: HTMLSlotElement, Element, HTMLElement);
    elements!(HTMLSourceElement: HTMLSourceElement, Element, HTMLElement);
    elements!(HTMLSpanElement: HTMLSpanElement, Element, HTMLElement);
    elements!(HTMLStyleElement: HTMLStyleElement, Element, HTMLElement);
    elements!(HTMLTableCaptionElement: HTMLTableCaptionElement, Element, HTMLElement);
    elements!(HTMLTableCellElement: HTMLTableCellElement, Element, HTMLElement);
    elements!(HTMLTableColElement: HTMLTableColElement, Element, HTMLElement);
    elements!(HTMLTableElement: HTMLTableElement, Element, HTMLElement);
    elements!(HTMLTableRowElement: HTMLTableRowElement, Element, HTMLElement);
    elements!(HTMLTableSectionElement: HTMLTableSectionElement, Element, HTMLElement);
    elements!(HTMLTemplateElement: HTMLTemplateElement, Element, HTMLElement);
    elements!(HTMLTextAreaElement: HTMLTextAreaElement, Element, HTMLElement);
    elements!(HTMLTimeElement: HTMLTimeElement, Element, HTMLElement);
    elements!(HTMLTitleElement: HTMLTitleElement, Element, HTMLElement);
    elements!(HTMLTrackElement: HTMLTrackElement, Element, HTMLElement);
    elements!(HTMLUListElement: HTMLUListElement, Element, HTMLElement);
    elements!(HTMLVideoElement: HTMLVideoElement, Element, HTMLElement, HTMLMediaElement);
}

/// DOM event interfaces, named as in TypeScript's lib.dom.
pub mod ev {
    /// An event interface.
    pub trait EventType {
        const TS: &'static str;
    }

    /// `Self` is a `P`: every `PointerEvent` is a `MouseEvent`, a `UIEvent` and an `Event`.
    #[diagnostic::on_unimplemented(
        message = "the handler takes a {P}, and this on: attribute's event is a {Self}",
        label = "a {Self}",
        note = "the event's type comes from the on: attribute's name (on:click is a MouseEvent); widen the JSDoc type to Event to take any"
    )]
    pub trait SubOf<P> {}

    macro_rules! events {
        ($name:ident: $($parent:ident),+) => {
            #[doc = concat!("`", stringify!($name), "`.")]
            pub enum $name {}
            impl EventType for $name {
                const TS: &'static str = stringify!($name);
            }
            $(impl SubOf<$parent> for $name {})+
        };
    }

    events!(Event: Event);
    events!(UIEvent: UIEvent, Event);
    events!(MouseEvent: MouseEvent, UIEvent, Event);
    events!(PointerEvent: PointerEvent, MouseEvent, UIEvent, Event);
    events!(WheelEvent: WheelEvent, MouseEvent, UIEvent, Event);
    events!(DragEvent: DragEvent, MouseEvent, UIEvent, Event);
    events!(KeyboardEvent: KeyboardEvent, UIEvent, Event);
    events!(InputEvent: InputEvent, UIEvent, Event);
    events!(FocusEvent: FocusEvent, UIEvent, Event);
    events!(CompositionEvent: CompositionEvent, UIEvent, Event);
    events!(TouchEvent: TouchEvent, UIEvent, Event);
    events!(SubmitEvent: SubmitEvent, Event);
    events!(ToggleEvent: ToggleEvent, Event);
    events!(AnimationEvent: AnimationEvent, Event);
    events!(TransitionEvent: TransitionEvent, Event);
    events!(ClipboardEvent: ClipboardEvent, Event);
    events!(ErrorEvent: ErrorEvent, Event);
    events!(CustomEvent: CustomEvent, Event);

    /// A typed event from `#[event]` is an `Event`.
    impl<E: super::DomEvent> SubOf<Event> for E {}
}

/// JSDoc types that are not elements or events.
pub mod ts {
    /// `string`: `&str`, `String`, a typed route (its URL).
    pub enum String {}
    /// `number`.
    pub enum Number {}
    /// `boolean`.
    pub enum Boolean {}
    /// `AbortSignal`: the `signal` keyword.
    pub enum AbortSignal {}
    /// Any other type: a JSON value (feature `json`), or `event` for a typed event.
    pub enum Any {}
    /// An array of elements: `HTMLTableRowElement[]`, a ref put on elements in a loop.
    pub struct Array<T>(std::marker::PhantomData<T>);
}

thread_local! {
    /// The component whose markup is rendering (one with refs), for refs and handlers.
    static SCOPE: Cell<Option<&'static str>> = const { Cell::new(None) };
    /// The script modules the page's handlers use, while a page renders.
    static MODULES: RefCell<Option<Vec<&'static str>>> = const { RefCell::new(None) };
}

/// Runs `render` and returns the script modules its handlers use, in first-use order.
pub(crate) fn collect<T>(render: impl FnOnce() -> T) -> (T, Vec<&'static str>) {
    let previous = MODULES.with(|m| m.replace(Some(Vec::new())));
    let value = render();
    let list = MODULES.with(|m| m.replace(previous)).unwrap_or_default();
    (value, list)
}

/// Used by the code `#[component]`, `html!`, `script!` and `#[event]` generate. Not a public API.
#[doc(hidden)]
pub mod __private {
    use super::*;

    /// Where an argument is written: the component rendering and the one that made the handler.
    pub struct Ctx {
        pub current: Option<&'static str>,
        pub home: Option<&'static str>,
    }

    /// An argument, written with the parameter type it was checked against.
    pub trait WriteArg {
        fn write_arg(&self, ctx: &Ctx, out: &mut String);
    }

    /// An argument and its parameter's type.
    pub struct Typed<A, P>(A, PhantomData<fn() -> P>);

    impl<A: Arg<P>, P> Typed<A, P> {
        #[inline]
        pub fn new(arg: A) -> Self {
            Typed(arg, PhantomData)
        }
    }

    impl<A: Arg<P>, P> WriteArg for Typed<A, P> {
        #[inline]
        fn write_arg(&self, ctx: &Ctx, out: &mut String) {
            self.0.write_arg(ctx, out)
        }
    }

    /// Only `#[component]` makes refs.
    #[diagnostic::on_unimplemented(
        message = "refs() names a component's elements: write `let (rows, rest) = refs();` at the top of a #[component]'s body",
        label = "outside a component's top level"
    )]
    pub trait Refs {}

    /// The arguments of a call, a tuple.
    pub trait Args {
        fn write_args(&self, ctx: &Ctx, out: &mut String);
    }

    macro_rules! args {
        ($(($($name:ident),*)),*) => {$(
            impl<$($name: WriteArg),*> Args for ($($name,)*) {
                #[allow(non_snake_case, unused_variables, unused_assignments, unused_mut)]
                fn write_args(&self, ctx: &Ctx, out: &mut String) {
                    let ($($name,)*) = self;
                    let mut first = true;
                    $(
                        if !first { out.push(','); }
                        first = false;
                        $name.write_arg(ctx, out);
                    )*
                }
            }
        )*};
    }
    args!(
        (),
        (A),
        (A, B),
        (A, B, C),
        (A, B, C, D),
        (A, B, C, D, E),
        (A, B, C, D, E, F),
        (A, B, C, D, E, F, G),
        (A, B, C, D, E, F, G, H)
    );

    /// A call of one handler: `module.name(args)`.
    pub struct Call<A> {
        module: &'static str,
        name: &'static str,
        home: Option<&'static str>,
        /// A shared module's own component, marked rendered for the dev check (debug builds).
        #[cfg_attr(not(debug_assertions), allow(dead_code))]
        owner: Option<&'static Component>,
        args: A,
    }

    impl<A> Call<A> {
        #[inline]
        pub fn new(module: &'static str, name: &'static str, owner: Option<&'static Component>, args: A) -> Self {
            Call { module, name, home: SCOPE.with(Cell::get), owner, args }
        }
    }

    impl<A: Args> Bind for Call<A> {
        fn write_bind(&self, out: &mut String) {
            MODULES.with(|m| {
                if let Some(list) = m.borrow_mut().as_mut() {
                    if !list.contains(&self.module) {
                        list.push(self.module);
                    }
                }
            });
            #[cfg(debug_assertions)]
            if let Some(owner) = self.owner {
                crate::style::__private::rendered(owner);
            }
            out.push_str(self.module);
            out.push('.');
            out.push_str(self.name);
            out.push('(');
            self.args.write_args(&Ctx { current: SCOPE.with(Cell::get), home: self.home }, out);
            out.push(')');
        }
    }

    /// Marks the component whose markup renders while `f` runs.
    #[inline]
    pub fn scoped<T>(scope: &'static str, f: impl FnOnce() -> T) -> T {
        struct Restore(Option<&'static str>);
        impl Drop for Restore {
            fn drop(&mut self) {
                SCOPE.with(|s| s.set(self.0));
            }
        }
        let _restore = Restore(SCOPE.with(|s| s.replace(Some(scope))));
        f()
    }

    /// `let (rows, rest) = refs();`: each name, made by `#[component]`.
    #[inline]
    pub const fn new_ref<T>(scope: &'static str, name: &'static str) -> Ref<T> {
        Ref { scope, name, kind: PhantomData }
    }

    /// A ref of another component's markup is written with its component: `offer-list/rows`.
    fn qualified(out: &mut String, owner: &'static str, current: Option<&'static str>) {
        if current != Some(owner) {
            out.push_str(owner);
            out.push('/');
        }
    }

    /// ` data-ref="rows"` on an element of interface `E`, which makes `rows` a `Ref<E>`.
    #[inline]
    pub fn ref_attr<E>(r: &Ref<E>, out: &mut String) {
        out.push_str(" data-ref=\"");
        qualified(out, r.scope, SCOPE.with(Cell::get));
        out.push_str(r.name);
        out.push('"');
    }

    /// `ref={trigger as HTMLElement}` on an element of interface `E`: `trigger` is a `Ref<W>`,
    /// and every `E` it is on must be a `W`.
    #[inline]
    pub fn ref_attr_as<E: el::SubOf<W>, W>(r: &Ref<W>, out: &mut String) {
        ref_attr(r, out)
    }

    /// ` data-on="click:m.f(..) input:m.g(..)"`, or nothing when no handler is there.
    pub fn data_on(out: &mut String, binds: &[(&str, &dyn Bind)]) {
        let mut first = true;
        for (event, bind) in binds {
            if !bind.present() {
                continue;
            }
            out.push_str(if first { " data-on=\"" } else { " " });
            first = false;
            out.push_str(event);
            out.push(':');
            bind.write_bind(out);
        }
        if !first {
            out.push('"');
        }
    }

    /// `event` in an `on:` handler: the event, of type `E`.
    pub struct EventKw<E>(PhantomData<E>);
    impl<E> EventKw<E> {
        pub const NEW: Self = EventKw(PhantomData);
    }
    /// `target`: the element with the `on:`, of interface `T`.
    pub struct TargetKw<T>(PhantomData<T>);
    impl<T> TargetKw<T> {
        pub const NEW: Self = TargetKw(PhantomData);
    }
    /// `signal`: aborts when the element with the `on:` leaves the page.
    pub struct SignalKw;
    /// `scope`: the root element of the component that made the handler.
    pub struct ScopeKw;

    /// What a parameter type gets for a ref: the first element (`@`) or every one (`*`).
    pub trait RefParam<T> {
        const SIGIL: &'static str;
    }
    impl<T: el::SubOf<P>, P: el::Elem> RefParam<T> for P {
        const SIGIL: &'static str = "@";
    }
    impl<T: el::SubOf<P>, P: el::Elem> RefParam<T> for ts::Array<P> {
        const SIGIL: &'static str = "*";
    }
    impl<T> RefParam<T> for ts::Any {
        const SIGIL: &'static str = "@";
    }

    /// A parameter type an event of type `E` fits.
    pub trait EventParam<E> {}
    impl<E: ev::SubOf<P>, P: ev::EventType> EventParam<E> for P {}
    impl<E> EventParam<E> for ts::Any {}

    impl<T, P: RefParam<T>> Arg<P> for Ref<T> {
        fn write_arg(&self, ctx: &Ctx, out: &mut String) {
            out.push_str(P::SIGIL);
            qualified(out, self.scope, ctx.current);
            out.push_str(self.name);
        }
    }
    impl<T, P: RefParam<T>> Arg<P> for TargetKw<T> {
        fn write_arg(&self, _: &Ctx, out: &mut String) {
            out.push_str("$t");
        }
    }
    impl<P: RefParam<el::HTMLElement>> Arg<P> for ScopeKw {
        fn write_arg(&self, ctx: &Ctx, out: &mut String) {
            out.push_str("$c");
            if let Some(home) = ctx.home {
                if ctx.current != Some(home) {
                    out.push('/');
                    out.push_str(home);
                }
            }
        }
    }
    impl<E, P: EventParam<E>> Arg<P> for EventKw<E> {
        fn write_arg(&self, _: &Ctx, out: &mut String) {
            out.push_str("$e");
        }
    }
    impl Arg<ts::AbortSignal> for SignalKw {
        fn write_arg(&self, _: &Ctx, out: &mut String) {
            out.push_str("$s");
        }
    }
    impl Arg<ts::Any> for SignalKw {
        fn write_arg(&self, _: &Ctx, out: &mut String) {
            out.push_str("$s");
        }
    }

    /// `on:RowToggled={..}` names a type made with `#[event]`.
    pub fn is_event<E: DomEvent>() {}

    /// Writes `text` as a JSON string, escaped for a double-quoted attribute.
    pub fn json_str(text: &str, out: &mut String) {
        out.push_str("&quot;");
        for c in text.chars() {
            match c {
                '"' => out.push_str("\\&quot;"),
                '\\' => out.push_str("\\\\"),
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '\'' => out.push_str("&#39;"),
                c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                    use std::fmt::Write;
                    let _ = write!(out, "\\u{:04x}", c as u32);
                }
                c => out.push(c),
            }
        }
        out.push_str("&quot;");
    }

    /// A value as written and behind one or two references (`&&str` from `rows.iter()`).
    macro_rules! values {
        ($p:ty, |$v:ident, $out:ident| $write:expr; $($t:ty),*) => {$(
            impl Arg<$p> for $t {
                #[inline]
                fn write_arg(&self, _: &Ctx, $out: &mut String) {
                    let $v: &$t = self;
                    $write
                }
            }
            impl Arg<$p> for &$t {
                #[inline]
                fn write_arg(&self, ctx: &Ctx, out: &mut String) {
                    Arg::<$p>::write_arg(*self, ctx, out)
                }
            }
            impl Arg<$p> for &&$t {
                #[inline]
                fn write_arg(&self, ctx: &Ctx, out: &mut String) {
                    Arg::<$p>::write_arg(**self, ctx, out)
                }
            }
        )*};
    }
    values!(ts::String, |v, out| json_str(v, out); &str, String, std::borrow::Cow<'_, str>);
    values!(ts::Number, |v, out| { use std::fmt::Write; let _ = write!(out, "{v}"); }; u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);
    // JSON has no NaN or infinity.
    values!(ts::Number, |v, out| if v.is_finite() { use std::fmt::Write; let _ = write!(out, "{v}"); } else { out.push_str("null") }; f32, f64);
    values!(ts::Boolean, |v, out| out.push_str(if *v { "true" } else { "false" }); bool);

    impl<R: crate::Route> Arg<ts::String> for R {
        fn write_arg(&self, _: &Ctx, out: &mut String) {
            let mut url = String::new();
            self.write_href(&mut url, false);
            json_str(&url, out)
        }
    }

    /// Any `Serialize` value, as JSON, for a parameter whose JSDoc type Corten does not check.
    #[cfg(feature = "json")]
    impl<T: serde::Serialize> Arg<ts::Any> for T {
        fn write_arg(&self, _: &Ctx, out: &mut String) {
            let json = serde_json::to_string(self).expect("a handler argument serializes");
            crate::view::escape_into(&json, out);
        }
    }

    /// How `corten::types` describes a type.
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
