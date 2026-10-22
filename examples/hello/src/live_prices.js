// @ts-check
import { island, subscribe } from "corten";

/** @typedef {import("../types/app").Stock} Stock Generated from `Stock` in src/app.rs. */

/**
 * @param {HTMLElement[]} cells The price cells, `all.price`.
 * @param {Stock} stock
 */
function paint(cells, stock) {
  const cell = cells.find((c) => c.dataset.id === stock.id);
  if (cell && cell.textContent !== stock.price) cell.textContent = stock.price;
}

// Keeps the server-rendered prices current from the feed the server names in props.feed. While
// paused it keeps the latest rows and shows them on resume.
export default island("live-prices", {
  state: () => ({ paused: false, latest: /** @type {Map<string, Stock>} */ (new Map()) }),
  connect({ el, refs, all, props, state, signal }) {
    refs.pauseButton.hidden = false;
    /** @param {Stock} stock */
    const show = (stock) => {
      state.latest.set(stock.id, stock);
      if (!state.paused) paint(all.price, stock);
    };
    subscribe(
      props.feed,
      {
        /** @param {Stock[]} rows */
        snapshot: (rows) => rows.forEach(show),
        patch: (changes) => {
          for (const change of changes) if (change.op === "upsert") show(change.value);
        },
        status: (status) => {
          el.dataset.status = status;
        },
      },
      { signal },
    );
  },
  actions: {
    pause({ all, state, target }) {
      state.paused = !state.paused;
      target.setAttribute("aria-pressed", String(state.paused));
      if (!state.paused) for (const stock of state.latest.values()) paint(all.price, stock);
    },
  },
});
