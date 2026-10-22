// @ts-check
/** @param {string} text @param {number} n @param {boolean} loud */
export function log(text, n, loud) {
  console.log(loud ? text.toUpperCase() : text, n);
}
