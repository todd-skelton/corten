// @ts-check
// corten_ui::dialog. The platform opens a dialog as a modal only from a script (`showModal`) or a
// click (`command="show-modal"`): a dialog the server draws `open` (a refused form, a result to
// read) is open but not modal. `modal` makes it modal when it joins the page.

/**
 * A dialog drawn `open` becomes modal: in the top layer over a backdrop, the page behind it inert,
 * Escape closes it, and focus goes to the first field the server marked invalid in it (else where
 * the browser puts it). A dialog drawn closed, or already modal, is left as it is.
 * @param {HTMLDialogElement} dialog
 */
export function modal(dialog) {
  if (!dialog.open || dialog.matches(":modal")) return;
  // Without `open` rather than close(): no `close` event for a dialog the user saw stay open.
  dialog.removeAttribute("open");
  dialog.showModal();
  const invalid = dialog.querySelector('[aria-invalid="true"]');
  if (invalid instanceof HTMLElement) invalid.focus();
}

/**
 * `modal`, for a dialog the address opens (`?selected=it_1`) and a link closes: Escape is a click
 * on `close`, so the address, a reload and a redraw agree with what the user closed, and the
 * history is what clicking the link makes. Only a `cancel` of this dialog: a file input's picker
 * dismissed inside it fires one too, which bubbles.
 * @param {HTMLDialogElement} dialog
 * @param {HTMLAnchorElement} close
 * @param {AbortSignal} signal
 */
export function modalClosedBy(dialog, close, signal) {
  modal(dialog);
  let followed = false;
  dialog.addEventListener(
    "cancel",
    (event) => {
      if (event.target !== dialog) return;
      // Followed once; a second Escape before the page arrives (a slow or failed navigation)
      // closes the dialog as Escape does.
      if (followed) return;
      followed = true;
      // Open until the page without it replaces it, where the browser allows: Chromium does not
      // let a page keep a dialog open on Escape without a click since the last time (or since
      // the page loaded). The link is followed either way.
      event.preventDefault();
      close.click();
    },
    { signal },
  );
}