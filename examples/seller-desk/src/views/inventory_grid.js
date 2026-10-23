// @ts-check
// InventoryGrid (inventory_grid.rs): selection with ranges and select all, filter, sort,
// archiving through a confirm dialog, and live stock levels. The rows are InventoryRow views:
// a handler writes a row's holes by name (`row.selected = true`, `row.qty = 3`), and the page
// holds the selection. The grid keeps only the last clicked row, for shift-click ranges.
import { state, subscribe } from "corten";
import { InventoryRow, rows } from "corten/views";
/** @import { Holds } from "corten/views" */

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

/** @param {HTMLElement} root */
const last = (root) => state(root, () => ({ index: -1 }));

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
 * @param {MouseEvent} event @param {HTMLInputElement} box @param {HTMLElement} root @param {Selection} sel
 */
export function toggle(event, box, root, sel) {
  const list = rows(sel.body, InventoryRow);
  const row = list.of(box);
  if (!row) return;
  const at = list.indexOf(row);
  const s = last(root);
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
 * @param {HTMLElement} root @param {Selection} sel
 */
export function selectAll(root, sel) {
  for (const row of rows(sel.body, InventoryRow)) if (!row.el.hidden) row.selected = sel.all.checked;
  last(root).index = -1;
  tally(sel);
}

/** @param {HTMLElement} root @param {Selection} sel */
export function clear(root, sel) {
  for (const row of rows(sel.body, InventoryRow)) row.selected = false;
  last(root).index = -1;
  tally(sel);
}

/** Lowercased name and set, read once per row. @type {WeakMap<Element, string>} */
const texts = new WeakMap();

/** @param {HTMLInputElement} input @param {HTMLParagraphElement} empty @param {Selection} sel */
export function filter(input, empty, sel) {
  const query = input.value.trim().toLocaleLowerCase();
  let shown = 0;
  for (const row of rows(sel.body, InventoryRow)) {
    let text = texts.get(row.el);
    if (text === undefined) texts.set(row.el, (text = `${row.el.cells[1].textContent} ${row.set}`.toLocaleLowerCase()));
    row.el.hidden = query !== "" && !text.includes(query);
    if (!row.el.hidden) shown += 1;
  }
  empty.hidden = shown > 0;
  tally(sel);
}

/**
 * Sorts the rows by the button's column, ascending then descending, numbers as numbers.
 * @param {HTMLButtonElement} button @param {HTMLTableSectionElement} body @param {number} column
 */
export function sort(button, body, column) {
  const th = /** @type {HTMLTableCellElement} */ (button.closest("th"));
  const descending = th.getAttribute("aria-sort") === "ascending";
  for (const other of Array.from(th.parentElement?.children ?? [])) other.removeAttribute("aria-sort");
  th.setAttribute("aria-sort", descending ? "descending" : "ascending");
  const key = (/** @type {HTMLTableRowElement} */ row) => {
    const cell = row.cells[column];
    return cell.querySelector("input")?.value ?? cell.textContent ?? "";
  };
  const numeric = column >= 4;
  const sorted = Array.from(body.rows).map((row) => ({ row, key: numeric ? Number(key(row)) : key(row) }));
  sorted.sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0));
  if (descending) sorted.reverse();
  body.append(...sorted.map((r) => r.row));
}

/** @param {HTMLDialogElement} dialog */
export function confirm(dialog) {
  dialog.showModal();
}

/**
 * Posts the selected ids; the server answers by removing their rows and with a status line.
 * @param {HTMLElement} root @param {HTMLFormElement} form @param {HTMLInputElement} ids @param {Selection} sel
 */
export function archive(root, form, ids, sel) {
  const picked = [];
  for (const row of rows(sel.body, InventoryRow)) if (row.selected) picked.push(InventoryRow.key(row.el));
  ids.value = picked.join(",");
  form.requestSubmit();
  clear(root, sel);
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
