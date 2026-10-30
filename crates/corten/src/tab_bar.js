// @ts-check
/** @typedef {import("./types/app").Tab} Tab */

/** @param {HTMLElement} bar @param {Tab} tab */
export function show(bar, tab) {
  bar.dataset.tab = tab;
}
