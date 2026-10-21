// @ts-check
// Not served or run: `npm run check` type-checks it to prove the island seams are strict.
// Every @ts-expect-error line must be an error, or tsc fails ("Unused '@ts-expect-error'").
// The islands are the examples', declared from their Rust markup in examples/*/types/app.d.ts.
import { island, subscribe } from "corten";

/** @typedef {import("../examples/hello/types/app").Card} Card Generated from the Rust struct. */

island("offer-list", {
  actions: {
    showAll({ refs, all, target, event, el, signal }) {
      refs.rest.content.cloneNode(true);
      refs.rows.rows.item(0);
      all.rows.map((rows) => rows.rows.length);
      target.disabled = true;
      event.clientX.toFixed();
      el.hidden = signal.aborted;
      // @ts-expect-error A ref the markup does not have.
      refs.row;
      // @ts-expect-error rows is on a <tbody>, which has no .content.
      refs.rows.content;
      // @ts-expect-error The action runs on a <button>, which has no .rows.
      target.rows;
    },
  },
});

// @ts-expect-error An action the markup names (on:click={ShowAll}) is missing.
island("offer-list", { actions: {} });

island("offer-list", {
  actions: {
    showAll() {},
    // @ts-expect-error An action the markup does not name.
    showNone() {},
  },
});

// @ts-expect-error A tag no #[island] has.
island("offer-lists", {});

island("offer-list", {
  actions: {
    showAll({ props }) {
      // @ts-expect-error The island has no #[prop]s.
      props.feed;
    },
  },
});

island("live-prices", {
  state: () => ({ ticks: 0 }),
  connect({ props, state, all }) {
    props.feed.toUpperCase();
    state.ticks += 1;
    all.price[0]?.cellIndex.toFixed();
    // @ts-expect-error feed is a &str in Rust, so a string here.
    props.feed.toFixed();
    // @ts-expect-error State has only what state() returns.
    state.tick;
    return () => {};
  },
  actions: {
    pause({ state, target }) {
      state.ticks = 0;
      target.ariaPressed = "true";
    },
  },
});

island("live-prices", {
  connect({ state }) {
    // @ts-expect-error Without state(), there is no state.
    state.ticks;
  },
  actions: { pause() {} },
});

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
