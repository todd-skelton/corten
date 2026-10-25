// @ts-check
// LivePrices (app.rs): keeps the server-rendered prices current from the feed. While paused it
// keeps the latest rows and shows them on resume.
import { state, subscribe } from "corten";
/** @import { Message } from "corten" */

/** @typedef {import("../types/app").Stock} Stock Generated from `Stock` in src/app.rs. */

/** @param {HTMLElement} root */
const prices = (root) => state(root, () => ({ paused: false, updates: 0, latest: /** @type {Map<string, Stock>} */ (new Map()) }));

/**
 * @param {HTMLElement} root @param {HTMLButtonElement} pause @param {Map<string, HTMLTableCellElement>} cells
 * @param {string} feed @param {AbortSignal} signal
 * @param {HTMLOutputElement} updates @param {Message<"count">} updated the count's text, in the page's locale
 */
export function watch(root, pause, cells, feed, signal, updates, updated) {
  const s = prices(root);
  pause.hidden = false;
  /** @param {Stock} stock */
  const show = (stock) => {
    s.latest.set(stock.id, stock);
    if (!s.paused) paint(cells, stock);
  };
  subscribe(
    feed,
    {
      /** @param {Stock[]} rows */
      snapshot: (rows) => rows.forEach(show),
      patch: (changes) => {
        for (const change of changes) if (change.op === "upsert") show(change.value);
        s.updates += changes.length;
        updates.textContent = updated({ count: s.updates });
      },
      status: (status) => {
        root.dataset.status = status;
      },
    },
    { signal },
  );
}

/** @param {HTMLElement} root @param {HTMLButtonElement} button @param {Map<string, HTMLTableCellElement>} cells */
export function pause(root, button, cells) {
  const s = prices(root);
  s.paused = !s.paused;
  button.setAttribute("aria-pressed", String(s.paused));
  if (!s.paused) for (const stock of s.latest.values()) paint(cells, stock);
}

/** @param {Map<string, HTMLTableCellElement>} cells @param {Stock} stock */
function paint(cells, stock) {
  const cell = cells.get(stock.id);
  if (cell && cell.textContent !== stock.price) cell.textContent = stock.price;
}
