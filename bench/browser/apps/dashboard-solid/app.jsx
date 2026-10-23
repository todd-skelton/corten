// Solid: a store with a key per symbol; each card reads its key, the summary a memo of all.
import { createMemo, batch, For } from "solid-js";
import { createStore } from "solid-js/store";
import { render } from "solid-js/web";
import { SYMBOLS, start, tick } from "../shared/market.js";

const [prices, setPrices] = createStore(Object.fromEntries(SYMBOLS.map((s, i) => [s, start(i)])));
const apply = (n) => batch(() => { for (const [i, p] of tick(n)) setPrices(SYMBOLS[i], p); });

function Card(props) {
  let last = prices[props.symbol];
  const cls = createMemo(() => { const p = prices[props.symbol]; const c = p >= last ? "up" : "down"; last = p; return c; });
  return <div class="card"><b>{props.symbol}</b> <span class={cls()}>{prices[props.symbol].toFixed(2)}</span></div>;
}

function App() {
  const total = createMemo(() => { let t = 0; for (const s of SYMBOLS) t += prices[s]; return t; });
  return (
    <>
      <div class="controls"><button id="tick20" onClick={() => apply(20)}>Tick 20</button><button id="tick200" onClick={() => apply(200)}>Tick 200</button></div>
      <p>Total <span>{total().toFixed(2)}</span></p>
      <div class="cards"><For each={SYMBOLS}>{(s) => <Card symbol={s} />}</For></div>
    </>
  );
}
render(App, document.getElementById("main"));
document.documentElement.dataset.ready = String(performance.now());
