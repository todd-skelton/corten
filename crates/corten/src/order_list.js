// @ts-check
// The script of OrderList, in the RowsOf docs (script.rs). An app imports Holds and OrderRow from
// "corten/views"; these local stand-ins keep the doctest's script self-contained.

/** @template V, E @typedef {E & { readonly view?: V }} Holds */
/** @typedef {{ id: number }} OrderRow */

/** @param {Holds<OrderRow, HTMLTableSectionElement>} body */
export function clear(body) {
  body.textContent = "";
}
