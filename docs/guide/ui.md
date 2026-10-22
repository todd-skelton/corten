# Headless components

[Back to the guides](../../README.md#documentation)

`corten-ui` is an optional crate of islands for the widgets the browser has no element for. Each
renders accessible markup on the server, works as plain HTML before its script loads, and ships
no CSS.

| Component | Without JavaScript | The script adds |
|---|---|---|
| `NumberField` | a native `<input type="number">` (arrow keys, `min`, `max`, `step`, `required`) | step buttons with press and hold, Page Up and Down, Home and End, the step's decimals |
| `ComboBox` | a native `<select>`, which keeps the value | a text input that filters a listbox (ARIA 1.2 combobox), validation reported on the input |
| `TabGroup` | tabs that are typed links to the page with that tab selected | roving focus, arrow keys, switching in place with the address following |
| `MenuButton` | a popover menu of links and form actions (`popovertarget`) | arrow keys, Home and End, typeahead, focus in and back, `aria-expanded`, the button as the menu's CSS anchor |

```rust,ignore
let qty = Number { value: Some(1.0), min: Some(1.0), ..Number::new("qty", f.qty, t.fewer(), t.more()) };
html! { <div class={Field}><label for="qty">{t.quantity()}</label><NumberField field={&qty} /></div> }
```

A design system wraps each one and styles its parts by role and state, which are what assistive
technology announces, so the styles cannot drift from it: `.field [role="option"][aria-selected="true"]`,
`.tabs [role="tab"][aria-selected="true"]`, `.menu [role="menu"]:popover-open`. The island's tag is
`display: contents`, so the parts are laid out by the wrapper. Labels come from the app, in the
page's language. Links are typed routes. List `corten_ui::COMPONENTS` in `App::components`.

Everything else Base UI-style libraries provide is already the platform's: `<dialog>` with
`command="show-modal"` for dialogs and alert dialogs, `popover` for popovers and tooltips,
`<details name>` for accordions, `<select>` (with `appearance: base-select` where supported),
checkboxes, radios and `<input type="range">`. [examples/ui-gallery](../../examples/ui-gallery) shows
all four styled by a small design system, with a browser test.

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
