// @ts-check
// CopyButton (copy_button.rs). The button is drawn hidden: without a script it cannot copy, and
// the text stays on the page to select. This shows it where the browser can write the clipboard,
// copies the named element's text on a click, and says so for a moment.

/** How long the button says it copied, in milliseconds. */
const SAID = 2000;

/** The label to put back, and the timer that will. @type {WeakMap<HTMLSpanElement, { label: Node[], timer: number }>} */
const shown = new WeakMap();

/** Shown only where the clipboard can be written (a secure origin). @param {HTMLButtonElement} button */
export function connect(button) {
  if (navigator.clipboard?.writeText) button.hidden = false;
}

/**
 * Writes the text of the element with id `id` (a form field's value) to the clipboard; once it
 * is written, `said` reads `copied` for a moment (a polite live region, so it is announced), then
 * its label again. Nothing to copy, or a failed write, changes nothing, so the button never
 * claims a copy that did not happen.
 * @param {string} id @param {HTMLSpanElement} said @param {string} copied
 */
export async function copy(id, said, copied) {
  const from = document.getElementById(id);
  const text = from instanceof HTMLInputElement || from instanceof HTMLTextAreaElement ? from.value : (from?.textContent ?? "");
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    return;
  }
  const was = shown.get(said);
  clearTimeout(was?.timer);
  const label = was?.label ?? [...said.childNodes];
  said.textContent = copied;
  const timer = setTimeout(() => {
    said.replaceChildren(...label);
    shown.delete(said);
  }, SAID);
  shown.set(said, { label, timer });
}
