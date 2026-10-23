// Remix 3: a shared store that is an EventTarget; each card listens for its symbol and calls
// its own handle.update(), so only changed cards render (the granular way, not one root update).
import { createRoot, on, type Handle } from "remix/component";
import { SYMBOLS, start, tick } from "../shared/market.js";

const prices = SYMBOLS.map((_, i) => start(i));
const store = new EventTarget();
const apply = (n: number) => {
  for (const [i, p] of tick(n)) { prices[i] = p; store.dispatchEvent(new CustomEvent("price", { detail: i })); }
  store.dispatchEvent(new Event("tick"));
};

function Card(handle: Handle<{ index: number }>) {
  let last = prices[handle.props.index];
  let cls = "up";
  store.addEventListener("price", (e) => {
    if ((e as CustomEvent).detail !== handle.props.index) return;
    const p = prices[handle.props.index];
    cls = p >= last ? "up" : "down";
    last = p;
    handle.update();
  }, { signal: handle.signal });
  return () => <div class="card"><b>{SYMBOLS[handle.props.index]}</b> <span class={cls}>{prices[handle.props.index].toFixed(2)}</span></div>;
}

function Summary(handle: Handle) {
  store.addEventListener("tick", () => handle.update(), { signal: handle.signal });
  return () => <p>Total <span>{prices.reduce((a, b) => a + b, 0).toFixed(2)}</span></p>;
}

function App() {
  return () => (
    <>
      <div class="controls"><button id="tick20" mix={on("click", () => apply(20))}>Tick 20</button><button id="tick200" mix={on("click", () => apply(200))}>Tick 200</button></div>
      <Summary />
      <div class="cards">{SYMBOLS.map((_, i) => <Card key={i} index={i} />)}</div>
    </>
  );
}
const root = createRoot(document.getElementById("main")!);
root.render(<App />);
root.flush();
document.documentElement.dataset.ready = String(performance.now());
