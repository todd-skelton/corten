// corten ui {{version}}: avatar
//! A person's picture over their initials, so the initials show while it loads and when there
//! is none: `<Avatar src={Some(&user.photo)} alt={&user.name} initials="AL" />`.
//! `alt` names the person; `alt=""` when their name is written next to it.
use corten::prelude::*;

/// A part without a rule does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[component(css)]
/// fn Avatar() -> impl Render {
///     html! { <span class={(Avatar, Square)}></span> }
/// }
/// ```
#[component(css)]
pub fn Avatar<'a>(src: Option<&'a str>, alt: &'a str, initials: &'a str) -> impl Render {
    html! {
        <span class={Avatar}>
            <span class={Initials} aria-hidden={src.is_some()}>{initials}</span>
            if let Some(src) = src {
                <img src={src} alt={alt} loading="lazy" decoding="async">
            }
        </span>
    }
}
