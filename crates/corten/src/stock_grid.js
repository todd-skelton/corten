// @ts-check
import { RowToggled } from "corten/events";

/**
 * What the selection handlers update.
 * @typedef {object} Selection
 * @property {HTMLTableRowElement[]} rows
 * @property {HTMLOutputElement} count
 */

/** @param {HTMLTableRowElement} row */
export function pick(row) {
  row.toggleAttribute("aria-selected");
}

/** @param {HTMLTableRowElement} row @param {Selection} sel */
export function toggle(row, sel) {
  row.toggleAttribute("aria-selected");
  sel.count.value = String(sel.rows.filter((r) => r.hasAttribute("aria-selected")).length);
}

/** @param {Map<string, HTMLTableCellElement>} cells @param {string} id */
export function show(cells, id) {
  cells.get(id)?.focus();
}

/** @param {RowToggled} event @param {Selection} sel */
export function toggled(event, sel) {
  sel.count.value = event.id;
  if (event.selected) sel.rows[0]?.focus();
}

/** @param {HTMLTableRowElement} row */
export function announce(row) {
  row.dispatchEvent(new RowToggled({ id: row.id, selected: true }));
}
