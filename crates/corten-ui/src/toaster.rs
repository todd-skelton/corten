use corten::prelude::*;

/// The toast's id, `ui-toast`: an action's patch replaces the [`Toast`] by it.
#[derive(Id)]
pub struct ToastId;

/// What an action says when it is done ("Saved.", "3 items archived."), inside a [`Toaster`].
/// An action answers with it as a patch, `req.patch(Toast { message: Some(text) })`, which
/// replaces the toaster's message; `None` renders it `hidden` and empty.
#[component]
pub fn Toast<M: Render>(message: Option<M>) -> impl Patch {
    html! {
        <div id={ToastId} hidden={message.is_none()}>
            if let Some(message) = message { {message} }
        </div>
    }
}

/// A toast: a short message after an action, announced politely and gone after `seconds`.
///
/// The toaster's live region (`role="status"`) stays on the page and is never hidden, while
/// patches replace the [`Toast`] inside it, so each new message is announced (a region inserted,
/// or shown, together with its message often is not). Render the toaster once in the page's
/// layout, where an answer without JavaScript can fill it too. One message shows at a time: a
/// new one replaces it.
///
/// Its script (`toaster.js`) shows the close button while there is a message, and hides the
/// message after `seconds` (`0`: until closed), waiting while the pointer is over the toaster,
/// focus is in it or the page is hidden, so a slow reader keeps it (WCAG 2.2.1). Without
/// JavaScript the message is part of the page the post answers with, and there is no button.
///
/// The toaster is not in the top layer: an open modal `<dialog>` covers it, so a form in a
/// dialog answers inside the dialog. A failure the user must act on belongs next to what failed,
/// not in a toast that goes away.
///
/// Parts: a `div` (place it with `position: fixed`; it has no box of its own) around
/// `[role=status]`, which holds the message (`#ui-toast`, the box to style, `[hidden]` when there
/// is none), and the close `button` after it (outside the live region, so it is not announced).
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::{Toast, Toaster};
///
/// let html = Toaster { close: "Dismiss", seconds: 5, toast: Toast { message: Some("Saved.") } }.to_html();
/// assert!(html.contains(r#"<div data-ref="status" role="status"><div id="ui-toast">Saved.</div></div>"#), "{html}");
/// assert_eq!(Toast { message: None::<&str> }.to_html(), r#"<div id="ui-toast" hidden></div>"#);
/// ```
#[component(js)]
pub fn Toaster<C: Render, M: Render>(close: C, seconds: u32, toast: Toast<M>) -> impl Render {
    let (status, button) = refs();
    html! {
        <div on:connect={js::connect(scope, status, button, seconds, signal)}>
            <div ref={status} role="status">{toast}</div>
            <button ref={button} type="button" hidden on:click={js::dismiss(status, button)}>{close}</button>
        </div>
    }
}
