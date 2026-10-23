// @ts-check
// What html! would generate from the Rust `Row` component used in a client list:
//
//   #[component] fn Row(id: u32, label: String, selected: bool) -> Markup {
//       html! { <tr class:danger={selected}><td>{id}</td><td><a on:click={js::select}>{label}</a></td> ... }
//   }
//
// One property per hole, read from and written to the DOM. The paths come from the markup.

/** @typedef {{ id: number, label: string }} RowData */

const proto = /** @type {HTMLTableRowElement} */ (/** @type {HTMLTemplateElement} */ (document.getElementById("row")).content.firstElementChild);
const idText = (/** @type {Element} */ el) => /** @type {Text} */ (el.firstChild?.firstChild);
const labelText = (/** @type {Element} */ el) => /** @type {Text} */ (el.firstChild?.nextSibling?.firstChild?.firstChild);

export class Row {
  /** @param {Element} el */
  constructor(el) {
    this.el = el;
  }
  get id() {
    return Number(idText(this.el).data);
  }
  get label() {
    return labelText(this.el).data;
  }
  set label(value) {
    const t = labelText(this.el);
    if (t.data !== value) t.data = value;
  }
  get selected() {
    return this.el.classList.contains("danger");
  }
  set selected(value) {
    this.el.classList.toggle("danger", value);
  }
  /** @param {RowData} data */
  static make(data) {
    const el = /** @type {Element} */ (proto.cloneNode(true));
    Row.fill(el, data);
    return el;
  }
  /** @param {Element} el @param {RowData} data */
  static fill(el, data) {
    idText(el).data = String(data.id);
    labelText(el).data = data.label;
  }
  /** @param {Element} el */
  static key(el) {
    return idText(el).data;
  }
  /** @param {RowData} data */
  static keyOf(data) {
    return String(data.id);
  }
}
