// @ts-check
// The proposal: a setup function with state({...}), handlers, and a render. No update() calls.
import { state, list, mount } from "corten/state";
import { Row } from "./row.js";
import { label, nextId, patched } from "../shared/labels.js";

/** @typedef {import("./row.js").RowData} RowData */
/** @param {number} n @returns {RowData[]} */
const build = (n) => Array.from({ length: n }, () => ({ id: nextId(), label: label() }));

/** Rows the server rendered (the server-rendered page), or none. @type {RowData[]} */
const initial = JSON.parse(document.getElementById("data")?.textContent ?? "[]");

/** @param {{ refs: Record<string, Element> }} ctx */
function bench({ refs }) {
  const s = state({ rows: initial, selected: 0 });
  const rows = list(refs.body, Row, (r) => r.id, initial); // adopts the server's rows
  /** @type {Element | undefined} */
  let marked; // the selected row's element: not state, nothing renders from it

  /** @param {number[]} at @param {string} suffix */
  const relabel = (at, suffix) => {
    const next = s.rows.slice();
    for (const i of at) next[i] = { ...next[i], label: next[i].label + suffix };
    s.rows = next;
  };

  return {
    render() {
      rows.set(s.rows);
      const node = rows.node(s.selected);
      if (node !== marked) {
        marked?.classList.remove("danger");
        node?.classList.add("danger");
        marked = node;
      }
    },
    create: () => (s.rows = build(1000)),
    create10k: () => (s.rows = build(10000)),
    append: () => (s.rows = s.rows.concat(build(1000))),
    update: () => (s.rows = s.rows.map((r, i) => (i % 10 ? r : { ...r, label: r.label + " !!!" }))),
    swap: () => {
      if (s.rows.length < 999) return;
      const next = s.rows.slice();
      [next[1], next[998]] = [next[998], next[1]];
      s.rows = next;
    },
    clear: () => (s.rows = []),
    patch1: () => relabel([s.rows.length >> 1], " *"),
    patch100: () => relabel(patched(s.rows.length), " *"),
    /** @param {Event} event */
    select: (event) => (s.selected = rows.item(event.target)?.id ?? 0),
    /** @param {Event} event */
    remove: (event) => {
      const row = rows.item(event.target);
      s.rows = s.rows.filter((r) => r !== row);
    },
  };
}

mount(/** @type {Element} */ (document.getElementById("main")), bench);
document.documentElement.dataset.ready = String(performance.now());
