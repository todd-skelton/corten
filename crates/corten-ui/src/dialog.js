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
 * `modal`, for a dialog the address opens (`?selected=it_1`) and a link closes: a close request
 * (Escape, Android's Back, a click outside a `closedby="any"` dialog, `requestClose()`) leaves
 * for the close link's address without adding to the history, so the address, a reload and a
 * redraw agree with what the user closed and Back cannot loop through it. Where the entry before
 * is that address (the user came from the list), it goes back to it; otherwise the link's
 * address replaces the dialog's. Only a `cancel` of this dialog: a file input's picker
 * dismissed inside it fires one too, which bubbles. A dialog drawn closed and opened later gets
 * the same.
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
      // Followed once; a second close request before the page arrives (a slow or failed
      // navigation) closes the dialog as it would.
      if (followed) return;
      followed = true;
      // Open until the page without it replaces it, where the browser allows: Chromium does not
      // let a page keep a dialog open on Escape without a click since the last time (or since
      // the page loaded). The address changes either way.
      event.preventDefault();
      leave(close.href);
    },
    { signal },
  );
  // A browser that loads whole pages keeps this one as it was left in its back/forward cache;
  // restored there, closing it leaves again.
  addEventListener("pageshow", (event) => event.persisted && (followed = false), { signal });
}

/** To `href` without a new history entry: back, when the entry before is `href`, else in place of this one. @param {string} href */
function leave(href) {
  const navigation = /** @type {any} */ (window).navigation;
  if (!navigation) return location.replace(href);
  const at = navigation.currentEntry?.index ?? -1;
  if (at > 0 && navigation.entries()[at - 1]?.url === href) navigation.back();
  else navigation.navigate(href, { history: "replace" });
}
