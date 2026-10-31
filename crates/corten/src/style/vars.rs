//! Custom properties set from data: `style={FILL.pct(42.0)}` in a component's markup, for a size
//! or position computed as the page renders (a meter's fill, a bar's length, a mark on a
//! timeline), under a CSP that allows no style attribute.

use std::{cell::RefCell, marker::PhantomData};

use crate::view::Render;

/// What a custom property holds, from the `syntax` of the `@property` rule that registers it:
/// the type parameter of its [`Var`], which gives it the methods for that kind of value.
pub mod syntax {
    /// `syntax: "<number>"`: `set`.
    #[derive(Clone, Copy, Debug)]
    pub struct Number;
    /// `syntax: "<integer>"`: `set`.
    #[derive(Clone, Copy, Debug)]
    pub struct Integer;
    /// `syntax: "<percentage>"`: `pct`.
    #[derive(Clone, Copy, Debug)]
    pub struct Percentage;
    /// `syntax: "<length>"`: `px`, `rem`, `em`, `ch`, `vw`, `vh`.
    #[derive(Clone, Copy, Debug)]
    pub struct Length;
    /// `syntax: "<length-percentage>"`: a length's units and `pct`.
    #[derive(Clone, Copy, Debug)]
    pub struct LengthPercentage;
    /// `syntax: "<angle>"`: `deg`, `turn`.
    #[derive(Clone, Copy, Debug)]
    pub struct Angle;
    /// `syntax: "<time>"`: `ms`, `s`.
    #[derive(Clone, Copy, Debug)]
    pub struct Time;
}

use syntax::*;

/// A custom property a component's stylesheet registers with `@property`, as markup sets it:
/// `@property --fill { syntax: "<percentage>"; inherits: false; initial-value: 0% }` in
/// `meter.css` is `FILL: Var<Percentage>` in `Meter`'s markup, named `--meter__fill` (the
/// component's, as its classes are), and `style={FILL.pct(42.0)}` sets it on an element.
///
/// Its methods are the units its syntax allows, so a value is a number with a unit and no CSS text
/// from data reaches the page. A number that is not finite sets nothing: the property keeps its
/// `initial-value`.
#[derive(Clone, Copy, Debug)]
pub struct Var<K> {
    name: &'static str,
    kind: PhantomData<K>,
}

impl<K> Var<K> {
    #[doc(hidden)]
    pub const fn __new(name: &'static str) -> Self {
        Var { name, kind: PhantomData }
    }

    /// The scoped name, `"--meter__fill"`.
    pub const fn name(self) -> &'static str {
        self.name
    }

    fn float(self, value: f64, unit: &'static str) -> Declaration {
        Declaration { name: self.name, value: Value::Float(value), unit }
    }
}

impl Var<Number> {
    /// `--scale:1.5`.
    pub fn set(self, value: f64) -> Declaration {
        self.float(value, "")
    }
}

impl Var<Integer> {
    /// `--span:3`.
    pub fn set(self, value: i64) -> Declaration {
        Declaration { name: self.name, value: Value::Int(value), unit: "" }
    }
}

impl Var<Percentage> {
    /// `--fill:42%`.
    pub fn pct(self, value: f64) -> Declaration {
        self.float(value, "%")
    }
}

macro_rules! units {
    ($kind:ty: $($unit:ident),*) => {
        impl Var<$kind> {
            $(
                #[doc = concat!("The value in `", stringify!($unit), "`.")]
                // `rem` is the CSS unit, named as CSS writes it, not a remainder.
                #[allow(clippy::should_implement_trait)]
                pub fn $unit(self, value: f64) -> Declaration {
                    self.float(value, stringify!($unit))
                }
            )*
        }
    };
}
units!(Length: px, rem, em, ch, vw, vh);
units!(LengthPercentage: px, rem, em, ch, vw, vh);
units!(Angle: deg, turn);
units!(Time: ms, s);

impl Var<LengthPercentage> {
    /// `--inset:42%`.
    pub fn pct(self, value: f64) -> Declaration {
        self.float(value, "%")
    }
}

/// One custom property's value, from a [`Var`]'s method: `FILL.pct(42.0)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Declaration {
    name: &'static str,
    value: Value,
    unit: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Value {
    Float(f64),
    Int(i64),
}

/// What `style={..}` takes: a [`Declaration`], an `Option` of one (`None` sets nothing), or a
/// tuple or array of them. With nothing to set, the element gets no attribute.
#[diagnostic::on_unimplemented(
    message = "style={{..}} takes custom properties of the component's stylesheet (FILL.pct(42.0)), not CSS text",
    label = "not a custom property's value",
    note = "register the property with @property in the component's stylesheet; its constant is its name in capitals"
)]
pub trait Vars: sealed::Sealed {
    /// Writes each declaration followed by `;`.
    fn write_vars(&self, out: &mut String);
}

