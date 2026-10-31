// @ts-check
// Not served or run: `npm run check` type-checks it to prove the script seams are strict.
// Every @ts-expect-error line must be an error, or tsc fails ("Unused '@ts-expect-error'").
// Handler parameters need nothing here: the Rust markup's calls are checked against each
// handler's JSDoc when the crate compiles. What crosses into scripts as JSON or as an event is
// declared from the Rust types in examples/*/types/app.d.ts.
import { keep, subscribe } from "corten";
import { rows } from "corten/views";
// @ts-expect-error The runtime takes a module's exports from the line Corten appends; it exports no hook for it.
import { register } from "corten";

/** @typedef {import("../examples/watchlist/types/app").Stock} Stock Generated from the Rust struct. */
/** @typedef {import("../examples/seller-desk/types/app").Level} Level Generated from the Rust struct. */
/** @import { FeedUrl } from "corten" */

/** @param {AbortSignal} signal @param {FeedUrl<Stock>} stocks */
export function connect(signal, stocks) {
  const s = keep(() => ({ ticks: 0 }));
  s.ticks += 1;
  // @ts-expect-error State has only what its init returns.
  s.tick;
  // @ts-expect-error keep() finds the component itself: it takes no element.
  keep(document.body, () => ({ ticks: 0 }));
  subscribe(stocks, { snapshot() {}, patch() {} }, { signal });
  // @ts-expect-error subscribe() returns nothing to stop it with; pass a signal.
  subscribe(stocks, { snapshot() {}, patch() {} })();
}

// The handler's parameter names the feed's rows (the route's rows = Stock, checked when the crate
// compiles), and subscribe() takes them from it.
/** @param {FeedUrl<Stock>} stocks @param {FeedUrl<Level>} levels @param {string} url */
export function follow(stocks, levels, url) {
  subscribe(stocks, {
    snapshot: (rows, seq, epoch) => {
      rows.map((row) => row.name);
      // The generation is a string: a seq is only comparable within one.
      // @ts-expect-error
      epoch.toFixed();
      return seq.toFixed() + epoch.length;
    },
    patch: (changes) => {
      for (const change of changes) {
        // @ts-expect-error A removal carries no value; the row type flows into patches.
        change.value.name;
        if (change.op === "upsert") change.value.price_cents.toFixed();
      }
    },
  });
  subscribe(levels, {
    /** @param {Level[]} rows */
    snapshot: (rows) => rows.map((row) => row.qty),
    patch: () => {},
  });
  // @ts-expect-error A Level feed's rows are not Stock rows.
  subscribe(levels, {
    /** @param {Stock[]} rows */
    snapshot: (rows) => rows.map((row) => row.name),
    patch: () => {},
  });
  // @ts-expect-error A plain string names no rows: the URL comes from a FeedUrl<Row> parameter.
  subscribe(url, { snapshot() {}, patch() {} });
  // @ts-expect-error Nor does a written one.
  subscribe("/feed/stocks", { snapshot() {}, patch() {} });
}

/** @import { Message } from "corten" */

/** @param {HTMLOutputElement} out @param {Message<"count">} label */
export function selected(out, label) {
  out.textContent = label({ count: 3 });
  // @ts-expect-error A message is filled in with the arguments its type names.
  out.textContent = label({ total: 3 });
  // @ts-expect-error Each of them.
  out.textContent = label({});
}

/** @import { NumberFormat, MoneyFormat, DateFormat, PercentFormat } from "corten" */

/** @param {HTMLOutputElement} out @param {NumberFormat} count @param {MoneyFormat} price @param {DateFormat} when @param {PercentFormat} rate */
export function formatted(out, count, price, when, rate) {
  out.textContent = `${count(1234)} ${count("21.5")} ${price(123450)} ${when(Date.now())} ${when(new Date())} ${rate(0.029)} ${rate("0.029")}`;
  // @ts-expect-error A ratio, not a date.
  rate(new Date());
  // @ts-expect-error An amount is an integer of minor units, not its text.
  price("12.50");
  // @ts-expect-error A date's format takes a date, not an object.
  when({ at: 0 });
  // At another offset, in seconds east: the browser's at that instant.
  out.title = when(Date.now(), -new Date().getTimezoneOffset() * 60);
  // @ts-expect-error An offset is seconds east, not a zone's name.
  when(Date.now(), "America/Chicago");
}

/** A view without key={..}, as #[component(view)] declares one: `fill` and its holes, no `key`. */
class Cell {
  /** @param {Element} el */
  constructor(el) {
    this.el = el;
  }
  get ladder() {
    return Number(this.el.getAttribute("data-ladder"));
  }
  /** @param {Element} el @param {{ ladder: number }} data */
  static fill(el, data) {
    el.setAttribute("data-ladder", String(data.ladder));
  }
}

// A script reads, removes and reorders rows of a view without a key; only a key finds or makes one.
/** @param {import("corten/views").Holds<Cell, HTMLUListElement>} cells */
export function ladders(cells) {
  const list = rows(cells, Cell);
  list.swap(0, 1);
  // @ts-expect-error get() finds a row by its key: the view has none.
  list.get("a");
  // @ts-expect-error replace() keeps rows by their key: the view has none.
  list.replace([{ ladder: 1 }]);
  // @ts-expect-error A clone would keep what the view's markup shows outside its holes.
  list.push([{ ladder: 1 }]);
  return [...list].map((cell) => cell.ladder);
}
