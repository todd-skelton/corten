// Remix 3's component model: a setup function with plain variables, a render function, and
// handle.update() after every change (the same component as the rmx bench's rows-bench.tsx).
import { clientEntry, on, type Handle } from "remix/component";
import { label, nextId, patched } from "../shared/labels.js";

interface Row { id: number; label: string }

export const RowsBench = clientEntry("/remix-ssr/app.js#RowsBench", function RowsBench(handle: Handle<{ initial?: Row[] }>) {
  let rows: Row[] = handle.props.initial ?? [];
  let selected: number | undefined;
  const build = (n: number) => { const out: Row[] = []; for (let i = 0; i < n; i++) out.push({ id: nextId(), label: label() }); return out; };
  const op = (f: () => void) => on("click", () => { f(); handle.update(); });
  const relabel = (at: number[], suffix: string) => { for (const i of at) rows[i].label += suffix; };
  return () => (
    <div>
      <div class="controls">
        <button id="create" mix={op(() => (rows = build(1000)))}>Create 1,000</button>
        <button id="create10k" mix={op(() => (rows = build(10000)))}>Create 10,000</button>
        <button id="append" mix={op(() => (rows = rows.concat(build(1000))))}>Append 1,000</button>
        <button id="update" mix={op(() => { for (let i = 0; i < rows.length; i += 10) rows[i].label += " !!!"; })}>Update every 10th</button>
        <button id="swap" mix={op(() => { if (rows.length > 998) [rows[1], rows[998]] = [rows[998], rows[1]]; })}>Swap rows</button>
        <button id="clear" mix={op(() => (rows = []))}>Clear</button>
        <button id="patch1" mix={op(() => relabel([rows.length >> 1], " *"))}>Patch 1</button>
        <button id="patch100" mix={op(() => relabel(patched(rows.length), " *"))}>Patch 100</button>
      </div>
      <table>
        <tbody id="tbody">
          {rows.map((row) => (
            <tr key={row.id} class={selected === row.id ? "danger" : ""}>
              <td class="col-md-1">{row.id}</td>
              <td class="col-md-4"><a class="select" mix={on("click", () => { selected = row.id; handle.update(); })}>{row.label}</a></td>
              <td class="col-md-1"><a class="remove" mix={on("click", () => { rows = rows.filter((r) => r !== row); handle.update(); })}><span class="remove" aria-hidden="true">x</span></a></td>
              <td class="col-md-6"></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
});
