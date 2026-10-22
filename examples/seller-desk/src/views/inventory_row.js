// @ts-check
// InventoryRow (inventory_row.rs): a row tells the grid its checkbox changed; the grid keeps
// the selection and the count.
import { RowToggled } from "corten/events";

/** @param {MouseEvent} event @param {HTMLInputElement} box @param {HTMLElement} row @param {string} id */
export function toggle(event, box, row, id) {
  row.setAttribute("aria-selected", String(box.checked));
  box.dispatchEvent(new RowToggled({ id, selected: box.checked, shift: event.shiftKey }));
}
