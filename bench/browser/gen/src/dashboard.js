// @ts-check
// The dashboard's handler (Dashboard in main.rs): a tick moves some prices; it writes each moved
// card through the generated Card view, and the total.
import { state } from "corten";
import { Card, rows } from "corten/views";
import { SYMBOLS, start, tick as next } from "../../apps/shared/market.js";
/** @import { Holds } from "corten/views" */

/** Marks the page ready, for the harness. */
export function ready() {
  document.documentElement.dataset.ready = String(performance.now());
}

/**
 * Applies the next tick of `n` prices.
 * @param {Holds<Card, HTMLDivElement>} cards @param {HTMLSpanElement} sum @param {number} n
 */
export function tick(cards, sum, n) {
  const prices = state(cards, () => SYMBOLS.map((_, i) => start(i)));
  const list = rows(cards, Card);
  for (const [i, p] of next(n)) {
    const card = list.at(i);
    card.down = p < prices[i];
    card.price = p.toFixed(2);
    prices[i] = p;
  }
  let total = 0;
  for (const p of prices) total += p;
  sum.textContent = total.toFixed(2);
}
