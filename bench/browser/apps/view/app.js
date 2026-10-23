// @ts-check
// Plain JavaScript as the framework: handlers write what they changed, by name, through the Row
// view that #[component(view)] generates from the Rust component (gen/src/main.rs). No state, no
// renders.
import { mount } from "corten/state";
import { rows } from "corten/view";
import { Row } from "corten/views";
import { label, nextId, patched } from "../shared/labels.js";

/** @param {number} n */
const build = (n) => Array.from({ length: n }, () => ({ id: nextId(), label: label(), selected: false }));

/** @param {{ refs: Record<string, Element> }} ctx */
function bench({ refs }) {
  const list = rows(refs.body, Row, /** @type {HTMLTemplateElement} */ (refs.row));
  /** @type {Row | undefined} */
  let selected;

  return {
    create: () => list.replace(build(1000)),
    create10k: () => list.replace(build(10000)),
    append: () => list.push(build(1000)),
    update: () => {
      for (let i = 0; i < list.length; i += 10) list.at(i).label += " !!!";
    },
    swap: () => list.swap(1, 998),
    clear: () => list.clear(),
    patch1: () => {
      list.at(list.length >> 1).label += " *";
    },
    patch100: () => {
      for (const i of patched(list.length)) list.at(i).label += " *";
    },
    /** @param {Event} event */
    select: (event) => {
      if (selected) selected.selected = false;
      selected = list.of(event.target);
      if (selected) selected.selected = true;
    },
    /** @param {Event} event */
    remove: (event) => {
      const row = list.of(event.target);
      if (row) list.remove(row);
    },
  };
}

mount(/** @type {Element} */ (document.getElementById("main")), bench);
document.documentElement.dataset.ready = String(performance.now());
