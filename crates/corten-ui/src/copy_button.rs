use corten::{prelude::*, view::Id};

/// The event a [`CopyButton`] dispatches once it has written the clipboard (`copied` in the
/// page). It bubbles from the button, so an ancestor hears the copies of the buttons inside it,
/// and it carries nothing: the text may be a secret, and the element that heard it knows what it
/// holds. A denied write, or nothing to copy, dispatches nothing.
#[corten::script::event]
pub struct Copied {}

/// A button that copies the text of an element on the page: an order number, a tracking number,
/// an invite link, an API key's one-time secret. The element is named by its `#[derive(Id)]` id
/// (`text={SecretId}`, `text={TrackingId(&parcel.id)}` in a list), so the text is on the page
/// once and each button copies its own row's. A form field (a read-only input with an invite link)
/// is copied by its value.
///
/// The button is drawn `hidden` and its script (`copy_button.js`) shows it where the browser has
/// the Clipboard API (a secure origin); without it the text stays on the page to select. A frame
/// the clipboard is not allowed in still shows it, and a click there changes nothing. A
/// click writes the element's text; once the browser has written it, the button's text reads
/// `copied` for two seconds, in a polite live region so it is announced, then `label` again (an
/// icon with it included). Nothing to copy, or a failed write (no permission), changes nothing, so
/// the button never says it copied when it did not. Both texts are the app's, in the page's
/// language.
///
/// After a write, and only then, the button dispatches a [`Copied`] event, which bubbles, so an
/// ancestor can count the copy (`on:Copied={js::counted(event)}`). Listing `CopyButton` serves
/// the event's class; the app does not list `Copied`.
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
/// assert!(html.contains("copy($t,&quot;ui-tracking-1Z998&quot;,@said,"), "{html}");
/// ```
///
/// The element is named by its id type, so text in its place does not compile:
///
/// ```compile_fail,E0277
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
        <button
            type="button"
            hidden
            on:connect={js::connect(target, said, signal)}
            on:click={js::copy(target, id.as_str(), said, copied)}
        >
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
