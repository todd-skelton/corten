// @ts-check
/** @param {string} text @param {number} n @param {boolean} loud */
export function log(text, n, loud) {
  console.log(loud ? text.toUpperCase() : text, n);
}

/** @param {AbortSignal} signal @param {{ id: string, tags: string[] }} options */
export function watch(signal, options) {
  signal.addEventListener("abort", () => console.log("stopped", options.id));
}

/** @import { Message } from "corten" */

/** @param {HTMLOutputElement} out @param {Message<"count">} label */
export function say(out, label) {
  out.textContent = label({ count: 3 });
}

/** @typedef {{ entity: string, label: Message<"count"> }} Counted */

/** @param {HTMLOutputElement} out @param {Counted} counted */
export function tally(out, counted) {
  out.textContent = `${counted.entity}: ${counted.label({ count: 2 })}`;
}

/** @import { NumberFormat, MoneyFormat, DateFormat } from "corten" */

/** @param {HTMLOutputElement} out @param {NumberFormat} count @param {MoneyFormat} price @param {DateFormat} when */
export function priced(out, count, price, when) {
  out.textContent = `${count(1234)} at ${price(123450)}, ${when(0)}`;
}
