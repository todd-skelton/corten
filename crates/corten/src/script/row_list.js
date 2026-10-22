// @ts-check
/** @param {Event} event @param {HTMLTableRowElement[]} rows */
export function count(event, rows) {
  console.log(event.type, rows.length);
}

/** @param {HTMLTableRowElement} row @param {string} id */
export function toggle(row, id) {
  row.toggleAttribute("aria-selected");
  console.log(id);
}