impl Vars for Declaration {
    #[inline]
    fn write_vars(&self, out: &mut String) {
        match self.value {
            Value::Float(value) if !value.is_finite() => return,
            Value::Float(value) => {
                out.push_str(self.name);
                out.push(':');
                Render::render(&value, out);
            }
            Value::Int(value) => {
                out.push_str(self.name);
                out.push(':');
                Render::render(&value, out);
            }
        }
        out.push_str(self.unit);
        out.push(';');
    }
}

impl<T: Vars + ?Sized> Vars for &T {
    #[inline]
    fn write_vars(&self, out: &mut String) {
        (**self).write_vars(out)
    }
}

impl<T: Vars> Vars for Option<T> {
    #[inline]
    fn write_vars(&self, out: &mut String) {
        if let Some(value) = self {
            value.write_vars(out)
        }
    }
}

impl<T: Vars> Vars for [T] {
    fn write_vars(&self, out: &mut String) {
        for value in self {
            value.write_vars(out)
        }
    }
}

impl<T: Vars, const N: usize> Vars for [T; N] {
    fn write_vars(&self, out: &mut String) {
        self.as_slice().write_vars(out)
    }
}

/// Only Corten's types are values: a type of the app's could write CSS text into the attribute
/// and the page's style block.
mod sealed {
    pub trait Sealed {}
    impl Sealed for super::Declaration {}
    impl<T: Sealed + ?Sized> Sealed for &T {}
    impl<T: Sealed> Sealed for Option<T> {}
    impl<T: Sealed> Sealed for [T] {}
    impl<T: Sealed, const N: usize> Sealed for [T; N] {}
    macro_rules! tuples {
        ($(($($name:ident),+)),*) => {$(impl<$($name: Sealed),+> Sealed for ($($name,)+) {})*};
    }
    tuples!((A, B), (A, B, C), (A, B, C, D), (A, B, C, D, E), (A, B, C, D, E, F), (A, B, C, D, E, F, G), (A, B, C, D, E, F, G, H));
}

macro_rules! tuples {
    ($(($($name:ident),+)),*) => {$(
        impl<$($name: Vars),+> Vars for ($($name,)+) {
            #[allow(non_snake_case)]
            #[inline]
            fn write_vars(&self, out: &mut String) {
                let ($($name,)+) = self;
                $($name.write_vars(out);)+
            }
        }
    )*};
}
tuples!((A, B), (A, B, C), (A, B, C, D), (A, B, C, D, E), (A, B, C, D, E, F), (A, B, C, D, E, F, G), (A, B, C, D, E, F, G, H));

/// The values a full page's render wrote, one after another, and where each ends.
struct Written {
    text: String,
    ends: Vec<usize>,
}

thread_local! {
    static WRITTEN: RefCell<Option<Written>> = const { RefCell::new(None) };
}

/// Runs `render` and returns the page's style block for the values it wrote: `[data-vars="X"]{X}`
/// for each distinct one, sorted, so a page whose values are the same is the same bytes (and the
/// same CSP hash). Empty when it wrote none.
pub(crate) fn collect<T>(render: impl FnOnce() -> T) -> (T, String) {
    let previous = WRITTEN.with(|w| w.replace(Some(Written { text: String::new(), ends: Vec::new() })));
    let value = render();
    let written = WRITTEN.with(|w| w.replace(previous)).expect("set above");
    if written.ends.is_empty() {
        return (value, String::new());
    }
    let mut all: Vec<&str> = Vec::with_capacity(written.ends.len());
    let mut start = 0;
    for &end in &written.ends {
        all.push(&written.text[start..end]);
        start = end;
    }
    all.sort_unstable();
    all.dedup();
    let mut rules = String::with_capacity(all.iter().map(|v| v.len() * 2 + 16).sum());
    for vars in all {
        rules.push_str("[data-vars=\"");
        rules.push_str(vars);
        rules.push_str("\"]{");
        rules.push_str(vars);
        rules.push('}');
    }
    (value, rules)
}

#[doc(hidden)]
pub mod __private {
    use super::{Vars, WRITTEN};

    /// ` data-vars="--meter__fill:42%"`, or nothing when there is nothing to set. A full page's
    /// values also go to its style block (`collect`).
    #[inline]
    pub fn vars_attr<T: Vars + ?Sized>(vars: &T, out: &mut String) {
        let start = out.len();
        out.push_str(" data-vars=\"");
        let text = out.len();
        vars.write_vars(out);
        if out.len() == text {
            out.truncate(start);
            return;
        }
        // The last declaration's `;`.
        out.pop();
        WRITTEN.with(|w| {
            if let Some(written) = w.borrow_mut().as_mut() {
                written.text.push_str(&out[text..]);
                written.ends.push(written.text.len());
            }
        });
        out.push('"');
    }
}
