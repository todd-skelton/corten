// @ts-check
// InventoryGrid (inventory_grid.rs): selection with ranges and select all, filter, sort,
// archiving through a confirm dialog, and live stock levels. The selection is the checkboxes
// themselves; the grid keeps only the last clicked row, for shift-click ranges.
import { state, subscribe } from "corten";

/** @typedef {import("../../types/app").Level} Level Generated from `Level` in src/app.rs. */

/**
 * What the selection handlers read and update. The markup builds it once,
 * `js::Selection { rows: row, boxes: select, all, count, bar }`, and passes it as one argument.
 * @typedef {object} Selection
 * @property {HTMLTableRowElement[]} rows
 * @property {HTMLInputElement[]} boxes
 * @property {HTMLInputElement} all
 * @property {HTMLOutputElement} count
 * @property {HTMLDivElement} bar
 */

/** @param {HTMLElement} root */
const last = (root) => state(root, () => ({ index: -1 }));

/**
 * Keeps the stock cells current from the levels feed until the grid leaves the page.
 * @param {Map<string, HTMLTableCellElement>} cells @param {string} feed @param {AbortSignal} signal
 */
export function live(cells, feed, signal) {
  /** @param {Level} level */
  const show = (level) => {
    const cell = cells.get(level.id);
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
 * A click on a row's checkbox (the one handler on the table body; it runs only for clicks on a
 * checkbox). With shift, every visible row between it and the last one follows.
 * @param {MouseEvent} event @param {HTMLInputElement} box @param {HTMLElement} root @param {Selection} sel
 */
export function toggle(event, box, root, sel) {
  const at = sel.boxes.indexOf(box);
  const s = last(root);
  if (event.shiftKey && s.index >= 0 && at >= 0) {
    const [from, to] = at < s.index ? [at, s.index] : [s.index, at];
    for (let i = from; i <= to; i += 1) if (!sel.rows[i].hidden) mark(sel.rows[i], sel.boxes[i], box.checked);
  } else if (at >= 0) {
    mark(sel.rows[at], box, box.checked);
  }
  s.index = at;
  tally(sel);
}

/**
 * Selects (or clears) every visible row.
 * @param {HTMLElement} root @param {Selection} sel
 */
export function selectAll(root, sel) {
  for (let i = 0; i < sel.rows.length; i += 1) if (!sel.rows[i].hidden) mark(sel.rows[i], sel.boxes[i], sel.all.checked);
  last(root).index = -1;
  tally(sel);
}

/** @param {HTMLElement} root @param {Selection} sel */
export function clear(root, sel) {
  for (let i = 0; i < sel.rows.length; i += 1) mark(sel.rows[i], sel.boxes[i], false);
  last(root).index = -1;
  tally(sel);
}

/** Lowercased row text, read once per row. @type {WeakMap<HTMLTableRowElement, string>} */
const texts = new WeakMap();

/** @param {HTMLInputElement} input @param {HTMLParagraphElement} empty @param {Selection} sel */
export function filter(input, empty, sel) {
  const query = input.value.trim().toLocaleLowerCase();
  let shown = 0;
  for (const row of sel.rows) {
    let text = texts.get(row);
    if (text === undefined) texts.set(row, (text = `${row.cells[1].textContent} ${row.cells[2].textContent}`.toLocaleLowerCase()));
    row.hidden = query !== "" && !text.includes(query);
    if (!row.hidden) shown += 1;
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
 * Posts the selected ids; the server answers by removing their rows and with a status line.
 * @param {HTMLElement} root @param {HTMLFormElement} form @param {HTMLInputElement} ids @param {Selection} sel
 */
export function archive(root, form, ids, sel) {
  ids.value = sel.boxes.filter((box) => box.checked).map((box) => box.value).join(",");
  form.requestSubmit();
  clear(root, sel);
}

/** @param {HTMLTableRowElement} row @param {HTMLInputElement} box @param {boolean} on */
function mark(row, box, on) {
  box.checked = on;
  row.setAttribute("aria-selected", String(on));
}

/**
 * The count, the bulk bar, and the select-all box: checked when every visible row is,
 * indeterminate when some are.
 * @param {Selection} sel
 */
function tally({ rows, boxes, all, count, bar }) {
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
