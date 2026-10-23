// @ts-check
// corten/state (prototype): a component's script is a setup function. It keeps state in a
// state({...}) object and plain variables, returns its event handlers and one or more renders,
// and never calls update(): assigning a different value to a state key runs, once, in a
// microtask, only the renders that read that key. No virtual DOM, no signals graph, no first
// render: the server's HTML is already right, and a render only patches what changed.
//
//   export default function counter({ refs }) {
//     const s = state({ n: 0 });
//     return {
//       render() { refs.count.textContent = String(s.n) },
//       add() { s.n += 1 },
//     };
//   }

/**
 * A render: a function the runtime runs again when a key it read last time is given a new value.
 * `stamp` counts its runs; each key's readers remember the run that last read it, so a render that
 * reads the same keys again (the usual case) updates a number instead of leaving and rejoining
 * every key's readers.
 * @typedef {{ run: () => void, order: number, stamp: number, deps: Map<Render, number>[] }} Render
 */

/** The render running now, which collects the keys it reads. @type {Render | null} */
let current = null;
/** Renders to run in the next flush. @type {Set<Render>} */
const dirty = new Set();
let scheduled = false;
let order = 0;
/** The "key" a render reads when it lists the keys (for..in, Object.keys): any key added or deleted. */
const KEYS = Symbol("keys");
/** Development checks: values given to state are frozen, and a render that writes state warns. */
export const dev = { on: false };

/** @param {unknown} value @param {WeakSet<object>} seen */
function freeze(value, seen = new WeakSet()) {
  if (value === null || typeof value !== "object" || seen.has(value)) return;
  const proto = Object.getPrototypeOf(value);
  if (proto !== Object.prototype && proto !== Array.prototype && proto !== null) return;
  seen.add(value);
  Object.freeze(value);
  for (const inner of Object.values(value)) freeze(inner, seen);
}

/**
 * State that renders read. Shallow: a render runs again when a key is given a value that is not
 * Object.is the old one, so replace objects and arrays (`s.rows = s.rows.filter(..)`) rather
 * than changing them in place. With `dev.on` the values are frozen, so a change in place throws.
 * @template {object} T
 * @param {T} init
 * @returns {T}
 */
export function state(init) {
  /** @type {Map<PropertyKey, Map<Render, number>>} */
  const readers = new Map();
  const target = /** @type {Record<PropertyKey, unknown>} */ ({ ...init });
  if (dev.on) for (const v of Object.values(target)) freeze(v);
  /** @param {PropertyKey} key */
  const read = (key) => {
    const r = /** @type {Render} */ (current);
    let by = readers.get(key);
    if (!by) readers.set(key, (by = new Map()));
    const seen = by.get(r);
    if (seen === r.stamp) return;
    if (seen === undefined) r.deps.push(by);
    by.set(r, r.stamp);
  };
  /** @param {PropertyKey} key */
  const changed = (key) => {
    const by = readers.get(key);
    if (!by?.size) return;
    for (const r of by.keys()) dirty.add(r);
    if (!scheduled) {
      scheduled = true;
      queueMicrotask(flush);
    }
  };
  return /** @type {T} */ (
    new Proxy(target, {
      get(t, key) {
        if (current) read(key);
        return t[key];
      },
      has(t, key) {
        if (current) read(key);
        return key in t;
      },
      ownKeys(t) {
        if (current) read(KEYS);
        return Reflect.ownKeys(t);
      },
      set(t, key, value) {
        const added = !(key in t);
        if (!added && Object.is(t[key], value)) return true;
        if (dev.on) {
          if (current) console.warn("corten: a render wrote state." + String(key) + "; renders should only read state");
          freeze(value);
        }
        t[key] = value;
        changed(key);
        if (added) changed(KEYS);
        return true;
      },
      deleteProperty(t, key) {
        if (!(key in t)) return true;
        delete t[key];
        changed(key);
        changed(KEYS);
        return true;
      },
    })
  );
}

/** Runs the renders whose keys changed, each once, in the order they were declared. */
export function flush() {
  for (let pass = 0; dirty.size && pass < 100; pass += 1) {
    const batch = [...dirty].sort((a, b) => a.order - b.order);
    dirty.clear();
    for (const r of batch) track(r);
  }
  scheduled = false;
}

/** Runs `r`, recording the keys it reads, and drops the keys it read last time but not now. @param {Render} r */
function track(r) {
  r.stamp += 1;
  const outer = current;
  current = r;
  try {
    r.run();
  } catch (error) {
    reportError(error);
  } finally {
    current = outer;
  }
  let kept = 0;
  for (let i = 0; i < r.deps.length; i += 1) {
    const by = r.deps[i];
    if (by.get(r) === r.stamp) r.deps[kept++] = by;
    else by.delete(r);
  }
  r.deps.length = kept;
}

/**
 * Shows `value` as the text of `el`, changing its text node only if it differs, so a render
 * that runs over the server's (already right) HTML changes nothing.
 * @param {Element} el @param {string} value
 */
