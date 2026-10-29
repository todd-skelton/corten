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
| `Toaster` | the message is part of the page the post answers with | a live region an action's patch fills (`req.patch_or(Toast { message: Some(text) }, page)`), hidden after `seconds` but kept while the pointer or focus is on it or the page is hidden, and a close button |
| `CopyButton` | no button: the text (an order number, an API secret) is on the page to select | a button, where the browser has the Clipboard API (a secure origin), that copies the text (or a field's value) of the element its `#[derive(Id)]` id names (`<code id={SecretId}>`, `<CopyButton text={SecretId} label={..} copied={..} />`), so the text is on the page once, and says it copied in a polite live region, then dispatches `Copied`, which bubbles, so an ancestor can count the copy (`on:Copied={js::counted(event)}`); nothing to copy or a failed write changes nothing and dispatches nothing |
| `PrintButton` | no button: the browser's own Print command prints the page | a button (`<PrintButton label={..} />`) that opens the print dialog; what prints, and at what paper size, is the page's print styles ([Printing](styles.md#printing)) |
| `CheckboxGroup` | a `<fieldset>` with its legend around the app's checkboxes of one name; the form is sent with none checked, and the handler refuses it | while no enabled box is checked, the first is invalid with the app's message (`missing="Choose where you ship."`), so the browser stops the submit and says so there, as for a required text field; checked again after a form reset |
| `RequiredWhen` | a `<fieldset>` around the app's fields, enabled, nothing required; the handler checks the rule | while the form's `control` field (a `FieldName`: a select, radios or a checkbox, also one joined with `form=`) has one of `values`, every field in the set but checkboxes is `required`; for any other value the set is disabled, so it is neither checked nor sent. Hide it with `:has` on the control in the app's CSS, which holds before the script and without it |

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
all ten on one screen, a seller's draft listing for a card, styled by a small design system, with a dialog the server refuses a form in and one the
address opens, and has a browser test that runs in Chromium, Firefox and
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

A dialog the address opens (a record beside its list, `/items?selected=it_1`, closed by a link to
`/items`) is drawn `open` the same way, but Escape on a modal dialog closes it where the address
cannot see: a reload or a redraw would open it again. `dialog::modalClosedBy` makes it modal, and
a close request on it (Escape, Android's Back, a click outside a `closedby="any"` dialog) goes to
its close link's address, a ref, without a new history entry and without clicking the link: back,
when the entry before is that address, else in place of the dialog's entry (always so in a browser
without the Navigation API, where Back after a dialog opened from the list shows the list once
more). Attributes on the link (`data-reload`, `data-keep-place`, an `on:click`) do not apply.
Android's Back on the open dialog cannot loop between the two. A `cancel` from inside it, such as
a file input's dismissed picker, is left alone. The close is a link: a button there does not
compile. Without a script, the dialog is open and the link closes it.

```rust,ignore
let close = refs();
html! {
    <dialog open aria-label={item.name} on:connect={dialog::modalClosedBy(target, close, signal)}>
        ..
        <a ref={close} href={url::Items { selected: None }}>"Close"</a>
    </dialog>
}
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
