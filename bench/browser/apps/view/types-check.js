// @ts-check
// Type-checked with the generated declarations (jsconfig.json), never bundled: what the compiler
// catches when a script and the Rust component disagree.
import { Card, Row, rows } from "corten/views";

/** @param {Element} body @param {HTMLTemplateElement} template */
export function check(body, template) {
  const list = rows(body, Row, template);
  const row = list.at(0);
  row.label = "ok";
  row.id = 2;
  row.selected = true;
  // @ts-expect-error: Row has no property lable (a typo of label)
  row.lable = "typo";
  // @ts-expect-error: label is text, not a number
  row.label = 3;
  // @ts-expect-error: a row's data has every hole, so a new row cannot leave selected unset
  list.push([{ id: 1, label: "new" }]);
  // @ts-expect-error: a row's refs are not its data (only holes are)
  list.push([{ id: 1, label: "new", selected: false, select: null }]);
  // @ts-expect-error: a Card's data is its own holes (symbol, price, down), not a Row's
  rows(body, Card).push([{ id: 1, label: "new", selected: false }]);
}
