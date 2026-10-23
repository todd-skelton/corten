// @ts-check
// Plain JavaScript, written for speed: the floor every framework is measured against.
import { label, nextId, patched } from "../shared/labels.js";

const tbody = /** @type {HTMLTableSectionElement} */ (document.getElementById("tbody"));
const proto = /** @type {HTMLTableRowElement} */ (/** @type {HTMLTemplateElement} */ (document.getElementById("row")).content.firstElementChild);
/** Rows the server rendered (the server-rendered page), or none. @type {{ id: number, label: string }[]} */
let data = JSON.parse(document.getElementById("data")?.textContent ?? "[]");
/** @type {HTMLTableRowElement[]} */
let rows = /** @type {HTMLTableRowElement[]} */ (Array.from(tbody.children));
/** @type {HTMLTableRowElement | null} */
let selected = null;

/** @param {{ id: number, label: string }} d */
function make(d) {
  const tr = /** @type {HTMLTableRowElement} */ (proto.cloneNode(true));
  const td = /** @type {HTMLElement} */ (tr.firstChild);
  /** @type {Text} */ (td.firstChild).data = String(d.id);
  /** @type {Text} */ (/** @type {HTMLElement} */ (/** @type {HTMLElement} */ (td.nextSibling).firstChild).firstChild).data = d.label;
  return tr;
}
/** @param {number} i */
const labelOf = (i) => /** @type {Text} */ (/** @type {HTMLElement} */ (/** @type {HTMLElement} */ (rows[i].firstChild?.nextSibling).firstChild).firstChild);

/** @param {number} n */
function append(n) {
  const frag = document.createDocumentFragment();
  for (let i = 0; i < n; i += 1) {
    const d = { id: nextId(), label: label() };
    const tr = make(d);
    data.push(d);
    rows.push(tr);
    frag.appendChild(tr);
  }
  tbody.appendChild(frag);
}
function clear() {
  tbody.textContent = "";
  data = [];
  rows = [];
  selected = null;
}
/** @param {number} i @param {string} text */
function relabel(i, text) {
  data[i].label = text;
  labelOf(i).data = text;
}

/** @type {Record<string, () => void>} */
const ops = {
  create: () => (clear(), append(1000)),
  create10k: () => (clear(), append(10000)),
  append: () => append(1000),
  update: () => { for (let i = 0; i < data.length; i += 10) relabel(i, data[i].label + " !!!"); },
  swap: () => {
    if (rows.length > 998) {
      const a = rows[1], b = rows[998], after = b.nextSibling;
      tbody.insertBefore(b, a);
      tbody.insertBefore(a, after);
      [rows[1], rows[998]] = [b, a];
      [data[1], data[998]] = [data[998], data[1]];
    }
  },
  clear,
  patch1: () => { const i = data.length >> 1; relabel(i, data[i].label + " *"); },
  patch100: () => { for (const i of patched(data.length)) relabel(i, data[i].label + " *"); },
};
for (const [id, op] of Object.entries(ops)) document.getElementById(id)?.addEventListener("click", op);
tbody.addEventListener("click", (event) => {
  const a = /** @type {Element} */ (event.target).closest("a");
  if (!a) return;
  const tr = /** @type {HTMLTableRowElement} */ (a.closest("tr"));
  if (a.classList.contains("select")) {
    selected?.classList.remove("danger");
    tr.classList.add("danger");
    selected = tr;
  } else {
    const i = rows.indexOf(tr);
    tr.remove();
    rows.splice(i, 1);
    data.splice(i, 1);
  }
});
document.documentElement.dataset.ready = String(performance.now());
