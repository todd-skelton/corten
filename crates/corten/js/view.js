// @ts-check
// What the classes in `corten/views` share. #[component(view)] generates a class per component
// whose properties are its holes: `row.label = "x"` writes that text node and nothing else, and
// `row.label` reads it back. A handler changes the page by assigning to them, as plain
// JavaScript writes to the DOM, but by name and with types. There is no state to compare, no
// tracking and no scheduler: the page holds what is shown, and a handler writes what it changed.
//
// rows(parent, Row, template) is the list of those views under `parent`: push, remove, swap,
// replace and clear change the elements, and at(i) / of(event.target) give the view to write.
// New rows are clones of the server-rendered `<template>` (one Row in it), filled with their
// data. Corten serves this file inside `corten/views` (scripts/build-js.mjs builds it).

/**
 * Gives a row cloned from a `<template>` the custom properties the server wrote for the
 * template's row (`style={..}`, `data-vars`): set through the CSSOM, as the runtime sets a
 * patch's, since a clone is under the document's policy. A script that fills rows draws their own
 * sizes itself.
 * @param {Element} root
 */
function vars(root) {
  for (const el of [root, ...Array.from(root.querySelectorAll("[data-vars]"))]) {
    const list = el.getAttribute("data-vars");
    if (!list || !(el instanceof HTMLElement || el instanceof SVGElement)) continue;
    for (const one of list.split(";")) {
      const at = one.indexOf(":");
      if (one.startsWith("--") && at > 2) el.style.setProperty(one.slice(0, at), one.slice(at + 1));
    }
  }
}

/**
 * Makes `value` the text of `el`, which holds only that text.
 * @param {Element} el @param {string} value
 */
export function text(el, value) {
  const t = /** @type {Text | null} */ (el.firstChild);
  if (t === null) el.textContent = value;
  else if (t.data !== value) t.data = value;
}

/**
 * Makes `value` what an `<input>` shows (even after the user typed) and what a form reset goes
 * back to.
 * @param {Element} el @param {string} v
 */
export function value(el, v) {
  const input = /** @type {HTMLInputElement} */ (el);
  input.defaultValue = v;
  if (input.value !== v) input.value = v;
}

/**
 * Checks or unchecks an `<input>` (even after the user clicked it), and what a form reset goes
 * back to.
 * @param {Element} el @param {boolean} on
 */
export function checked(el, on) {
  const input = /** @type {HTMLInputElement} */ (el);
  input.defaultChecked = on;
  if (input.checked !== on) input.checked = on;
}

/**
 * What #[component(view)] generates for a component whose root has `key={..}`. `keyOf` is there
 * when the key is one of its holes, so a script can make rows; a view whose key the markup also
 * shows elsewhere (a cell's form, an element id) has only `key`, and the server makes its rows.
 * @template D the data it is made from: every hole's value
 * @template {object} V the view
 * @typedef {{
 *   new (el: Element): V,
 *   fill(el: Element, data: D): void,
 *   key(el: Element): string,
 *   keyOf?: (data: D) => string,
 * }} ViewClass
 */

/**
 * The views of every child of a parent element, all made from one component: `at(i)`,
 * `get(key)` and `of(event.target)` give one; `replace(items)` shows exactly `items` (rows
 * with a key it had keep their element and are filled again, the rest are made, and the order
 * follows `items`).
 * @template D
 * @template {{ el: Element }} V
 * @typedef {{
 *   readonly length: number,
 *   at(i: number): V,
 *   get(key: string): V | undefined,
 *   of(target: EventTarget | null): V | undefined,
 *   indexOf(view: V): number,
 *   push(items: readonly D[]): void,
 *   remove(view: V): void,
 *   swap(i: number, j: number): void,
 *   clear(): void,
 *   replace(items: readonly D[]): void,
 *   [Symbol.iterator](): IterableIterator<V>,
 * }} Rows
 */

const SERVER_MADE = "corten: the server makes these rows: a script makes only rows of a view whose key={..} is one of its holes";

/** @type {WeakMap<Element, { View: unknown, list: unknown, use: (t: HTMLTemplateElement | undefined) => void }>} */
const lists = new WeakMap();

/**
 * The rows under `parent`, each a `View`. Every call for one parent returns the same list, so a
 * handler calls it each time it runs; it follows changes made elsewhere (a server patch, a
 * sort) when it is next used.
 * @template D
 * @template {{ el: Element }} V
 * @param {Element} parent
 * @param {ViewClass<D, V>} View
 * @param {HTMLTemplateElement} [template] holds one server-rendered row, cloned for new rows
 * @returns {Rows<D, V>}
 */
