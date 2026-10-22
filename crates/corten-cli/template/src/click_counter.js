// @ts-check
// The handlers of <ClickCounter /> in views.rs. Each counter's count is the text of its own
// <output>, so the page holds the state and nothing else needs to.

/** @param {HTMLOutputElement} total */
export function count(total) {
  total.value = String(Number(total.value) + 1);
}
