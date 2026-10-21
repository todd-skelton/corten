// @ts-check
// corten/solid: render an island with Solid. Served prebuilt by the corten crate (feature "solid") at
// /corten/solid.js; `npm run build:solid` regenerates js/dist/solid.js from this file.
//
//   import { SolidIsland, h, createSignal } from "corten/solid";
//   export default class StockBadge extends SolidIsland {
//     view() { const [n, setN] = createSignal(0); return h("button", { onClick: () => setN(n() + 1) }, () => n()); }
//   }
//
// Rules (h has no compiler, so reactivity is explicit):
// 1. A value that changes is passed as a function: `h("td", () => row().price)`, `{ class: () => ... }`.
//    A plain value is read once.
// 2. Event handlers are `onClick` style props (delegated) and are never wrapped.
// 3. Components are called through h: `h(Row, { id })`, not `Row({ id })`. A zero-argument
//    function prop arrives as a reactive getter: `h(Row, { index })` is read as `props.index`.
// 4. Switch between views with `when(cond, view, fallback)`, never `() => cond() ? h(...) : h(...)`:
//    `when` builds the chosen view untracked, so only `cond` decides when it is rebuilt.
// 5. Render lists with `each(list, (item, index) => h(...))`, never `h(For, ...)`. h builds lazily, so
//    under a bare For each item would be built by the parent's effect, and the next list change
//    would dispose the item's signals and handlers while its DOM stays on screen.
import { Island } from "corten";
import { render } from "solid-js/web";
import { For, untrack } from "solid-js";
import h from "solid-js/h";

export * from "solid-js";
export { createStore, produce, reconcile, unwrap } from "solid-js/store";
export { Dynamic, Portal } from "solid-js/web";
export { h };

/** Builds what h returned (a function that creates the node) now, in the current owner. */
function build(/** @type {unknown} */ made) {
  while (typeof made === "function") made = made();
  return made;
}

/**
 * Shows `view()` while `cond()` is truthy and `fallback()` otherwise. Each returns `h(...)`, a node,
 * or an array of nodes, and is built untracked, so reads inside it do not rebuild it.
 * @param {() => unknown} cond
 * @param {() => any} view
 * @param {() => any} [fallback]
 */
export function when(cond, view, fallback = () => null) {
  const make = (/** @type {() => any} */ fn) => untrack(() => build(fn()));
  return () => (cond() ? make(view) : make(fallback));
}

/**
 * A keyed list: `render(item, index)` runs once per item (compared by identity, so use ids or
 * stable objects) inside the item's own root; `index` is a getter. Items that stay keep their
 * nodes and state when the list changes.
 * @template T
 * @param {() => readonly T[]} list
 * @param {(item: T, index: () => number) => any} render
 */
export function each(list, render) {
  return h(For, { each: list }, (/** @type {T} */ item, /** @type {() => number} */ index) => build(render(item, index)));
}

/**
 * An island rendered with Solid. Write `view(server)`; it runs inside a Solid root when the
 * island connects and is disposed when the island leaves the page. `server` holds the nodes the
 * server rendered inside the element, so the view can keep showing them until its data is ready
 * (`when(loaded, view, () => server)`); empty the array once they are gone (`server.length = 0`)
 * so they can be collected.
 */
export class SolidIsland extends Island {
  setup() {
    if (this.querySelector(":scope > script[data-props]")) this.props(); // read before the nodes go
    const server = Array.from(this.childNodes);
    this.replaceChildren();
    this.cleanup(render(() => this.view(server), this));
  }

  /**
   * @param {Node[]} server
   * @returns {any}
   */
  view(server) {
    throw new Error(`<${this.localName}> extends SolidIsland and must define view()`);
  }
}
