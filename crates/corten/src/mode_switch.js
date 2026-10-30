// @ts-check

/** A name from a fixed set written as a string literal union: refused when the crate compiles. @param {HTMLElement} el @param {"list" | "grid"} mode */
export function show(el, mode) {
  el.dataset.mode = mode;
}
