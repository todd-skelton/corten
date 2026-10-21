// @ts-check
import { Island } from "corten";

/** @typedef {{ id: string, name: string, price_cents: number, price: string }} Card Same fields as `Card` in src/app.rs. */

/** Keeps the server-rendered prices current from the feed at `data-src`. */
export default class LivePrices extends Island {
  setup() {
    const src = this.dataset.src;
    if (!src) throw new Error("<live-prices> needs data-src");
    /** @param {Card} card */
    const show = (card) => {
      const cell = this.querySelector(`[data-ref="price"][data-id="${CSS.escape(card.id)}"]`);
      if (cell && cell.textContent !== card.price) cell.textContent = card.price;
    };
    this.subscribe(src, {
      /** @param {Card[]} rows */
      snapshot: (rows) => rows.forEach(show),
      patch: (changes) => {
        for (const change of changes) if (change.op === "upsert") show(change.value);
      },
      status: (status) => {
        this.dataset.status = status;
      },
    });
  }
}
