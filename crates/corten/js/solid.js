// @ts-check
// corten/solid: render part of a component with Solid. Served prebuilt by the corten crate
// (feature "solid") at /corten/solid.js; `npm run build:js` regenerates js/dist/solid.js from this file.
//
//   // stock_badge.rs: <div on:connect={js::start(target, signal)}>"0"</div>
//   import { mount, h, createSignal } from "corten/solid";
//   /** @param {HTMLDivElement} el @param {AbortSignal} signal */
//   export function start(el, signal) {
//     mount(el, () => { const [n, setN] = createSignal(0); return h("button", { onClick: () => setN(n() + 1) }, () => n()); }, { signal });
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
 * Renders `view(server)` with Solid in `el` until `signal` aborts, then disposes it (its effects,
 * cleanups and listeners): `mount(el, (server) => h(Grid, props), { signal })` in an `on:connect`
 * handler, with its `signal`.
 * `server` holds the nodes the server rendered inside the element, so the view can keep showing
 * them until its data is ready (`when(loaded, view, () => server)`); empty the array once they are
 * gone (`server.length = 0`) so they can be collected. 
 * @param {HTMLElement} el
 * @param {(server: Node[]) => any} view
 * @param {{ signal: AbortSignal }} options
 * @returns {void}
 */
export function mount(el, view, { signal }) {
  if (signal.aborted) return;
  const server = Array.from(el.childNodes);
  el.replaceChildren();
  signal.addEventListener("abort", render(() => view(server), el), { once: true });
}
