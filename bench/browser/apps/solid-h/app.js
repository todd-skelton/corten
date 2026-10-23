// Solid without a compiler, through corten/solid's h, each and mount: what Corten ships today
// for client-heavy parts of a page.
import { h, each, mount, createSignal, createSelector, batch } from "corten/solid";
import { label, nextId, patched } from "../shared/labels.js";

const build = (n) => {
  const out = new Array(n);
  for (let i = 0; i < n; i++) {
    const [text, setText] = createSignal(label());
    out[i] = { id: nextId(), label: text, setLabel: setText };
  }
  return out;
};

function App() {
  const [data, setData] = createSignal([]);
  const [selected, setSelected] = createSignal(0);
  const isSelected = createSelector(selected);
  const relabel = (at, suffix) => batch(() => { const d = data(); for (const i of at) d[i].setLabel(d[i].label() + suffix); });
  const ops = {
    create: () => setData(build(1000)),
    create10k: () => setData(build(10000)),
    append: () => setData(data().concat(build(1000))),
    update: () => batch(() => { const d = data(); for (let i = 0; i < d.length; i += 10) d[i].setLabel(d[i].label() + " !!!"); }),
    swap: () => { const d = data().slice(); if (d.length > 998) { const t = d[1]; d[1] = d[998]; d[998] = t; setData(d); } },
    clear: () => setData([]),
    patch1: () => relabel([data().length >> 1], " *"),
    patch100: () => relabel(patched(data().length), " *"),
  };
  const remove = (id) => { const d = data(); d.splice(d.findIndex((r) => r.id === id), 1); setData([...d]); };
  const button = (id, text) => h("button", { id, onClick: ops[id] }, text);
  return h("div", {},
    h("div", { class: "controls" }, button("create", "Create 1,000"), button("create10k", "Create 10,000"), button("append", "Append 1,000"),
      button("update", "Update every 10th"), button("swap", "Swap rows"), button("clear", "Clear"), button("patch1", "Patch 1"), button("patch100", "Patch 100")),
    h("table", {}, h("tbody", { id: "tbody" }, each(data, (row) =>
      h("tr", { class: () => (isSelected(row.id) ? "danger" : "") },
        h("td", { class: "col-md-1" }, String(row.id)),
        h("td", { class: "col-md-4" }, h("a", { class: "select", onClick: () => setSelected(row.id) }, () => row.label())),
        h("td", { class: "col-md-1" }, h("a", { class: "remove", onClick: () => remove(row.id) }, h("span", { class: "remove", "aria-hidden": "true" }, "x"))),
        h("td", { class: "col-md-6" })),
    ))),
  );
}

mount(/** @type {HTMLElement} */ (document.getElementById("main")), () => h(App, {}), { signal: new AbortController().signal });
document.documentElement.dataset.ready = String(performance.now());
