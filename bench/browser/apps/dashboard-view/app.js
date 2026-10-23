// @ts-check
// The dashboard in plain JavaScript with generated views: the component that receives the ticks
// owns the cards and the total, so its handler writes them.
import { mount } from "corten/state";
import { SYMBOLS, start, tick } from "../shared/market.js";

/** What html! would generate from a Rust `Card`: the price hole, and its up/down class. */
class Card {
  /** @param {Element} el */
  constructor(el) {
    this.price = /** @type {Element} */ (el.querySelector("[data-ref=price]"));
  }
  /** Shows `now`, marked up or down from `before`. @param {number} before @param {number} now */
  move(before, now) {
    this.price.className = now >= before ? "up" : "down";
    this.price.textContent = now.toFixed(2);
  }
}

/** @param {{ root: Element, refs: Record<string, Element> }} ctx */
function dashboard({ root, refs }) {
  const cards = Array.from(root.querySelectorAll("[data-symbol]"), (el) => new Card(el));
  const prices = SYMBOLS.map((_, i) => start(i));
  /** @param {number} n */
  const apply = (n) => {
    for (const [i, p] of tick(n)) {
      cards[i].move(prices[i], p);
      prices[i] = p;
    }
    let total = 0;
    for (const p of prices) total += p;
    refs.total.textContent = total.toFixed(2);
  };
  return { tick20: () => apply(20), tick200: () => apply(200) };
}

mount(/** @type {Element} */ (document.getElementById("main")), dashboard);
document.documentElement.dataset.ready = String(performance.now());
