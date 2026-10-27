// @ts-check
// LivePrices (app.rs): keeps the server-rendered prices and today's change current from the
// feed. While paused it keeps the latest rows and shows them on resume. Prices and changes are
// written with the page's formats (t.js().money(USD), t.js().percent(2)), which write what the
// server rendered.
import { state, subscribe } from "corten";
/** @import { Message, MoneyFormat, PercentFormat } from "corten" */

/** @typedef {import("../types/app").Stock} Stock Generated from `Stock` in src/market.rs. */

/**
 * The cells a row's values go in, by ticker, and the formats they are written with.
 * @typedef {{ money: MoneyFormat, percent: PercentFormat }} Formats
 * @typedef {{ prices: Map<string, HTMLTableCellElement>, changes: Map<string, HTMLTableCellElement> } & Formats} Cells
 */

/**
 * What the component keeps: whether it is paused, the latest rows, and the formats watch was given.
 * @param {HTMLElement} root
 */
const held = (root) =>
  state(root, () => ({ paused: false, updates: 0, latest: /** @type {Map<string, Stock>} */ (new Map()), formats: /** @type {Formats | null} */ (null) }));

/**
 * @param {HTMLElement} root @param {HTMLButtonElement} pause
 * @param {Map<string, HTMLTableCellElement>} prices @param {Map<string, HTMLTableCellElement>} changes
 * @param {MoneyFormat} money @param {PercentFormat} percent @param {string} feed @param {AbortSignal} signal
 * @param {HTMLOutputElement} updates @param {Message<"count">} updated the count's text, in the page's locale
 */
export function watch(root, pause, prices, changes, money, percent, feed, signal, updates, updated) {
  const s = held(root);
  s.formats = { money, percent };
  /** @type {Cells} */
  const cells = { prices, changes, money, percent };
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

/**
 * The cells are the page's as of the click, so rows put in since watch ran are painted too.
 * @param {HTMLElement} root @param {HTMLButtonElement} button
 * @param {Map<string, HTMLTableCellElement>} prices @param {Map<string, HTMLTableCellElement>} changes
 */
export function pause(root, button, prices, changes) {
  const s = held(root);
  s.paused = !s.paused;
  button.setAttribute("aria-pressed", String(s.paused));
  const formats = s.formats;
  if (!s.paused && formats) for (const stock of s.latest.values()) paint({ prices, changes, ...formats }, stock);
}

/** A row's price and change, for a stock on the list (the feed has the whole market). @param {Cells} cells @param {Stock} stock */
function paint(cells, stock) {
  const price = cells.prices.get(stock.id);
  const text = cells.money(stock.price_cents);
  if (price && price.textContent !== text) price.textContent = text;
  const change = cells.changes.get(stock.id);
  if (!change) return;
  const ratio = cells.percent(stock.change_bp / 10000);
  if (change.textContent !== ratio) change.textContent = ratio;
  change.dataset.trend = stock.change_bp > 0 ? "up" : stock.change_bp < 0 ? "down" : "flat";
}
