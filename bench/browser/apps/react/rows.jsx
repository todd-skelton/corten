// React 19, written the way the js-framework-benchmark's keyed react-hooks app is: one reducer,
// immutable rows, and a memoized Row that re-renders only when its row or selection changes.
import { memo, useLayoutEffect, useReducer } from "react";
import { label, nextId, patched } from "../shared/labels.js";

const build = (n) => {
  const out = new Array(n);
  for (let i = 0; i < n; i++) out[i] = { id: nextId(), label: label() };
  return out;
};
const relabel = (data, at, suffix) => {
  const d = data.slice();
  for (const i of at) d[i] = { ...d[i], label: d[i].label + suffix };
  return d;
};

function reducer(state, action) {
  const { data, selected } = state;
  switch (action.type) {
    case "create": return { data: build(1000), selected };
    case "create10k": return { data: build(10000), selected };
    case "append": return { data: data.concat(build(1000)), selected };
    case "update": {
      const d = data.slice();
      for (let i = 0; i < d.length; i += 10) d[i] = { ...d[i], label: d[i].label + " !!!" };
      return { data: d, selected };
    }
    case "swap": {
      if (data.length <= 998) return state;
      const d = data.slice();
      const t = d[1];
      d[1] = d[998];
      d[998] = t;
      return { data: d, selected };
    }
    case "clear": return { data: [], selected };
    case "patch1": return { data: relabel(data, [data.length >> 1], " *"), selected };
    case "patch100": return { data: relabel(data, patched(data.length), " *"), selected };
    case "select": return { data, selected: action.id };
    case "remove": {
      const d = data.slice();
      d.splice(d.findIndex((r) => r.id === action.id), 1);
      return { data: d, selected };
    }
    default: return state;
  }
}

const Row = memo(
  ({ item, selected, dispatch }) => (
    <tr className={selected ? "danger" : ""}>
      <td className="col-md-1">{item.id}</td>
      <td className="col-md-4"><a className="select" onClick={() => dispatch({ type: "select", id: item.id })}>{item.label}</a></td>
      <td className="col-md-1"><a className="remove" onClick={() => dispatch({ type: "remove", id: item.id })}><span className="remove" aria-hidden="true">x</span></a></td>
      <td className="col-md-6" />
    </tr>
  ),
  (a, b) => a.item === b.item && a.selected === b.selected,
);

const BUTTONS = [
  ["create", "Create 1,000"], ["create10k", "Create 10,000"], ["append", "Append 1,000"], ["update", "Update every 10th"],
  ["swap", "Swap rows"], ["clear", "Clear"], ["patch1", "Patch 1"], ["patch100", "Patch 100"],
];

/** `initial`: rows the server rendered, as { id, label }. Ready once React has committed. */
export function App({ initial = [] }) {
  const [{ data, selected }, dispatch] = useReducer(reducer, initial, (rows) => ({ data: rows, selected: 0 }));
  useLayoutEffect(() => {
    document.documentElement.dataset.ready = String(performance.now());
  }, []);
  return (
    <>
      <div className="controls">
        {BUTTONS.map(([id, text]) => <button key={id} id={id} onClick={() => dispatch({ type: id })}>{text}</button>)}
      </div>
      <table>
        <tbody id="tbody">
          {data.map((item) => <Row key={item.id} item={item} selected={selected === item.id} dispatch={dispatch} />)}
        </tbody>
      </table>
    </>
  );
}
