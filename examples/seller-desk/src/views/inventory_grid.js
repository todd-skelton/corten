// @ts-check
// InventoryGrid (inventory_grid.rs): selection with ranges and select all, filter, sort,
// archiving through a confirm dialog (opened by its button's commandfor, no script), and live
// stock levels. The rows are InventoryRow views: a handler reads and writes a row's holes by name
// (`row.selected = true`, `row.qty = 3`, `row.name`), and the page holds the selection. The
// grid keeps only the last clicked row, for shift-click ranges.
import { keep, subscribe } from "corten";
import { InventoryRow, rows } from "corten/views";
/** @import { Message } from "corten" */
/** @import { Holds } from "corten/views" */
/** @import { PriceEdited } from "corten/events" */

/** @typedef {import("../../types/app").Level} Level Generated from `Level` in src/app.rs. */

/**
 * What the selection handlers read and update. The markup builds it once,
 * `js::Selection { body, all, count, bar }`, and passes it as one argument.
 * @typedef {object} Selection
 * @property {Holds<InventoryRow, HTMLTableSectionElement>} body
 * @property {HTMLInputElement} all
 * @property {HTMLOutputElement} count
 * @property {HTMLDivElement} bar
 */

/**
 * The note that counts prices typed and not saved, and its text in the page's locale: one
 * message per plural form. The markup builds it once, `js::Unsent { note, one, other }`.
 * @typedef {object} Unsent
 * @property {HTMLOutputElement} note
 * @property {Message<"count">} one
 * @property {Message<"count">} other
 */

/**
 * What the grid keeps between events: the last clicked row, for shift-click ranges, and the rows
 * whose price was typed and not saved. One object, since a component has one state.
 */
const kept = () => keep(() => ({ index: -1, unsent: /** @type {Set<HTMLTableRowElement>} */ (new Set()) }));

/**
 * Keeps the stock cells current from the levels feed until the grid leaves the page.
 * @param {Holds<InventoryRow, HTMLTableSectionElement>} body @param {string} feed @param {AbortSignal} signal
 */
export function live(body, feed, signal) {
  const list = rows(body, InventoryRow);
  /** @param {Level} level */
  const show = (level) => {
    const row = list.get(level.id);
    if (row) row.qty = level.qty;
  };
  subscribe(
    feed,
    {
      /** @param {Level[]} levels */
      snapshot: (levels) => levels.forEach(show),
      patch: (changes) => {
        for (const change of changes) if (change.op === "upsert") show(change.value);
      },
    },
    { signal },
  );
}

/**
 * A click on a row's checkbox (the one handler on the table body; it runs only for clicks on a
 * checkbox). With shift, every visible row between it and the last one follows.
 * @param {MouseEvent} event @param {HTMLInputElement} box @param {Selection} sel
 */
export function toggle(event, box, sel) {
  const list = rows(sel.body, InventoryRow);
  const row = list.of(box);
  if (!row) return;
  const at = list.indexOf(row);
  const s = kept();
  if (event.shiftKey && s.index >= 0) {
    const [from, to] = at < s.index ? [at, s.index] : [s.index, at];
    for (let i = from; i <= to; i += 1) {
      const other = list.at(i);
      if (!other.el.hidden) other.selected = box.checked;
    }
  } else {
    row.selected = box.checked;
  }
  s.index = at;
  tally(sel);
}

/**
 * Selects (or clears) every visible row.
 * @param {Selection} sel
 */
export function selectAll(sel) {
  for (const row of rows(sel.body, InventoryRow)) if (!row.el.hidden) row.selected = sel.all.checked;
  kept().index = -1;
  tally(sel);
}

/** @param {Selection} sel */
export function clear(sel) {
  for (const row of rows(sel.body, InventoryRow)) row.selected = false;
  kept().index = -1;
  tally(sel);
}

/** Lowercased name and set, read once per row. @type {WeakMap<Element, string>} */
const texts = new WeakMap();

/**
 * The price a row shows, in cents, or Infinity when it is not a number (a refused edit), so it
 * sorts after every price (before them, descending). The price is its own component, PriceCell, which the server patches, so it is read
 * from the cell's input: the grid sorts what the seller sees.
 * @param {InventoryRow} row
 */
function shownCents(row) {
  const input = row.el.querySelector("input:not([type])");
  const cents = input instanceof HTMLInputElement ? Math.round(Number(input.value) * 100) : NaN;
  return Number.isFinite(cents) ? cents : Infinity;
}

/** @param {HTMLInputElement} input @param {HTMLParagraphElement} empty @param {Selection} sel */
export function filter(input, empty, sel) {
  const query = input.value.trim().toLocaleLowerCase();
  let shown = 0;
  for (const row of rows(sel.body, InventoryRow)) {
    let text = texts.get(row.el);
    if (text === undefined) texts.set(row.el, (text = `${row.name} ${row.set}`.toLocaleLowerCase()));
    row.el.hidden = query !== "" && !text.includes(query);
    if (!row.el.hidden) shown += 1;
  }
  empty.hidden = shown > 0;
  tally(sel);
}

