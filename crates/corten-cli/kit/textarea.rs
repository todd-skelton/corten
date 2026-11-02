// corten ui {{version}}: textarea
//! A multi-line field's look, as the classes of the `<textarea>` you write, as `Input` is for an
//! `<input>`: `<textarea name={f.note} rows="4" class={Textarea {}}></textarea>`.
use corten::prelude::*;

/// A part without a rule in `textarea.css` does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[component(css)]
/// fn Textarea() -> impl ClassList {
///     (Textarea, Tall)
/// }
/// ```
#[component(css)]
pub fn Textarea() -> impl ClassList {
    Textarea
}
