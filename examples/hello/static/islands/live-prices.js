// @ts-check
import { Island } from "corten";

/** @typedef {import("../../types/app").Card} Card Generated from `Card` in src/app.rs. */
/** @typedef {import("../../types/app").LivePricesRef} Ref Generated from `LivePricesRef` in src/app.rs. */

/**
 * Keeps the server-rendered prices current from the feed at `data-src`.
 * @extends {Island<unknown, Ref>}
 */
export default class LivePrices extends Island {
  setup() {
    const src = this.dataset.src;
    if (!src) throw new Error("<live-prices> needs data-src");
    const cells = new Map(this.refs("price").map((cell) => [cell.dataset.id, cell]));
    /** @param {Card} card */
    const show = (card) => {
      const cell = cells.get(card.id);
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