export function text(el, value) {
  const t = el.firstChild;
  if (t && t.nodeType === 3 && !t.nextSibling) {
    if (/** @type {Text} */ (t).data !== value) /** @type {Text} */ (t).data = value;
  } else el.textContent = value;
}

/**
 * A component made from a template in the page: what the server renders for a row is the same
 * markup, so the client clones it instead of building it. `holes` finds, in a fresh copy, the
 * nodes that change (text nodes and elements), and `fill` writes an item into them, comparing with
 * the item it showed before (`undefined` the first time). Generated from the Rust component.
 * @template T
 * @typedef {{ make: (item: T) => Element, patch: (node: Element, item: T, before: T) => void }} Template
 */

/**
 * @template T
 * @template {Node[]} H
 * @param {HTMLTemplateElement} tpl
 * @param {(root: Element) => H} holes
 * @param {(holes: H, item: T, before: T | undefined) => void} fill
 * @returns {Template<T>}
 */
export function template(tpl, holes, fill) {
  const proto = /** @type {Element} */ (tpl.content.firstElementChild);
  return {
    make(item) {
      const node = /** @type {Element & { $h?: H }} */ (proto.cloneNode(true));
      const h = (node.$h = holes(node));
      fill(h, item, undefined);
      return node;
    },
    patch(node, item, before) {
      const n = /** @type {Element & { $h?: H }} */ (node);
      fill((n.$h ??= holes(n)), item, before);
    },
  };
}

/**
 * A keyed list that owns every child of `parent`. `set(items)` makes the children match:
 * items with a key it had keep their element (patched if the item is a new object), others are
 * made from the template, and elements move only when their order changed. `initial` adopts
 * elements the server rendered for those items.
 * @template T
 * @param {Element} parent
 * @param {Template<T>} row
 * @param {(item: T) => unknown} key
 * @param {readonly T[]} [initial]
 */
export function list(parent, row, key, initial = []) {
  /** @type {readonly T[]} */
  let items = initial;
  /** @type {Element[]} */
  let nodes = /** @type {Element[]} */ (Array.from(parent.children)).slice(0, initial.length);
  /** The element node() found last. @type {Element | undefined} */
  let found;
  for (let i = 0; i < nodes.length; i += 1) /** @type {any} */ (nodes[i]).$i = initial[i];

  /** @param {T} item */
  const make = (item) => {
    const node = row.make(item);
    /** @type {any} */ (node).$i = item;
    return node;
  };
  /** @param {Element} node @param {T} item @param {T} before */
  const patch = (node, item, before) => {
    if (item !== before) {
      row.patch(node, item, before);
      /** @type {any} */ (node).$i = item;
    }
  };

  return {
    /** @param {readonly T[]} next */
    set(next) {
      if (next === items) return;
      const old = items;
      const oldNodes = nodes;
      const n = next.length;
      const m = old.length;
      items = next;
      if (n === 0) {
        nodes = [];
        if (m) parent.textContent = "";
        return;
      }
      if (m === 0) {
        nodes = new Array(n);
        appendAll(parent, next, nodes, 0, n, make, null);
        return;
      }
      // Same keys at the start and the end: patch in place.
      let start = 0;
      // An item that is the same object as before needs nothing: no key() call, no patch. With
      // shallow state, a change to one row of 10,000 makes one new object, so this loop is the
      // whole cost of the other 9,999.
      const end = n < m ? n : m;
      while (start < end) {
        const a = next[start];
        const b = old[start];
        if (a !== b) {
          if (key(a) !== key(b)) break;
          row.patch(oldNodes[start], a, b);
          /** @type {any} */ (oldNodes[start]).$i = a;
        }
        start += 1;
      }
      let endNew = n - 1;
      let endOld = m - 1;
      while (endNew >= start && endOld >= start) {
        const a = next[endNew];
        const b = old[endOld];
        if (a !== b) {
          if (key(a) !== key(b)) break;
          row.patch(oldNodes[endOld], a, b);
          /** @type {any} */ (oldNodes[endOld]).$i = a;
        }
        endNew -= 1;
        endOld -= 1;
      }
      // Every item kept its place (the usual edit: some rows changed): the elements are the same.
      if (start > endNew && start > endOld) return;
      /** @type {Element[]} */
      const out = new Array(n);
      nodes = out;
      for (let i = 0; i < start; i += 1) out[i] = oldNodes[i];
      for (let i = endNew + 1, j = endOld + 1; i < n; i += 1, j += 1) out[i] = oldNodes[j];
      if (start > endOld) {
        if (start <= endNew) appendAll(parent, next, out, start, endNew + 1, make, oldNodes[endOld + 1] ?? null);
        return;
      }
      if (start > endNew) {
        for (let i = start; i <= endOld; i += 1) oldNodes[i].remove();
        return;
      }
      // The middle: find each new item's old element.
      /** @type {Map<unknown, number>} */
      const where = new Map();
      for (let i = start; i <= endOld; i += 1) where.set(key(old[i]), i);
      const count = endNew - start + 1;
      const from = new Int32Array(count).fill(-1);
      let kept = 0;
      for (let i = start; i <= endNew; i += 1) {
        const j = where.get(key(next[i]));
        if (j !== undefined) {
          from[i - start] = j;
          where.delete(key(next[i]));
          kept += 1;
        }
      }
      if (kept === 0 && start === 0 && endOld === m - 1 && endNew === n - 1) {
        // Nothing kept (a new list): clear and build in one go.
        parent.textContent = "";
        appendAll(parent, next, out, 0, n, make, null);
        return;
      }
      for (const j of where.values()) oldNodes[j].remove();
      // Elements on the longest run already in order stay; the rest move or are made.
      const stay = lis(from);
      let s = stay.length - 1;
      /** @type {Node | null} */
      let before = endNew + 1 < n ? out[endNew + 1] : null;
      for (let i = count - 1; i >= 0; i -= 1) {
        const at = start + i;
        const j = from[i];
        let node;
        if (j < 0) {
          node = make(next[at]);
          parent.insertBefore(node, before);
        } else {
          node = oldNodes[j];
          patch(node, next[at], old[j]);
          if (s >= 0 && stay[s] === i) s -= 1;
          else parent.insertBefore(node, before);
        }
        out[at] = node;
        before = node;
      }
    },
    /** The item shown by the element that holds `target` (an event's target), if any. @param {EventTarget | null} target @returns {T | undefined} */
    item(target) {
      let el = /** @type {Node | null} */ (target);
      while (el && el.parentNode !== parent) el = el.parentNode;
      return el ? /** @type {any} */ (el).$i : undefined;
    },
    /**
     * The element showing the item with this key. It remembers the last one it found, so a render
     * that asks for the same key every time it runs (the selected row) does not search the list.
     * @param {unknown} k @returns {Element | undefined}
     */
    node(k) {
      if (found && found.parentNode === parent && key(/** @type {any} */ (found).$i) === k) return found;
      found = undefined;
      for (let i = 0; i < items.length; i += 1) if (key(items[i]) === k) return (found = nodes[i]);
      return undefined;
    },
    get items() {
      return items;
    },
  };
}

