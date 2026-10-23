// @ts-check
import { SYMBOLS, start, tick } from "../shared/market.js";

const prices = SYMBOLS.map((_, i) => start(i));
const cells = Array.from(document.querySelectorAll("[data-symbol] [data-ref=price]"));
const total = /** @type {Element} */ (document.querySelector("[data-ref=total]"));
/** @param {number} n */
function apply(n) {
  for (const [i, p] of tick(n)) {
    cells[i].className = p >= prices[i] ? "up" : "down";
    cells[i].textContent = p.toFixed(2);
    prices[i] = p;
  }
  total.textContent = prices.reduce((a, b) => a + b, 0).toFixed(2);
}
document.getElementById("tick20")?.addEventListener("click", () => apply(20));
document.getElementById("tick200")?.addEventListener("click", () => apply(200));
document.documentElement.dataset.ready = String(performance.now());
