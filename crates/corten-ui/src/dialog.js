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