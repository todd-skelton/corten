// @ts-check
// The bench's handlers (Bench in main.rs): plain functions the markup calls with its elements. They
// change the page through the generated Row view, by name (`row.label += " !!!"`), and through
// rows(), the list of a parent's Row views. No state, no renders.
import { state } from "corten";
import { Row, rows } from "corten/views";
import { label, nextId, patched } from "../../apps/shared/labels.js";
/** @import { Holds } from "corten/views" */

/** @param {number} n */
const build = (n) => Array.from({ length: n }, () => ({ id: nextId(), label: label(), selected: false }));

/** Marks the page ready, for the harness: the runtime runs on:connect once this module is in. */
export function ready() {
  document.documentElement.dataset.ready = String(performance.now());
}

/** @param {Holds<Row, HTMLTableSectionElement>} body @param {Holds<Row, HTMLTemplateElement>} template @param {number} n */
export function create(body, template, n) {
  rows(body, Row, template).replace(build(n));
}

/** @param {Holds<Row, HTMLTableSectionElement>} body @param {Holds<Row, HTMLTemplateElement>} template */
export function append(body, template) {
  rows(body, Row, template).push(build(1000));
}

/** @param {Holds<Row, HTMLTableSectionElement>} body */
export function update(body) {
  const list = rows(body, Row);
  for (let i = 0; i < list.length; i += 10) list.at(i).label += " !!!";
}

/** @param {Holds<Row, HTMLTableSectionElement>} body */
export function swap(body) {
  rows(body, Row).swap(1, 998);
}

/** @param {Holds<Row, HTMLTableSectionElement>} body */
export function clear(body) {
  rows(body, Row).clear();
}

/** @param {Holds<Row, HTMLTableSectionElement>} body */
export function patch1(body) {
  const list = rows(body, Row);
  list.at(list.length >> 1).label += " *";
}

/** @param {Holds<Row, HTMLTableSectionElement>} body */
export function patch100(body) {
  const list = rows(body, Row);
  for (const i of patched(list.length)) list.at(i).label += " *";
}

/**
 * The row whose label was clicked becomes the selected one.
 * @param {HTMLAnchorElement} link @param {Holds<Row, HTMLTableSectionElement>} body
 */
export function select(link, body) {
  const last = state(body, () => /** @type {{ row: Row | undefined }} */ ({ row: undefined }));
  if (last.row) last.row.selected = false;
  last.row = rows(body, Row).of(link);
  if (last.row) last.row.selected = true;
}

/** @param {HTMLAnchorElement} link @param {Holds<Row, HTMLTableSectionElement>} body */
export function remove(link, body) {
  const list = rows(body, Row);
  const row = list.of(link);
  if (row) list.remove(row);
}
