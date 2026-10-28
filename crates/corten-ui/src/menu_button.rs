use corten::prelude::*;

use crate::{Link, Url};

/// One item of a [`MenuButton`]'s menu.
pub struct MenuItem<'a, L: Render> {
    /// What the item shows: text, or a view with text in it (an icon beside visually hidden text),
    /// since typing a letter in the menu moves to the item whose text starts with it. Every item of
    /// a menu has one label type; labels of different types (a message with arguments beside one
    /// without) are each a reference, `MenuItem::<&dyn Render> { label: &t.archive(n), .. }`.
    pub label: L,
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
/// Parts: a `div` around `button[aria-haspopup=menu]` (`[aria-expanded=true]` when open); `[role=menu]`,
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
/// let items = [MenuItem { label: "Orders", link: &Orders, post: false }, MenuItem { label: "Sign out", link: &SignOut, post: true }];
/// let html = MenuButton { id: "account", label: "Account", items: &items }.to_html();
/// assert!(html.contains(r#"<button data-ref="trigger" type="button" id="account-button" popovertarget="account" aria-haspopup="menu" aria-expanded="false""#));
/// assert!(html.contains(r#"<a data-ref="item" role="menuitem" href="/orders""#));
/// assert!(html.contains(r#"<form method="post" action="/sign-out" data-patch><button data-ref="item" role="menuitem""#));
/// ```
#[component(js)]
pub fn MenuButton<'a, L: Render, I: Render>(id: &'a str, label: L, items: &'a [MenuItem<'a, I>]) -> impl Render {
    let (trigger, menu, item) = refs();
    html! {
        <div on:connect={js::connect(trigger, menu, item)}>
            <button
                ref={trigger}
                type="button"
                id={(id, "-button")}
                popovertarget={id}
                aria-haspopup="menu"
                aria-expanded="false"
                on:keydown={js::open(event, menu)}
            >
                {label}
            </button>
            <div
                ref={menu}
                popover
                id={id}
                role="menu"
                aria-labelledby={(id, "-button")}
                on:keydown={js::step(event, menu, item)}
                on:beforetoggle={js::closing(event, menu)}
                on:toggle={js::toggled(event, trigger, menu, item)}
            >
                for i in items {
                    if i.post {
                        <form method="post" action={Url(i.link)} data-patch>
                            <button ref={item as HTMLElement} role="menuitem" on:click={js::used(menu)}>{i.label}</button>
                        </form>
                    } else {
                        <a ref={item as HTMLElement} role="menuitem" href={Url(i.link)} on:click={js::used(menu)}>{i.label}</a>
                    }
                }
            </div>
        </div>
    }
}
