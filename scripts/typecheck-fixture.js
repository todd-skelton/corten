// @ts-check
// Not served or run: `npm run check` type-checks it to prove the island seams are strict.
// Every @ts-expect-error line must be an error, or tsc fails ("Unused '@ts-expect-error'").
import { Island, subscribe } from "corten";

/** @typedef {import("../examples/hello/types/app").Card} Card Generated from the Rust struct. */
/** @typedef {import("../examples/item-page/types/app").OfferListRef} OfferListRef Generated from the Rust enum. */

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

/** @extends {Island<unknown, OfferListRef>} */
export class WithRefs extends Island {
  setup() {
    this.ref("rows");
    this.refs("more");
    this.on("click", "rest", () => {});
    // @ts-expect-error A data-ref name the island's Rust enum does not have.
    this.ref("row");
    // @ts-expect-error Event handlers are checked against the same names.
    this.on("click", "mor", () => {});
  }
}

export class WithoutRefs extends Island {
  setup() {
    // @ts-expect-error An island that declares no refs can use none.
    this.ref("rows");
  }
}
