// @ts-check
// A dashboard in the proposal: a shared store (a module-level state), 200 card components that
// each read one key of it, and a summary that reads all of them. A tick changes some prices;
// only the cards whose key changed (and the summary) render again.
import { state, mount, text } from "corten/state";
import { SYMBOLS, start, tick } from "../shared/market.js";

/** @type {Record<string, number>} */
const prices = state(Object.fromEntries(SYMBOLS.map((s, i) => [s, start(i)])));

/** @param {{ root: Element, refs: Record<string, Element> }} ctx */
function card({ root, refs }) {
  const symbol = /** @type {string} */ (root.getAttribute("data-symbol"));
  let last = prices[symbol];
  return {
    render() {
      const price = prices[symbol];
      text(refs.price, price.toFixed(2));
      refs.price.className = price >= last ? "up" : "down";
      last = price;
    },
  };
}

/** @param {number} n */
const apply = (n) => { for (const [i, p] of tick(n)) prices[SYMBOLS[i]] = p; };

/** @param {{ refs: Record<string, Element> }} ctx */
function summary({ refs }) {
  return {
    render() {
      let total = 0;
      // Known keys, not `for..in`: iterating a Proxy's keys takes the engine's slow path.
      for (const symbol of SYMBOLS) total += prices[symbol];
      text(refs.total, total.toFixed(2));
    },
    tick20: () => apply(20),
    tick200: () => apply(200),
  };
}

for (const el of document.querySelectorAll("[data-symbol]")) mount(el, card);
mount(/** @type {Element} */ (document.getElementById("main")), summary);
document.documentElement.dataset.ready = String(performance.now());
