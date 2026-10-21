// @ts-check
import { island, subscribe } from "corten";

/** @typedef {import("../types/app").Card} Card Generated from `Card` in src/app.rs. */

/**
 * @param {HTMLElement[]} cells The price cells, `all.price`.
 * @param {Card} card
 */
function paint(cells, card) {
  const cell = cells.find((c) => c.dataset.id === card.id);
  if (cell && cell.textContent !== card.price) cell.textContent = card.price;
}

// Keeps the server-rendered prices current from the feed the server names in props.feed. While
// paused it keeps the latest rows and shows them on resume.
export default island("live-prices", {
  state: () => ({ paused: false, latest: /** @type {Map<string, Card>} */ (new Map()) }),
  connect({ el, refs, all, props, state }) {
    refs.pauseButton.hidden = false;
    /** @param {Card} card */
    const show = (card) => {
      state.latest.set(card.id, card);
      if (!state.paused) paint(all.price, card);
    };
    return subscribe(props.feed, {
      /** @param {Card[]} rows */
      snapshot: (rows) => rows.forEach(show),
      patch: (changes) => {
        for (const change of changes) if (change.op === "upsert") show(change.value);
      },
      status: (status) => {
        el.dataset.status = status;
      },
    });
  },
  actions: {
    pause({ all, state, target }) {
      state.paused = !state.paused;
      target.setAttribute("aria-pressed", String(state.paused));
      if (!state.paused) for (const card of state.latest.values()) paint(all.price, card);
    },
  },
});