/**
 * @template T
 * @param {Element} parent @param {readonly T[]} items @param {Element[]} out @param {number} from @param {number} to
 * @param {(item: T) => Element} make @param {Node | null} before
 */
function appendAll(parent, items, out, from, to, make, before) {
  const frag = document.createDocumentFragment();
  for (let i = from; i < to; i += 1) frag.appendChild((out[i] = make(items[i])));
  parent.insertBefore(frag, before);
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

/**
 * What a component's setup returns: its handlers (named in its markup's on:event attributes)
 * and its render, or several named renders.
 * @typedef {{ render?: (() => void) | Record<string, () => void>, [handler: string]: unknown }} Component
 */

/** @type {WeakMap<Element, { type: string, name: string }[]>} */
const parsed = new WeakMap();
/** @param {Element} el */
function bindings(el) {
  let list = parsed.get(el);
  if (!list) {
    list = (el.getAttribute("data-on") ?? "").split(" ").filter(Boolean).map((b) => {
      const colon = b.indexOf(":");
      return { type: b.slice(0, colon), name: b.slice(colon + 1) };
    });
    parsed.set(el, list);
  }
  return list;
}

/**
 * Starts a component on its server-rendered root: finds its refs, runs `setup`, runs each render
 * once to learn what it reads, and listens (on the root) for the event types the root's
 * `data-events` lists, until `signal` aborts. The server writes `data-events` from every
 * on:event in the component's markup and its templates (known when it compiles), so starting a
 * component never searches its markup for handlers.
 * @param {Element} root
 * @param {(ctx: { root: Element, refs: Record<string, Element>, signal: AbortSignal }) => Component} setup
 * @param {{ signal?: AbortSignal }} [options]
 */
export function mount(root, setup, { signal = new AbortController().signal } = {}) {
  /** @type {Record<string, Element>} */
  const refs = {};
  for (const el of root.querySelectorAll("[data-ref]")) refs[/** @type {string} */ (el.getAttribute("data-ref"))] = el;
  const component = setup({ root, refs, signal });
  const renders = typeof component.render === "function" ? [component.render] : Object.values(component.render ?? {});
  for (const run of renders) {
    /** @type {Render} */
    const r = { run, order: (order += 1), stamp: 0, deps: [] };
    track(r);
    signal.addEventListener("abort", () => { for (const by of r.deps) by.delete(r); r.deps.length = 0; dirty.delete(r); }, { once: true });
  }
  for (const type of (root.getAttribute("data-events") ?? "").split(" ")) {
    if (!type) continue;
    root.addEventListener(
      type,
      (event) => {
        for (let el = /** @type {Element | null} */ (event.target); el; el = el.parentElement) {
          if (el.hasAttribute("data-on")) {
            for (const b of bindings(el)) {
              if (b.type !== type) continue;
              const handler = component[b.name];
              if (typeof handler === "function") handler(event, el);
              else console.error("corten: no handler " + b.name, el);
            }
            if (event.cancelBubble) return;
          }
          if (el === root) return;
        }
      },
      { signal },
    );
  }
  return component;
}
