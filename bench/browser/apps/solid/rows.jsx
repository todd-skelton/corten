// Solid with its compiler, written the way the js-framework-benchmark's keyed Solid app is:
// a signal per row label, createSelector for the selected row.
import { createSignal, createSelector, batch, For } from "solid-js";
import { label, nextId, patched } from "../shared/labels.js";

const row = (id, text) => {
  const [label, setLabel] = createSignal(text);
  return { id, label, setLabel };
};
const build = (n) => {
  const out = new Array(n);
  for (let i = 0; i < n; i++) out[i] = row(nextId(), label());
  return out;
};

/** `initial`: rows the server rendered, as { id, label }. */
export function App(props) {
  const [data, setData] = createSignal((props.initial ?? []).map((r) => row(r.id, r.label)));
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
  return (
    <div>
      <div class="controls">
        <button id="create" onClick={ops.create}>Create 1,000</button>
        <button id="create10k" onClick={ops.create10k}>Create 10,000</button>
        <button id="append" onClick={ops.append}>Append 1,000</button>
        <button id="update" onClick={ops.update}>Update every 10th</button>
        <button id="swap" onClick={ops.swap}>Swap rows</button>
        <button id="clear" onClick={ops.clear}>Clear</button>
        <button id="patch1" onClick={ops.patch1}>Patch 1</button>
        <button id="patch100" onClick={ops.patch100}>Patch 100</button>
      </div>
      <table>
        <tbody id="tbody">
          <For each={data()}>
            {(row) => {
              const id = row.id;
              return (
                <tr class={isSelected(id) ? "danger" : ""}>
                  <td class="col-md-1" textContent={id} />
                  <td class="col-md-4"><a class="select" onClick={[setSelected, id]} textContent={row.label()} /></td>
                  <td class="col-md-1"><a class="remove" onClick={[remove, id]}><span class="remove" aria-hidden="true">x</span></a></td>
                  <td class="col-md-6" />
                </tr>
              );
            }}
          </For>
        </tbody>
      </table>
    </div>
  );
}

