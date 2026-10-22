// @ts-check
/** @param {string} text @param {number} n @param {boolean} loud */
export function log(text, n, loud) {
  console.log(loud ? text.toUpperCase() : text, n);
}

/** @param {AbortSignal} signal @param {{ id: string, tags: string[] }} options */
export function watch(signal, options) {
  signal.addEventListener("abort", () => console.log("stopped", options.id));
}
