use corten::prelude::*;

/// A button that opens a menu: the WAI-ARIA menu button pattern.
///
/// The items are its children, each the platform's own element with `role="menuitem"`: a link
/// (any `href`, with `download` or `target` as it needs), a `<form method="post">` around a
/// `<button role="menuitem">` with the fields it sends, or a `<button role="menuitem">` that opens
/// a dialog (`command="show-modal" commandfor={..}`). So a menu holds whatever a page can, written
/// the way the page writes it.
///
/// The menu is a popover the button opens with `popovertarget`, so it opens, closes on Escape
/// or a click outside, and its items work before the script (`menu_button.js`) loads. The
/// script adds the keys (Arrow Down or Up on the button opens the menu at the first or last
/// item; in the menu the arrow keys, Home, End and typing a letter move, Escape and Tab close),
/// moves focus into the menu and back to the button, keeps `aria-expanded`, makes the button the
/// menu's CSS anchor, and closes the menu when an item is used. It finds the items as it needs
/// them, so an item a patch adds or takes away is one too.
///
/// Parts: a `div` around `button[aria-haspopup=menu]` (`[aria-expanded=true]` when open);
/// `[role=menu]`, the popover, whose anchor is the button (place it with `position-area`);
/// `[role=menuitem]` (or `menuitemcheckbox`, `menuitemradio`), the app's items. An item's text is
/// what typing a letter matches, so an icon item has visually hidden text. A `disabled` or `hidden`
/// item is passed over by the keys; `aria-disabled="true"` keeps one reachable. Closing a dialog
/// an item opened (`commandfor`) returns focus to the button.
///
/// ```
/// use corten::prelude::*;
/// use corten_ui::MenuButton;
///
/// #[derive(Route)]
/// #[route("/orders")]
/// struct Orders;
///
/// #[derive(Route)]
/// #[route("/orders/export")]
/// struct Export;
///
/// #[derive(Route)]
/// #[route("/labels")]
/// struct Labels;
///
/// #[derive(Id)]
/// struct ConfirmId;
///
/// let html = html! {
///     <MenuButton id="account" label="Account">
///         <a role="menuitem" href={Orders}>"Orders"</a>
///         <a role="menuitem" href={Export} download>"Export"</a>
///         <form method="post" action={Labels} data-patch>
///             <input type="hidden" name="direction" value="return">
///             <button role="menuitem">"Buy return label"</button>
///         </form>
///         <button type="button" role="menuitem" command="show-modal" commandfor={ConfirmId}>"Buy another"</button>
///     </MenuButton>
/// }
/// .to_html();
/// assert!(html.contains(r#"<button data-ref="trigger" type="button" id="account-button" popovertarget="account" aria-haspopup="menu" aria-expanded="false""#));
/// assert!(html.contains(r#"<a role="menuitem" href="/orders/export" download>Export</a>"#));
/// assert!(html.contains(r#"<input type="hidden" name="direction" value="return"><button role="menuitem">"#));
/// ```
#[component(js)]
pub fn MenuButton<'a, L: Render, C: Render>(id: &'a str, label: L, children: C) -> impl Render {
    let (trigger, menu) = refs();
    html! {
        <div on:connect={js::connect(trigger, menu)}>
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
                on:keydown={js::step(event, menu)}
                on:click={js::used(event, trigger, menu)}
                on:beforetoggle={js::closing(event, menu)}
                on:toggle={js::toggled(event, trigger, menu)}
            >
                {children}
            </div>
        </div>
    }
}
