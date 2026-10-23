// @ts-check
// The dashboard's data: 200 symbols with a price each, and the ticks every app applies.
export const SYMBOLS = Array.from({ length: 200 }, (_, i) => "S" + String(i).padStart(3, "0"));
export const start = (/** @type {number} */ i) => 100 + (i % 17);
let t = 0;
/** The next tick: `count` symbols (spread over the list) and their new prices. @param {number} count */
export function tick(count) {
  t += 1;
  /** @type {[number, number][]} */
  const out = [];
  for (let k = 0; k < count; k += 1) {
    const i = (k * 7 + t) % SYMBOLS.length;
    out.push([i, start(i) + Math.sin(t + i) * 5]);
  }
  return out;
}