export function rows(parent, View, template) {
  const known = lists.get(parent);
  if (known) {
    if (known.View !== View) throw new Error("corten: rows() of one element are one view", { cause: parent });
    known.use(template);
    return /** @type {Rows<D, V>} */ (known.list);
  }
  /** @type {Element[]} */
  let els = Array.from(parent.children);
  /** The rows by key, made on the first get() after a change. @type {Map<string, Element> | null} */
  let byKey = null;
  // Changes to the children made elsewhere: the list reads them again before its next use.
  // Its own changes are taken (and dropped) as it makes them.
  let stale = false;
  const watch = new MutationObserver(() => {
    stale = true;
  });
  watch.observe(parent, { childList: true });
  const sync = () => {
    if (watch.takeRecords().length || stale) {
      els = Array.from(parent.children);
      byKey = null;
      stale = false;
    }
  };
  const done = () => {
    watch.takeRecords();
    byKey = null;
  };
  /** @type {Element | null | undefined} */
  let proto;
  /** Whether the template's row has custom properties to give its copies. */
  let protoVars = false;
  const use = (/** @type {HTMLTemplateElement | undefined} */ t) => {
    if (!t) return;
    proto = t.content.firstElementChild;
    protoVars = !!proto && (proto.hasAttribute("data-vars") || !!proto.querySelector("[data-vars]"));
  };
  use(template);
  /** @param {D} data */
  const make = (data) => {
    if (!proto) throw new Error("corten: rows() makes new rows from a <template> with one row in it", { cause: parent });
    const el = /** @type {Element} */ (proto.cloneNode(true));
    if (protoVars) vars(el);
    View.fill(el, data);
    return el;
  };
  /** @param {readonly D[]} items @param {Element[]} out @param {number} from @param {Node | null} before */
  const add = (items, out, from, before) => {
    const frag = document.createDocumentFragment();
    for (let i = 0; i < items.length; i += 1) frag.appendChild((out[from + i] = make(items[i])));
    parent.insertBefore(frag, before);
  };
  /** @type {Rows<D, V>} */
  const list = {
    get length() {
      sync();
      return els.length;
    },
    at(i) {
      sync();
      return new View(els[i]);
    },
    get(key) {
      sync();
      if (!byKey) {
        byKey = new Map();
        for (const el of els) byKey.set(View.key(el), el);
      }
      const el = byKey.get(key);
      return el ? new View(el) : undefined;
    },
    of(target) {
      let el = /** @type {Node | null} */ (target);
      while (el && el.parentNode !== parent) el = el.parentNode;
      return el ? new View(/** @type {Element} */ (el)) : undefined;
    },
    indexOf(view) {
      sync();
      return els.indexOf(view.el);
    },
    push(items) {
      if (!View.keyOf) throw new Error(SERVER_MADE, { cause: parent });
      sync();
      add(items, els, els.length, null);
      done();
    },
    remove(view) {
      sync();
      const i = els.indexOf(view.el);
      if (i < 0) return;
      els.splice(i, 1);
      view.el.remove();
      done();
    },
    swap(i, j) {
      sync();
      const a = els[i];
      const b = els[j];
      if (!a || !b || a === b) return;
      // Either may come first, and they may be neighbors: a goes where b is, then b where a was.
      const afterA = a.nextSibling;
      if (afterA === b) parent.insertBefore(b, a);
      else {
        parent.insertBefore(a, b);
        parent.insertBefore(b, afterA);
      }
      els[i] = b;
      els[j] = a;
      done();
    },
    clear() {
      parent.textContent = "";
      els = [];
      done();
    },
    replace(items) {
      sync();
      const n = items.length;
      /** @type {Map<string, number>} */
      const where = new Map();
      const keyOf = View.keyOf;
      if (!keyOf) throw new Error(SERVER_MADE, { cause: parent });
      for (let j = 0; j < els.length; j += 1) where.set(View.key(els[j]), j);
      // Each new item's old element, or -1.
      const from = new Int32Array(n).fill(-1);
      let kept = 0;
      // With no rows to keep (a first fill, or after clear()), no key is looked up.
      if (where.size) for (let i = 0; i < n; i += 1) {
        const k = keyOf(items[i]);
        const j = where.get(k);
        if (j !== undefined) {
          from[i] = j;
          where.delete(k);
          kept += 1;
        }
      }
      /** @type {Element[]} */
      const next = new Array(n);
      if (kept === 0) {
        // A new list: clear and build in one go.
        parent.textContent = "";
        add(items, next, 0, null);
      } else {
        for (const j of where.values()) els[j].remove();
        // Elements on the longest run already in order stay; the rest move or are made.
        const stay = lis(from);
        let s = stay.length - 1;
        /** @type {Node | null} */
        let before = null;
        for (let i = n - 1; i >= 0; i -= 1) {
          const j = from[i];
          let el;
          if (j < 0) {
            el = make(items[i]);
            parent.insertBefore(el, before);
          } else {
            el = els[j];
            View.fill(el, items[i]);
            if (s >= 0 && stay[s] === i) s -= 1;
            else parent.insertBefore(el, before);
          }
          next[i] = el;
          before = el;
        }
      }
      els = next;
      done();
    },
    *[Symbol.iterator]() {
      sync();
      for (const el of els.slice()) yield new View(el);
    },
  };
  lists.set(parent, { View, list, use });
  return list;
}

/** Positions (into `a`) of a longest increasing run of its non-negative values. @param {Int32Array} a */
function lis(a) {
  /** @type {number[]} */
  const tails = [];
  const prev = new Int32Array(a.length);
  for (let i = 0; i < a.length; i += 1) {
    const v = a[i];
    if (v < 0) continue;
    let lo = 0;
    let hi = tails.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (a[tails[mid]] < v) lo = mid + 1;
      else hi = mid;
    }
    prev[i] = lo > 0 ? tails[lo - 1] : -1;
    tails[lo] = i;
  }
  const run = new Array(tails.length);
  for (let i = tails.length - 1, k = tails[tails.length - 1]; i >= 0; i -= 1, k = prev[k]) run[i] = k;
  return run;
}
