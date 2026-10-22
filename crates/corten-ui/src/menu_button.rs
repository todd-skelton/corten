use corten::prelude::*;

use crate::{Link, Url};

/// One item of a [`MenuButton`]'s menu.
pub struct MenuItem<'a> {
    /// What the item shows.
    pub label: &'a dyn Render,
    /// Where it goes, or with `post`, where it posts.
    pub link: &'a dyn Link,
    /// `true`: the item is a form action (`<form method="post" data-patch>`), so it can change
    /// something and the server answers with patches, a page or a redirect. `false`: a link.
    pub post: bool,
}

/// A button that opens a menu of links and form actions: the WAI-ARIA menu button pattern.
///
/// The menu is a popover the button opens with `popovertarget`, so it opens, closes on Escape
/// or a click outside, and its items work before the script (`menu_button.js`) loads. The
/// script adds the keys (Arrow Down or Up on the button opens the menu at the first or last
/// item; in the menu the arrow keys, Home, End and typing a letter move, Escape and Tab close),
/// moves focus into the menu and back to the button, keeps `aria-expanded`, makes the button the
/// menu's CSS anchor, and closes the menu when an item is used.
///
/// Parts: `button[aria-haspopup=menu]` (`[aria-expanded=true]` when open); `[role=menu]`,
/// the popover, whose anchor is the button (place it with `position-area`); `[role=menuitem]`, an
/// `a` or a `button` in a form.
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::{MenuButton, MenuItem};
///
/// #[derive(Route)]
/// #[route("/orders")]
/// struct Orders;
///
/// #[derive(Route)]
/// #[route("/sign-out")]
/// struct SignOut;
///
/// let items = [MenuItem { label: &"Orders", link: &Orders, post: false }, MenuItem { label: &"Sign out", link: &SignOut, post: true }];
/// let html = MenuButton { id: "account", label: &"Account", items: &items }.to_html();
/// assert!(html.contains(r#"<button type="button" id="account-button" popovertarget="account" aria-haspopup="menu" aria-expanded="false""#));
/// assert!(html.contains(r#"<a role="menuitem" href="/orders""#));
/// assert!(html.contains(r#"<form method="post" action="/sign-out" data-patch><button role="menuitem""#));
/// ```
#[island]
pub fn MenuButton<'a>(id: &'a str, label: &'a dyn Render, items: &'a [MenuItem<'a>]) -> impl Render {
    html! {
        <button type="button" id={(id, "-button")} popovertarget={id} aria-haspopup="menu" aria-expanded="false"
            ref={Trigger} on:keydown={Open}>{label}</button>
        <div popover id={id} role="menu" aria-labelledby={(id, "-button")} ref={Menu} on:keydown={Move} on:toggle={Toggled}>
            for item in items {
                if item.post {
                    <form method="post" action={Url(item.link)} data-patch>
                        <button role="menuitem" ref={Item} on:click={Used}>{item.label}</button>
                    </form>
                } else {
                    <a role="menuitem" href={Url(item.link)} ref={Item} on:click={Used}>{item.label}</a>
                }
            }
        </div>
    }
}
