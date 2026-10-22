// @ts-check
// InventoryGrid (inventory_grid.rs): selection with ranges and select all, filter, sort,
// archiving through a confirm dialog, and live stock levels. The selection is the checkboxes
// themselves; the grid keeps only the last toggled row, for shift-click ranges.
import { state, subscribe } from "corten";

/** @typedef {import("corten/events").RowToggled} RowToggled */
/** @typedef {import("../../types/app").Level} Level Generated from `Level` in src/app.rs. */

/** @param {HTMLElement} root */
const last = (root) => state(root, () => ({ index: -1 }));

/**
 * Keeps the stock cells current from the levels feed until the grid leaves the page.
 * @param {HTMLTableCellElement[]} cells @param {string} feed @param {AbortSignal} signal
 */
export function live(cells, feed, signal) {
  const byId = new Map(cells.map((cell) => [cell.dataset.id, cell]));
  /** @param {Level} level */
  const show = (level) => {
    const cell = byId.get(level.id);
    const text = String(level.qty);
    if (cell && cell.textContent !== text) cell.textContent = text;
  };
  subscribe(
    feed,
    {
      /** @param {Level[]} rows */
      snapshot: (rows) => rows.forEach(show),
      patch: (changes) => {
        for (const change of changes) if (change.op === "upsert") show(change.value);
      },
    },
    { signal },
  );
}

/**
 * A row's checkbox changed; with shift, every visible row between it and the last one follows.
 * @param {RowToggled} event @param {HTMLElement} root @param {HTMLTableRowElement[]} rows
 * @param {HTMLInputElement[]} boxes @param {HTMLInputElement} all @param {HTMLOutputElement} count @param {HTMLDivElement} bar
 */
export function toggled(event, root, rows, boxes, all, count, bar) {
  const at = boxes.indexOf(/** @type {HTMLInputElement} */ (event.target));
  const s = last(root);
  if (event.shift && s.index >= 0 && at >= 0) {
    const [from, to] = at < s.index ? [at, s.index] : [s.index, at];
    for (let i = from; i <= to; i += 1) if (!rows[i].hidden) mark(rows[i], boxes[i], event.selected);
  }
  s.index = at;
  tally(rows, boxes, all, count, bar);
}

/**
 * Selects (or clears) every visible row.
 * @param {HTMLElement} root @param {HTMLTableRowElement[]} rows @param {HTMLInputElement[]} boxes
 * @param {HTMLInputElement} all @param {HTMLOutputElement} count @param {HTMLDivElement} bar
 */
export function selectAll(root, rows, boxes, all, count, bar) {
  for (let i = 0; i < rows.length; i += 1) if (!rows[i].hidden) mark(rows[i], boxes[i], all.checked);
  last(root).index = -1;
  tally(rows, boxes, all, count, bar);
}

/**
 * @param {HTMLElement} root @param {HTMLInputElement[]} boxes @param {HTMLTableRowElement[]} rows
 * @param {HTMLInputElement} all @param {HTMLOutputElement} count @param {HTMLDivElement} bar
 */
export function clear(root, boxes, rows, all, count, bar) {
  for (let i = 0; i < rows.length; i += 1) mark(rows[i], boxes[i], false);
  last(root).index = -1;
  tally(rows, boxes, all, count, bar);
}

/** Lowercased row text, read once per row. @type {WeakMap<HTMLTableRowElement, string>} */
const texts = new WeakMap();

/**
 * @param {HTMLInputElement} input @param {HTMLTableRowElement[]} rows @param {HTMLParagraphElement} empty
 * @param {HTMLInputElement[]} boxes @param {HTMLInputElement} all @param {HTMLOutputElement} count @param {HTMLDivElement} bar
 */
export function filter(input, rows, empty, boxes, all, count, bar) {
  const query = input.value.trim().toLocaleLowerCase();
  let shown = 0;
  for (const row of rows) {
    let text = texts.get(row);
    if (text === undefined) texts.set(row, (text = `${row.cells[1].textContent} ${row.cells[2].textContent}`.toLocaleLowerCase()));
    row.hidden = query !== "" && !text.includes(query);
    if (!row.hidden) shown += 1;
  }
  empty.hidden = shown > 0;
  tally(rows, boxes, all, count, bar);
}

/**
 * Sorts by the column, ascending then descending, numbers as numbers.
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
  const rows = Array.from(body.rows).map((row) => ({ row, key: numeric ? Number(key(row)) : key(row) }));
  rows.sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0));
  if (descending) rows.reverse();
  body.append(...rows.map((r) => r.row));
}

/** @param {HTMLDialogElement} dialog */
export function confirm(dialog) {
  dialog.showModal();
}

/**
 * Posts the selected ids; the server answers with the archived rows (hidden) and a status line.
 * @param {HTMLElement} root @param {HTMLInputElement[]} boxes @param {HTMLTableRowElement[]} rows @param {HTMLInputElement} all
 * @param {HTMLFormElement} form @param {HTMLInputElement} ids @param {HTMLOutputElement} count @param {HTMLDivElement} bar
 */
export function archive(root, boxes, rows, all, form, ids, count, bar) {
  ids.value = boxes.filter((box) => box.checked).map((box) => box.value).join(",");
  form.requestSubmit();
  clear(root, boxes, rows, all, count, bar);
}

/** @param {HTMLTableRowElement} row @param {HTMLInputElement} box @param {boolean} on */
function mark(row, box, on) {
  box.checked = on;
  row.setAttribute("aria-selected", String(on));
}

/**
 * The count, the bulk bar, and the select-all box: checked when every visible row is,
 * indeterminate when some are.
 * @param {HTMLTableRowElement[]} rows @param {HTMLInputElement[]} boxes @param {HTMLInputElement} all
 * @param {HTMLOutputElement} count @param {HTMLDivElement} bar
 */
function tally(rows, boxes, all, count, bar) {
  let n = 0;
  let visible = 0;
  let visibleChecked = 0;
  for (let i = 0; i < boxes.length; i += 1) {
    if (boxes[i].checked) n += 1;
    if (!rows[i].hidden) {
      visible += 1;
      if (boxes[i].checked) visibleChecked += 1;
    }
  }
  count.value = String(n);
  bar.hidden = n === 0;
  all.checked = visible > 0 && visibleChecked === visible;
  all.indeterminate = visibleChecked > 0 && visibleChecked < visible;
}
