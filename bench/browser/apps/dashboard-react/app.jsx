// React: the prices and their directions in one state; a memoized card per symbol, the total
// computed in render.
import { memo, useLayoutEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { SYMBOLS, start, tick } from "../shared/market.js";

const Card = memo(({ symbol, price, dir }) => <div className="card"><b>{symbol}</b> <span className={dir}>{price.toFixed(2)}</span></div>);

function App() {
  const [state, setState] = useState(() => ({ prices: SYMBOLS.map((_, i) => start(i)), dirs: SYMBOLS.map(() => "up") }));
  useLayoutEffect(() => {
    document.documentElement.dataset.ready = String(performance.now());
  }, []);
  const apply = (n) => {
    const changes = tick(n);
    setState(({ prices, dirs }) => {
      const p = prices.slice();
      const d = dirs.slice();
      for (const [i, v] of changes) {
        d[i] = v >= p[i] ? "up" : "down";
        p[i] = v;
      }
      return { prices: p, dirs: d };
    });
  };
  let total = 0;
  for (const p of state.prices) total += p;
  return (
    <>
      <div className="controls"><button id="tick20" onClick={() => apply(20)}>Tick 20</button><button id="tick200" onClick={() => apply(200)}>Tick 200</button></div>
      <p>Total <span>{total.toFixed(2)}</span></p>
      <div className="cards">{SYMBOLS.map((s, i) => <Card key={s} symbol={s} price={state.prices[i]} dir={state.dirs[i]} />)}</div>
    </>
  );
}
createRoot(document.getElementById("main")).render(<App />);
