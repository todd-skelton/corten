use corten::{prelude::*, view::Id};

/// A button that copies the text of an element on the page: an order number, a tracking number,
/// an invite link, an API key's one-time secret. The element is named by its `#[derive(Id)]` id
/// (`text={SecretId}`, `text={TrackingId(&parcel.id)}` in a list), so the text is on the page
/// once and each button copies its own row's. A form field (a read-only input with an invite link)
/// is copied by its value.
///
/// The button is drawn `hidden` and its script (`copy_button.js`) shows it where the browser can
/// write the clipboard (a secure origin); without one the text stays on the page to select. A
/// click writes the element's text; once the browser has written it, the button's text reads
/// `copied` for two seconds, in a polite live region so it is announced, then `label` again (an
/// icon with it included). Nothing to copy, or a failed write (no permission), changes nothing, so
/// the button never says it copied when it did not. Both texts are the app's, in the page's
/// language.
///
/// Parts: the `button` and the `span[aria-live=polite]` inside it that holds its text.
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::CopyButton;
///
/// #[derive(Id)]
/// struct TrackingId<'a>(&'a str);
///
/// let parcels = ["1Z999", "1Z998"];
/// let html = html! {
///     for parcel in parcels {
///         <p>
///             <code id={TrackingId(parcel)}>{parcel}</code>
///             <CopyButton text={TrackingId(parcel)} label="Copy" copied="Copied" />
///         </p>
///     }
/// }
/// .to_html();
/// // Each button names its own row's element (ids in this crate's examples start with `ui-`).
/// assert!(html.contains(r#"<code id="ui-tracking-1Z998">1Z998</code><button data-scope="ui-copy-button" type="button" hidden"#), "{html}");
/// assert!(html.contains("copy(&quot;ui-tracking-1Z998&quot;,@said,"), "{html}");
/// ```
///
/// The element is named by its id type, so text in its place does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
/// use corten_ui::CopyButton;
///
/// let html = html! { <CopyButton text="tracking-1Z999" label="Copy" copied="Copied" /> };
/// ```
#[component(js)]
pub fn CopyButton<'a, I: Id, L: Render>(text: I, label: L, copied: &'a str) -> impl Render {
    let said = refs();
    let id = plain_id(&text);
    html! {
        <button type="button" hidden on:connect={js::connect(target)} on:click={js::copy(id.as_str(), said, copied)}>
            <span ref={said} aria-live="polite">{label}</span>
        </button>
    }
}

/// The id as the element has it: `write_id` escapes it for an attribute, and the script gets it
/// as a value, escaped on its own way into the page.
fn plain_id(id: &impl Id) -> String {
    let mut out = String::new();
    id.write_id(&mut out);
    if !out.contains('&') {
        return out;
    }
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}
