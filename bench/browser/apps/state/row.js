// @ts-check
// What html! would generate from a Rust `Row` component used in a client list: its markup is in
// the page as <template id="row">, and this finds and fills its holes.
import { template } from "corten/state";

/** @typedef {{ id: number, label: string }} RowData */

export const Row = template(
  /** @type {HTMLTemplateElement} */ (document.getElementById("row")),
  (tr) => [/** @type {Text} */ (tr.firstChild?.firstChild), /** @type {Text} */ (tr.firstChild?.nextSibling?.firstChild?.firstChild)],
  /** @param {[Text, Text]} h @param {RowData} row @param {RowData | undefined} before */
  (h, row, before) => {
    if (row.id !== before?.id) h[0].data = String(row.id);
    if (row.label !== before?.label) h[1].data = row.label;
  },
);
