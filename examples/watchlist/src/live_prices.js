// @ts-check
// LivePrices (home.rs): keeps the server-rendered prices and today's change current from the
// feed, which carries the whole market; the rows are the watched stocks. While paused it keeps
// the latest rows and shows them on resume. Prices and changes are written with the page's
// formats (t.js().money(USD), t.js().change(2)), which write what the server rendered; they
// are written into the page once, for watch, which keeps them for pause.
import { keep, subscribe } from "corten";
/** @import { FeedUrl, Message, MoneyFormat, PercentFormat } from "corten" */

/** @typedef {import("../types/app").Stock} Stock Generated from `Stock` in src/market.rs. */

/**
 * The cells a stock's values go in, by ticker, and the formats they are written with.
 * @typedef {{ money: MoneyFormat, percent: PercentFormat }} Formats
 * @typedef {{ prices: Map<string, HTMLTableCellElement>, changes: Map<string, HTMLTableCellElement> }} Cells
 */

/** What the component keeps: whether it is paused, the latest rows, and watch's formats. */
const held = () =>
  keep(() => ({ paused: false, updates: 0, latest: /** @type {Map<string, Stock>} */ (new Map()), formats: /** @type {Formats | null} */ (null) }));

/**
 * @param {HTMLElement} root @param {HTMLButtonElement} pause
 * @param {Map<string, HTMLTableCellElement>} prices @param {Map<string, HTMLTableCellElement>} changes
 * @param {MoneyFormat} money @param {PercentFormat} percent @param {FeedUrl<Stock>} feed @param {AbortSignal} signal
 * @param {HTMLParagraphElement} updates @param {Message<"count">} updated the count's text, in the page's locale
 */
export function watch(root, pause, prices, changes, money, percent, feed, signal, updates, updated) {
  const s = held();
  const formats = (s.formats = { money, percent });
  /** @type {Cells} */
  const cells = { prices, changes };
  pause.hidden = false;
  /** @param {Stock} stock */
  const show = (stock) => {
    s.latest.set(stock.id, stock);
    if (!s.paused) paint(cells, formats, stock);
  };
  subscribe(
    feed,
    {
      snapshot: (rows) => rows.forEach(show),
      patch: (changes) => {
        for (const change of changes) {
          if (change.op !== "upsert") continue;
          show(change.value);
          // Only the stocks on the list count.
          if (prices.has(change.id)) s.updates += 1;
        }
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
 * @param {HTMLButtonElement} button
 * @param {Map<string, HTMLTableCellElement>} prices @param {Map<string, HTMLTableCellElement>} changes
 */
export function pause(button, prices, changes) {
  const s = held();
  s.paused = !s.paused;
  button.setAttribute("aria-pressed", String(s.paused));
  const formats = s.formats;
  if (!s.paused && formats) for (const stock of s.latest.values()) paint({ prices, changes }, formats, stock);
}

/**
 * `p` pauses or resumes wherever focus is on the page, unless the reader is typing in a field or
 * the key is part of another shortcut.
 * @param {KeyboardEvent} event @param {HTMLButtonElement} button
 * @param {Map<string, HTMLTableCellElement>} prices @param {Map<string, HTMLTableCellElement>} changes
 */
export function pauseKey(event, button, prices, changes) {
  if (event.key !== "p" || event.defaultPrevented || event.ctrlKey || event.altKey || event.metaKey || typing(event.target)) return;
  event.preventDefault();
  pause(button, prices, changes);
}

/** Whether a key goes into a field, or a dialog the reader has open. @param {EventTarget | null} target */
function typing(target) {
  return target instanceof Element && target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"]), dialog[open]') !== null;
}

/** A stock's price and change, when it is on the list. @param {Cells} cells @param {Formats} formats @param {Stock} stock */
function paint(cells, formats, stock) {
  const price = cells.prices.get(stock.id);
  const text = formats.money(stock.price_cents);
  if (price && price.textContent !== text) price.textContent = text;
  const change = cells.changes.get(stock.id);
  if (!change) return;
  // The ratio as text (a decimal), so it is written exactly: 35 basis points is "0.0035".
  const ratio = formats.percent(String(stock.change_bp / 10000));
  if (change.textContent !== ratio) change.textContent = ratio;
  change.dataset.trend = stock.change_bp > 0 ? "up" : stock.change_bp < 0 ? "down" : "flat";
}