// One handler per column, so the markup names a column the compiler knows.

/** @param {HTMLButtonElement} button @param {Holds<InventoryRow, HTMLTableSectionElement>} body */
export function sortByName(button, body) {
  sort(button, body, "name");
}

/** @param {HTMLButtonElement} button @param {Holds<InventoryRow, HTMLTableSectionElement>} body */
export function sortBySet(button, body) {
  sort(button, body, "set");
}

/** @param {HTMLButtonElement} button @param {Holds<InventoryRow, HTMLTableSectionElement>} body */
export function sortByQty(button, body) {
  sort(button, body, "qty");
}

/** @param {HTMLButtonElement} button @param {Holds<InventoryRow, HTMLTableSectionElement>} body */
export function sortByPrice(button, body) {
  sort(button, body, "price");
}

/**
 * Sorts the rows by one of their fields, ascending then descending: text in the page's
 * language's order, numbers as numbers.
 * @param {HTMLButtonElement} button @param {Holds<InventoryRow, HTMLTableSectionElement>} body
 * @param {"name" | "set" | "qty" | "price"} by
 */
function sort(button, body, by) {
  const th = /** @type {HTMLTableCellElement} */ (button.closest("th"));
  const descending = th.getAttribute("aria-sort") === "ascending";
  for (const other of Array.from(th.parentElement?.children ?? [])) other.removeAttribute("aria-sort");
  th.setAttribute("aria-sort", descending ? "descending" : "ascending");
  // Each row's key is read once, not at every comparison.
  const list = Array.from(rows(body, InventoryRow));
  /** @type {HTMLTableRowElement[]} */
  let sorted;
  if (by === "name" || by === "set") {
    const text = new Intl.Collator(document.documentElement.lang || undefined);
    const keyed = list.map((row) => ({ el: row.el, key: row[by] }));
    sorted = keyed.sort((a, b) => text.compare(a.key, b.key)).map((r) => r.el);
  } else {
    const keyed = list.map((row) => ({ el: row.el, key: by === "qty" ? row.qty : shownCents(row) }));
    // Infinity - Infinity is NaN: two unreadable prices are equal.
    sorted = keyed.sort((a, b) => (a.key === b.key ? 0 : a.key - b.key)).map((r) => r.el);
  }
  if (descending) sorted.reverse();
  body.append(...sorted);
}

/**
 * Posts the checked rows (their boxes belong to the form); the server answers by removing their
 * rows and with a status line. The form's data is read as it submits, so clearing after is safe.
 * @param {HTMLFormElement} form @param {Selection} sel
 */
export function archive(form, sel) {
  form.requestSubmit();
  clear(sel);
}

/**
 * A price cell says whether its price is not saved yet (it bubbles up from the cell's input).
 * @param {PriceEdited} event
 * @param {Holds<InventoryRow, HTMLTableSectionElement>} body @param {Unsent} note
 */
export function priceEdited(event, body, note) {
  const row = rows(body, InventoryRow).of(event.target);
  if (!row) return;
  const unsent = kept().unsent;
  if (event.unsent) unsent.add(row.el);
  else unsent.delete(row.el);
  showUnsent(unsent, note);
}

/**
 * A form in the grid was sent: when it is a row's price, the price is on its way. The server
 * answers with the cell, or the cell with its error.
 * @param {SubmitEvent} event
 * @param {Holds<InventoryRow, HTMLTableSectionElement>} body @param {Unsent} note
 */
export function priceSent(event, body, note) {
  const row = rows(body, InventoryRow).of(event.target);
  if (!row) return;
  const unsent = kept().unsent;
  unsent.delete(row.el);
  showUnsent(unsent, note);
}

/** @param {Set<HTMLTableRowElement>} unsent @param {Unsent} note */
function showUnsent(unsent, { note, one, other }) {
  // An archived row has left the page, and its edit with it.
  for (const el of unsent) if (!el.isConnected) unsent.delete(el);
  note.value = (unsent.size === 1 ? one : other)({ count: unsent.size });
  note.hidden = unsent.size === 0;
}

/**
 * The count, the bulk bar, and the select-all box: checked when every visible row is,
 * indeterminate when some are.
 * @param {Selection} sel
 */
function tally({ body, all, count, bar }) {
  let n = 0;
  let visible = 0;
  let visibleChecked = 0;
  for (const row of rows(body, InventoryRow)) {
    const on = row.selected;
    if (on) n += 1;
    if (!row.el.hidden) {
      visible += 1;
      if (on) visibleChecked += 1;
    }
  }
  count.value = String(n);
  bar.hidden = n === 0;
  all.checked = visible > 0 && visibleChecked === visible;
  all.indeterminate = visibleChecked > 0 && visibleChecked < visible;
}
