// @ts-check
// CopyButton (copy_button.rs). The button is drawn hidden: without a script it cannot copy, and
// the text stays on the page to select. This shows it where the browser can write the clipboard,
// copies the named element's text on a click, and says so for a moment.

import { keep } from "corten";

/** How long the button says it copied, in milliseconds. */
const SAID = 2000;

/** The button's label to put back once it has said it copied, and the timer that will. */
const shown = () => keep(() => ({ label: /** @type {Node[] | null} */ (null), timer: 0 }));

/**
 * Shown where the browser has the Clipboard API (a secure origin). When the button leaves the
 * page, its timer ends and its label is put back, so one that comes back does not say it copied.
 * @param {HTMLButtonElement} button @param {HTMLSpanElement} said @param {AbortSignal} signal
 */
export function connect(button, said, signal) {
  if (typeof navigator.clipboard?.writeText === "function") button.hidden = false;
  const s = shown();
  signal.addEventListener("abort", () => {
    clearTimeout(s.timer);
    if (s.label) said.replaceChildren(...s.label);
    s.label = null;
  });
}

/**
 * Writes the text of the element with id `id` (a form field's value) to the clipboard; once it
 * is written, `said` reads `copied` for a moment (a polite live region, so it is announced), then
 * its label again. Nothing to copy, or a failed write, changes nothing, so the button never
 * claims a copy that did not happen.
 * @param {string} id @param {HTMLSpanElement} said @param {string} copied
 */
export async function copy(id, said, copied) {
  // Taken before the write: keep() reads the running handler, which an await leaves.
  const s = shown();
  const from = document.getElementById(id);
  const text = from instanceof HTMLInputElement || from instanceof HTMLTextAreaElement ? from.value : (from?.textContent ?? "");
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    return;
  }
  clearTimeout(s.timer);
  const label = (s.label ??= [...said.childNodes]);
  said.textContent = copied;
  s.timer = setTimeout(() => {
    said.replaceChildren(...label);
    s.label = null;
  }, SAID);
}
