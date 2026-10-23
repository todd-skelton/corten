// @ts-check
// Type-checked with the generated declarations (jsconfig.json), never bundled: what the compiler
// catches when a script and the Rust component disagree.
import { Card, Row, rows } from "corten/views";

/** @import { Holds } from "corten/views" */

/**
 * @param {Holds<Row, HTMLTableSectionElement>} body @param {Holds<Row, HTMLTemplateElement>} template
 * @param {HTMLTableSectionElement} plain @param {Holds<Card, HTMLDivElement>} cards
 * @param {Holds<Fixed, HTMLTableSectionElement>} fixed
 */
export function check(body, template, plain, cards, fixed) {
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
  rows(cards, Card).push([{ id: 1, label: "new", selected: false }]);
  // @ts-expect-error: the body holds Rows, not Cards (rows(body, Card) is the wrong list)
  rows(body, Card);
  // @ts-expect-error: a parameter typed only as an element does not say which view its rows are
  rows(plain, Row);
  // @ts-expect-error: the template's one row is a Row, not a Card
  rows(cards, Card, template);
  const served = rows(fixed, Fixed);
  served.get("a")?.el.remove();
  // @ts-expect-error: a view whose key is also shown outside its holes has rows the server makes
  served.push([{ n: 1 }]);
  // @ts-expect-error: nor can a script replace them
  served.replace([]);
}

/** What #[component(view)] makes when the key is also shown outside the holes: no keyOf. */
class Fixed {
  /** @param {Element} el */
  constructor(el) {
    this.el = el;
  }
  /** @param {Element} el @param {{ n: number }} data */
  static fill(el, data) {
    el.textContent = String(data.n);
  }
  /** @param {Element} el */
  static key(el) {
    return el.getAttribute("data-key") ?? "";
  }
}
