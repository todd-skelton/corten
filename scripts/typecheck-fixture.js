// @ts-check
// Not served or run: `npm run check` type-checks it to prove the script seams are strict.
// Every @ts-expect-error line must be an error, or tsc fails ("Unused '@ts-expect-error'").
// Handler parameters need nothing here: the Rust markup's calls are checked against each
// handler's JSDoc when the crate compiles. What crosses into scripts as JSON or as an event is
// declared from the Rust types in examples/*/types/app.d.ts.
import { state, subscribe } from "corten";
// @ts-expect-error The runtime takes a module's exports from the line Corten appends; it exports no hook for it.
import { register } from "corten";

/** @typedef {import("../examples/hello/types/app").Stock} Stock Generated from the Rust struct. */

/** @param {HTMLElement} root @param {AbortSignal} signal */
export function connect(root, signal) {
  const s = state(root, () => ({ ticks: 0 }));
  s.ticks += 1;
  // @ts-expect-error State has only what its init returns.
  s.tick;
  subscribe("/feed/stocks", { snapshot() {}, patch() {} }, { signal });
  // @ts-expect-error subscribe() returns nothing to stop it with; pass a signal.
  subscribe("/feed/stocks", { snapshot() {}, patch() {} })();
}

subscribe("/feed/stocks", {
  /** @param {Stock[]} rows */
  snapshot: (rows) => rows.map((row) => row.name),
  patch: (changes) => {
    for (const change of changes) {
      // @ts-expect-error A removal carries no value; the row type flows into patches.
      change.value.name;
      if (change.op === "upsert") change.value.price_cents.toFixed();
    }
  },
});

subscribe("/feed/stocks", {
  snapshot: (rows) => {
    // @ts-expect-error Without a row type, rows are unknown, not any.
    rows[0].name;
  },
  patch: () => {},
});
