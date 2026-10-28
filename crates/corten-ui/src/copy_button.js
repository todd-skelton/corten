// @ts-check
// CopyButton (copy_button.rs). The button is drawn hidden: without a script it cannot copy, and
// the text stays on the page to select. This shows it where the browser can write the clipboard,
// copies the text element's text on a click, and says so for a moment.

/** How long the button says it copied, in milliseconds. */
const SAID = 2000;

/** Shown only where the clipboard can be written (a secure origin). @param {HTMLButtonElement} button */
export function connect(button) {
  if (navigator.clipboard?.writeText) button.hidden = false;
}

/**
 * Writes `text`'s text to the clipboard; once it is written, `said` reads `copied` for a moment
 * (a polite live region, so it is announced), then its label again. A failed write changes
 * nothing, so the button never claims a copy that did not happen.
 * @param {HTMLElement} text @param {HTMLSpanElement} said @param {string} copied @param {AbortSignal} signal
 */
export async function copy(text, said, copied, signal) {
  try {
    await navigator.clipboard.writeText(text.textContent ?? "");
  } catch {
    return;
  }
  const label = said.dataset.label ?? said.textContent ?? "";
  said.dataset.label = label;
  said.textContent = copied;
  const back = setTimeout(() => (said.textContent = label), SAID);
  signal.addEventListener("abort", () => clearTimeout(back), { once: true });
}
