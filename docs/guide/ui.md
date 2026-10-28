# Headless components

[Back to the guides](../../README.md#documentation)

`corten-ui` is an optional crate of components for the widgets the browser has no element for. Each
renders accessible markup on the server, works as plain HTML before its script loads, and ships
no CSS.

| Component | Without JavaScript | The script adds |
|---|---|---|
| `NumberField` | a native `<input type="number">` (arrow keys, `min`, `max`, `step`, `required`) | step buttons with press and hold, Page Up and Down, Home and End, the step's decimals |
| `ComboBox` | a native `<select>`, which keeps the value | a text input that filters a listbox (ARIA 1.2 combobox), validation reported on the input |
| `TabGroup` | tabs that are typed links to the page with that tab selected | roving focus, arrow keys, switching in place with the address following |
| `MenuButton` | a popover menu of links and form actions (`popovertarget`) | arrow keys, Home and End, typeahead, focus in and back, `aria-expanded`, the button as the menu's CSS anchor |
| `Toaster` | the message is part of the page the post answers with | a live region an action's patch fills (`req.patch_or(Toast { message: Some(&text) }, page)`), hidden after `seconds` but kept while the pointer or focus is on it or the page is hidden, and a close button |

```rust,ignore
let qty = Number { value: Some(1.0), min: Some(1.0), ..Number::new("qty", f.qty, t.fewer(), t.more()) };
html! { <div class={Field}><label for="qty">{t.quantity()}</label><NumberField field={&qty} /></div> }
```

A design system wraps each one and styles its parts by role and state, which are what assistive
technology announces, so the styles cannot drift from it: `.field [role="option"][aria-selected="true"]`,
`.tabs [role="tab"][aria-selected="true"]`, `.menu [role="menu"]:popover-open`. Each component's root is a `div`
(`[data-scope=number-field]`) around its parts; the wrapper lays it out, or gives it
`display: contents` so the parts join the wrapper's layout. Labels come from the app, in the
page's language. Links are typed routes. List `corten_ui::COMPONENTS` in `App::components`.

Everything else Base UI-style libraries provide is already the platform's: `<dialog>` with
`command="show-modal"` for dialogs and alert dialogs, `popover` for popovers and tooltips,
`<details name>` for accordions, toggletips as a `popover` opened by `popovertarget`, `<select>` (with `appearance: base-select` where supported),
checkboxes, radios and `<input type="range">`. [examples/ui-gallery](../../examples/ui-gallery) shows
all five styled by a small design system, and a dialog the server refuses a form in, with a browser test that runs in Chromium, Firefox and
WebKit. A tooltip shown on hover or focus is `interestfor` with `popover="hint"` once every
browser has them (Safari has neither in October 2026); the crate does not add one meanwhile. The
toaster is not in the top layer, so an open modal `<dialog>` covers it: a form in a dialog answers
inside the dialog.

A button opens a dialog, or any popover, with no script: `command="show-modal"` (or
`popovertarget`) and `commandfor={..}`, the dialog's `#[derive(Id)]` id, which a component that
renders the dialog for its parent takes as a prop of any id type (`I: Id`). A script calls
`showModal()` only for what a click cannot do, below.

A form in a dialog that the server refuses comes back with the dialog drawn `open`, and when the
answer is a whole page (a post without the runtime) `open` is all the platform has: the dialog
shows, but it is not modal, so focus and Tab leave it for the page behind, which is not inert,
and there is no backdrop. `corten_ui::dialog::modal` makes such a dialog modal when it joins the
page (a load, a navigation or a patch) and focuses the first field marked invalid in it; one drawn
closed stays closed. It takes the dialog itself, so on any other element it does not compile.
List `corten_ui::COMPONENTS` and it is served.

```rust,ignore
<button type="button" command="show-modal" commandfor={RenameId}>"Rename"</button>
<dialog id={RenameId} open={!errors.is_empty()} on:connect={dialog::modal(target)}>
    <form method="post" action={url::Rename} data-patch>..</form>
</dialog>
```

The design system places popups with CSS anchor positioning (the script makes the control that
opens each popup its anchor):

```css
.field [popover] {
  inset: auto;
  position-area: block-end span-inline-end;
  position-try-fallbacks: flip-block;
  min-width: anchor-size(width);
}
```
