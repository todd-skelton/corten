use corten::{
    prelude::*,
    script::{el, Arg},
};

/// A button that copies the text of an element on the page: an order number, a tracking number,
/// an invite link, an API key's one-time secret. The text is a ref the page puts on its element
/// (`let secret = refs();`, `<code ref={secret}>..</code>`), so it is on the page once.
///
/// The button is drawn `hidden` and its script (`copy_button.js`) shows it where the browser can
/// write the clipboard (a secure origin); without one the text stays on the page to select. A
/// click writes the element's text; once the browser has written it, the button's text reads
/// `copied` for two seconds, in a polite live region so it is announced, then `label` again. A
/// failed write (no permission) changes nothing, so the button never says it copied when it did
/// not. Both texts are the app's, in the page's language.
///
/// Parts: the `button` and the `span[aria-live=polite]` inside it that holds its text.
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::CopyButton;
///
/// #[component]
/// fn OrderNumber<'a>(number: &'a str) -> impl Render {
///     let text = refs();
///     html! {
///         <p>
///             <code ref={text}>{number}</code>
///             <CopyButton text={text} label="Copy" copied="Copied" />
///         </p>
///     }
/// }
///
/// let html = OrderNumber { number: "ORD-1234" }.to_html();
/// assert!(html.contains(r#"<code data-ref="text">ORD-1234</code>"#), "{html}");
/// assert!(html.contains(r#"<button data-scope="ui-copy-button" type="button" hidden"#), "{html}");
/// // The text is the page's own element, found by its ref.
/// assert!(html.contains("copy(@ui-order-number/text,@said,"), "{html}");
/// ```
///
/// The text is a ref to an element, not text written into the page a second time:
///
/// ```compile_fail
/// use corten::prelude::*;
/// use corten_ui::CopyButton;
///
/// let html = html! { <CopyButton text="ORD-1234" label="Copy" copied="Copied" /> };
/// ```
#[component(js)]
pub fn CopyButton<'a, T, L: Render>(text: Ref<T>, label: L, copied: &'a str) -> impl Render
where
    Ref<T>: Arg<el::HTMLElement>,
{
    let said = refs();
    html! {
        <button type="button" hidden on:connect={js::connect(target)} on:click={js::copy(text, said, copied, signal)}>
            <span ref={said} aria-live="polite">{label}</span>
        </button>
    }
}
