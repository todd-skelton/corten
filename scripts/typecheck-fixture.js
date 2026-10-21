// @ts-check
// Not served or run: `npm run check` type-checks it to prove the island seams are strict.
// Every @ts-expect-error line must be an error, or tsc fails ("Unused '@ts-expect-error'").
import { Island, subscribe } from "corten";

/** @typedef {import("../examples/hello/types/app").Card} Card Generated from the Rust struct. */

/** @extends {Island<Card>} */
export class Typed extends Island {
  setup() {
    const card = this.props();
    card.price.toUpperCase();
    // @ts-expect-error A field the Rust struct does not have.
    card.price_dollars;
    // @ts-expect-error price_cents is a number in Rust, so not a string here.
    card.price_cents.toUpperCase();
  }
}

export class Untyped extends Island {
  setup() {
    // @ts-expect-error Without a props type, props() is unknown: nothing can be read unchecked.
    this.props().price;
  }
}

subscribe("/feed/cards", {
  /** @param {Card[]} rows */
  snapshot: (rows) => rows.map((row) => row.name),
  patch: (changes) => {
    for (const change of changes) {
      // @ts-expect-error A removal carries no value; the row type flows into patches.
      change.value.name;
      if (change.op === "upsert") change.value.price_cents.toFixed();
    }
  },
});

subscribe("/feed/cards", {
  snapshot: (rows) => {
    // @ts-expect-error Without a row type, rows are unknown, not any.
    rows[0].name;
  },
  patch: () => {},
});
