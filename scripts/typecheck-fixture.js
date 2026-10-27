// @ts-check
// Not served or run: `npm run check` type-checks it to prove the script seams are strict.
// Every @ts-expect-error line must be an error, or tsc fails ("Unused '@ts-expect-error'").
// Handler parameters need nothing here: the Rust markup's calls are checked against each
// handler's JSDoc when the crate compiles. What crosses into scripts as JSON or as an event is
// declared from the Rust types in examples/*/types/app.d.ts.
import { state, subscribe } from "corten";
// @ts-expect-error The runtime takes a module's exports from the line Corten appends; it exports no hook for it.
import { register } from "corten";

/** @typedef {import("../examples/watchlist/types/app").Stock} Stock Generated from the Rust struct. */

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

/** @import { Message } from "corten" */

/** @param {HTMLOutputElement} out @param {Message<"count">} label */
export function selected(out, label) {
  out.textContent = label({ count: 3 });
  // @ts-expect-error A message is filled in with the arguments its type names.
  out.textContent = label({ total: 3 });
  // @ts-expect-error Each of them.
  out.textContent = label({});
}

/** @import { NumberFormat, MoneyFormat, DateFormat } from "corten" */

/** @param {HTMLOutputElement} out @param {NumberFormat} count @param {MoneyFormat} price @param {DateFormat} when */
export function formatted(out, count, price, when) {
  out.textContent = `${count(1234)} ${count("21.5")} ${price(123450)} ${when(Date.now())} ${when(new Date())}`;
  // @ts-expect-error An amount is an integer of minor units, not its text.
  price("12.50");
  // @ts-expect-error A date's format takes a date, not an object.
  when({ at: 0 });
}
