// @ts-check
// Corten runtime. Served by the corten crate at /corten/{version}/corten.js, minified from this
// file into dist/corten.js by `npm run build:js`, and imported as "corten".
//
// What it does, and nothing else:
// 1. Same-origin navigations: fetch the next page with `corten: nav` and swap it into <main>.
// 2. <form data-patch>: submit with `corten: action` and apply the returned fragments by id (a
//    `<template id data-remove>` removes its element, `<template id data-append>` adds to it). The latest submit of a form wins; a
//    navigation abandons pending ones.
// 3. <corten-live src>: apply fragments streamed as server-sent `patch` events, until an `end`.
// 4. Handlers: `data-on="click:cart_line.confirm($e,@dialog)"` (what `on:click={js::confirm(event, dialog)}`
//    prints) runs the export `confirm` of `{scripts dir}/cart_line.js` with the event and the
//    element with `data-ref="dialog"` in the same component. One listener per event type on the
//    document; modules are imported when markup that names them joins the page, and markup the
//    runtime inserts (a navigation, a patch) waits for its modules, so handlers always run while
//    the event is dispatched. `on:connect` runs when the element joins, and its `signal` aborts
//    when it leaves.
// 5. subscribe(): live rows from a corten::feed (snapshot + ordered patches) for scripts that keep state.
// 6. Hover prefetch of same-origin links, the page's head tags and section stylesheets on
//    navigation (a stylesheet loads before the swap and stays), `aria-current` on
//    shell links with `data-nav`, and the service worker when the shell names one (corten::pwa).
// 7. A page response whose shell key differs from this page's (the user signed in or out, or a
//    deploy changed the runtime or assets) is loaded in full instead of swapped.
// 8. A page that asks to be read again (`ReadAgain`) is fetched again after its delay and
//    swapped in place, however it was reached; a patch or a page that asks for a part
//    (`ReadAgain::from`) has it read again from its route and applied (and such a page is not
//    read whole).
// Without JavaScript, links and forms still work as plain HTML.

import { parseOn } from "./on.js";

const scriptsDir = document.querySelector('meta[name="corten-scripts"]')?.getAttribute("content") ?? "/assets/scripts/";

/** @typedef {import("./on.js").Binding} Binding */
/** @typedef {import("./on.js").Arg} Arg */

// Numbers, money and dates for a script (format.js) are a module of their own, served with the
// app's scripts: only a page whose handlers take a format (`t.js().money(USD)`) loads it, as it
// loads the handlers' own modules.
const FORMAT = "corten-format";

/** Script modules by name, once imported. @type {Map<string, Record<string, unknown>>} */
const modules = new Map();
/** @type {Map<string, Promise<unknown>>} */
const loading = new Map();

/** Modules the first page loads with its own module scripts, until they arrive. @type {Map<string, () => void>} */
const arriving = new Map();
let contentLoaded = false;
document.addEventListener("DOMContentLoaded", () => {
  contentLoaded = true;
  // A module script that failed or never arrived: imported the usual way instead.
  for (const done of arriving.values()) done();
  arriving.clear();
});

/** The page's own module script for script module `name`, if it has one. @param {string} name */
const moduleScript = (name) => /** @type {HTMLScriptElement | null} */ (document.querySelector(`script[type="module"][src="${scriptsDir}${name}.js"]`));

// Corten ends each script module it serves with a "corten:module" event carrying the module's
// name, URL and exports, so a module the page loads with a module script is found without waiting
// on an import(). It is not an API: the runtime takes exports only from the module the page's own
// script tag for that name loaded (its URL), once, and only while the page loads.
document.addEventListener("corten:module", (event) => {
  const { name, url, exports } = /** @type {CustomEvent<{ name: string, url: string, exports: Record<string, unknown> }>} */ (event).detail;
  if (contentLoaded || modules.has(name) || moduleScript(name)?.src !== url) return;
  modules.set(name, exports);
  arriving.get(name)?.();
  arriving.delete(name);
});

/**
 * Imports the script module `name` (`{scripts dir}/name.js`) once. The first page's modules are
 * module scripts of its own, which hand their exports over as they run, before `DOMContentLoaded` and without
 * waiting on an `import()`; markup the runtime inserts later waits for its modules, so a handler
 * always runs while its event is dispatched (and can call `event.preventDefault()`). An event
 * that arrives before its module (a click while the first page still loads) gets the browser's
 * default, as it would without JavaScript.
 * @param {string} name
 */
function load(name) {
  let pending = loading.get(name);
  if (!pending) {
    const url = `${scriptsDir}${name}.js`;
    const fromUrl = () =>
      import(url).then(
        (module) => void modules.set(name, module),
        (error) => console.error(`corten: no script module at ${url}; list its component in App::components`, error),
      );
    if (modules.has(name)) pending = Promise.resolve();
    else if (!contentLoaded && moduleScript(name)) {
      pending = new Promise((done) => arriving.set(name, () => done(undefined))).then(() => (modules.has(name) ? undefined : fromUrl()));
    } else pending = fromUrl();
    loading.set(name, pending);
  }
  return pending;
}

/** Each element's parsed `data-on`, until the attribute changes. @type {WeakMap<Element, { source: string, list: Binding[] }>} */
const parsed = new WeakMap();

/** @param {Element} el */
function bindings(el) {
  const source = el.getAttribute("data-on") ?? "";
  let entry = parsed.get(el);
  if (!entry || entry.source !== source) {
    entry = { source, list: parseOn(source) };
    for (const b of entry.list) if (b.args.some(formatted)) formats.add(b);
    parsed.set(el, entry);
  }
  return entry.list;
}

/** The bindings with a format argument, which need the format module too. @type {WeakSet<Binding>} */
const formats = new WeakSet();

/** @param {Arg} arg @returns {boolean} */
const formatted = (arg) => arg.kind === "f" || (arg.kind === "{" && arg.fields.some(([, field]) => formatted(field)));

/**
 * Imports what binding `b` needs: its module, and the format module when it takes a format.
 * @param {Binding} b
 */
function loadFor(b) {
  return formats.has(b) ? Promise.all([load(b.module), load(FORMAT)]) : load(b.module);
}

/**
 * The root element of the component that owns `from`'s markup (`owner`: of that component, an
 * ancestor of it).
 * @param {Element} from
 * @param {string | null} owner
 */
function scopeOf(from, owner) {
  const scope = owner ? from.closest(`[data-scope="${owner}"]`) : from.closest("[data-scope]");
  if (!scope) throw new Error(`corten: no component root ${owner ? `data-scope="${owner}"` : "[data-scope]"} around`, { cause: from });
  return scope;
}

/**
 * The elements of ref `name` in the component that owns `from`'s markup. A ref is the
 * component's own (`data-ref="rows"` inside its root and not inside a nested component's root) or
 * was put on an element a child component rendered (`data-ref="offer-list/rows"`).
 * @param {Element} from
 * @param {string | null} owner
 * @param {string} name
 */
function findRefs(from, owner, name) {
  const scope = scopeOf(from, owner);
  // Resolved refs are kept per component root until the page's elements change: a click on a
  // row of a 500-row grid then costs a lookup, not a query of every row. Mutations not yet
  // delivered to the observer are taken now, so a handler never gets a stale list.
  const pending = observer.takeRecords();
  if (pending.length) observed(pending);
  let cached = refCache.get(scope);
  if (!cached) refCache.set(scope, (cached = new Map()));
  // Still right while no element with this ref name joined the page and none it found left.
  const changes = refChanges.get(name) ?? 0;
  const hit = cached.get(name);
  if (hit && hit.changes === changes && hit.found.every((el) => el.isConnected)) return hit.found.slice();
  const own = scope.getAttribute("data-scope");
  const qualified = `${own}/${name}`;
  /** @type {Element[]} */
  const found = [];
  const check = (/** @type {Element} */ el) => {
    const ref = el.getAttribute("data-ref");
    if (ref === name ? el.closest("[data-scope]") === scope : ref === qualified && el.closest(`[data-scope="${own}"]`) === scope) found.push(el);
  };
  check(scope);
  for (const el of Array.from(scope.querySelectorAll(`[data-ref="${name}"],[data-ref="${qualified}"]`))) check(el);
  cached.set(name, { changes, found });
  return found.slice();
}

/**
 * How often elements with a ref of this name (`rows`, for `rows` and `offer-list/rows`) joined
 * the page. Only those can add to what a ref resolves to; elements that leave are noticed when a
 * cached ref is next used. So 10,000 rows put in or taken out of a list leave the refs of the
 * list's component cached.
 * @type {Map<string, number>}
 */
const refChanges = new Map();
/** Each component root's resolved refs. @type {WeakMap<Element, Map<string, { changes: number, found: Element[] }>>} */
const refCache = new WeakMap();

/** @param {Element} el @param {Set<string>} names */
function refName(el, names) {
  const ref = el.getAttribute("data-ref");
  if (ref !== null) names.add(ref.slice(ref.indexOf("/") + 1));
}

/** Each component root's kept state. @type {WeakMap<Element, unknown>} */
const states = new WeakMap();

/** The handler running, and the element it is bound on: whose component `keep` is for. @type {[Element, Binding] | null} */
let running = null;

/**
 * The state the running handler's component keeps, made by `init` the first time and kept while
 * the component's root is on the page: `const s = keep(() => ({ paused: false }))`. Handlers are
 * plain functions, so state that lasts between events lives here (or in the DOM) rather than in
 * the module, which every instance of the component shares; each instance keeps its own, with no
 * element passed. The component is the one whose script the handler is in (a component that
 * imports `keep` is always scoped), also when a parent passed the handler to a child. A component
 * has one state, whichever of its handlers asks first: a later call gets that object whatever its
 * `init`, so keep everything in one object (a `const kept = () => keep(() => ({ .. }))` the
 * handlers share). Take it first, before an `await` or anything that dispatches an event: a
 * timer, a listener or a feed's rows the handler sets up use the object it got.
 * @template T
 * @param {() => T} init
 * @returns {T}
 */
export function keep(init) {
  if (!running) throw new Error("corten: keep() outside a handler");
  const root = scopeOf(running[0], running[1].home);
  if (states.has(root)) return /** @type {T} */ (states.get(root));
  const made = init();
  states.set(root, made);
  return made;
}

/** Each element's life, for `signal`: aborted when it leaves the page. @type {Map<Element, AbortController>} */
const lives = new Map();

/** @param {Element} el */
function signalOf(el) {
  let life = lives.get(el);
  if (!life) {
    life = new AbortController();
    lives.set(el, life);
  }
  return life.signal;
}

/** A `^ref` argument with no such element around the event's target: the handler is not for it. */
const SKIP = Symbol("skip");

/**
 * The element with ref `name` (of the component that owns `el`'s markup) that holds the event's
 * target, or nothing.
 * @param {Element} el @param {string | null} owner @param {string} name @param {Event} event
 */
function closestRef(el, owner, name, event) {
  const scope = scopeOf(el, owner);
  const own = scope.getAttribute("data-scope");
  const qualified = `${own}/${name}`;
  for (const node of event.composedPath()) {
    if (!(node instanceof Element)) continue;
    const ref = node.getAttribute("data-ref");
    if (ref === name ? node.closest("[data-scope]") === scope : ref === qualified && node.closest(`[data-scope="${own}"]`) === scope) return node;
    if (node === scope) break;
  }
  return null;
}

/**
 * A message of the app's catalog with the arguments `K`, which a handler fills in and shows with
 * `textContent`: `label({ count: rows.length })`. The markup passes it in the page's locale
 * (`js::count(total, t.js().rows_selected())`), and the compiler checks that the message's
 * arguments are the ones named here.
 * @template {string} K
 * @typedef {(args: Record<K, string | number>) => string} Message
 */

/**
 * The page's number format: `format(1234)` is `1,234` in en, as the server's `t.number` writes it.
 * The markup passes it (`t.js().number()`); a decimal is passed as its text (`"21.5"`).
 * @typedef {(n: number | string) => string} NumberFormat
 */

/**
 * An amount's format in one currency: `format(123450)` (minor units, an integer) is `$1,234.50`
 * in en, as `t.money(Minor(123450), USD)` writes it. The markup passes it (`t.js().money(USD)`).
 * @typedef {(minor: number) => string} MoneyFormat
 */

/**
 * The page's percentage format: `format(0.029)` is `2.9%` in en and `2,9 %` in fr, as the
 * server's `t.percent` writes it, with the fraction digits the markup chose
 * (`t.js().percent(2)`). It takes a ratio; a decimal may be passed as its text (`"0.029"`).
 * @typedef {(ratio: number | string) => string} PercentFormat
 */

/**
 * A date's format: `format(at)` (a `Date`, milliseconds or ISO text) as the server's `t.date`,
 * `t.time` or `t.date_time` writes it, in the zone the markup chose (`t.js().date(DateStyle::Medium)`).
 * @typedef {(at: Date | number | string) => string} DateFormat
 */

/**
 * The function a `Message` argument is: its text with each argument put in.
 * @param {string[]} parts text and argument names, alternating, text first and last
 * @returns {(args: Record<string, string | number>) => string}
 */
function message(parts) {
  return (args) => {
    let text = parts[0];
    for (let i = 1; i < parts.length; i += 2) text += String(args[parts[i]]) + parts[i + 1];
    return text;
  };
}

/**
 * The value of one argument.
 * @param {Arg} arg @param {Element} el @param {Event} event @param {Binding} binding
 * @returns {unknown}
 */
function resolve(arg, el, event, binding) {
  switch (arg.kind) {
    case "v":
      return arg.value;
    case "%":
      return message(arg.parts);
    case "f":
      return /** @type {{ formatter: (spec: import("./format.js").Spec) => unknown }} */ (modules.get(FORMAT)).formatter(arg.spec);
    case "$e":
      return event;
    case "$t":
      return el;
    case "$s":
      return signalOf(el);
    case "$c":
      return scopeOf(el, arg.owner);
    case "*":
      return findRefs(el, arg.owner, arg.name);
    case "#":
      return new Map(findRefs(el, arg.owner, arg.name).map((found) => [found.getAttribute("data-key"), found]));
    case "^":
      return closestRef(el, arg.owner, arg.name, event) ?? SKIP;
    case "{": {
      /** @type {Record<string, unknown>} */
      const group = {};
      for (const [key, field] of arg.fields) {
        const value = resolve(field, el, event, binding);
        if (value === SKIP) return SKIP;
        group[key] = value;
      }
      return group;
    }
    default: {
      const [first] = findRefs(el, arg.owner, arg.name);
      if (!first) throw new Error(`corten: ${binding.module}.${binding.name} takes ref ${arg.name}, and its component has no element with it`);
      return first;
    }
  }
}

/**
 * Runs one handler for `el`, while `event` is dispatched. A module still loading (an event before
 * the first page's scripts are in) runs nothing, so the browser does what it would without
 * JavaScript; `on:connect` waits for it instead.
 * @param {Element} el
 * @param {Binding} binding
 * @param {Event} event
 */
function run(el, binding, event) {
  const module = modules.get(binding.module);
  if (!module || (formats.has(binding) && !modules.has(FORMAT))) {
    const ready = () => modules.has(binding.module) && (!formats.has(binding) || modules.has(FORMAT));
    if (event.type === "connect") loadFor(binding).then(() => ready() && el.isConnected && run(el, binding, event));
    return;
  }
  const handler = module[binding.name];
  if (typeof handler !== "function") {
    console.error(`corten: ${binding.module}.js exports no function ${binding.name}`, el);
    return;
  }
  const outer = running;
  try {
    const args = [];
    for (const arg of binding.args) {
      const value = resolve(arg, el, event, binding);
      if (value === SKIP) return;
      args.push(value);
    }
    running = [el, binding];
    const result = handler(...args);
    if (result instanceof Promise) result.catch(reportError);
  } catch (error) {
    reportError(error);
  } finally {
    running = outer;
  }
}

/** Event types with a listener on the document. */
const listening = new Set();

/** Elements with a handler for events anywhere on the page (`on:document:keydown`). @type {Set<Element>} */
const onPage = new Set();

/** The `data-on` type of a handler for `type` anywhere on the page. */
const PAGE = "document.";

/**
 * Listens for `type` on the document, once: an event that bubbles runs the handlers of every
 * element on its path, innermost first, until one stops it, and then those of the elements on
 * the page with a handler for it anywhere (`on:document:keydown`), each as a listener of its own
 * on the document would; one that does not (`focus`, `mouseenter`) reaches the document only by
 * capture and runs its target's handlers.
 * @param {string} type
 */
function listen(type) {
  if (type === "connect" || listening.has(type)) return;
  listening.add(type);
  const anywhere = PAGE + type;
  document.addEventListener(type, (event) => {
    if (!event.bubbles) return;
    for (const node of event.composedPath()) {
      if (!(node instanceof Element)) continue;
      if (node.hasAttribute("data-on")) for (const b of bindings(node)) if (b.type === type) run(node, b, event);
      // A handler that stops a submit's propagation keeps it from the page's outer handlers, not
      // from the runtime: a data-patch form is still sent as a patch.
      if (event.cancelBubble) return type === "submit" && patchSubmit(event);
    }
    // Each element's page-wide handlers are a listener of their own on the document: stopping the
    // event does not keep it from another's, and an element that joins during the event waits
    // for the next one. A submit one of them stopped is still sent as a patch.
    if (!onPage.size) return;
    for (const el of [...onPage]) {
      if (!el.isConnected) continue;
      for (const b of bindings(el)) if (b.type === anywhere) run(el, b, event);
    }
    if (event.cancelBubble && type === "submit") patchSubmit(event);
  });
  document.addEventListener(
    type,
    (event) => {
      const target = event.composedPath()[0];
      if (event.bubbles || !(target instanceof Element) || !target.hasAttribute("data-on")) return;
      for (const b of bindings(target)) if (b.type === type) run(target, b, event);
    },
    { capture: true },
  );
}

/** Elements whose `on:connect` handlers ran in their current life. @type {Set<Element>} */
const connected = new Set();

/**
 * Wires the markup under `root`: imports the modules its handlers name, listens for their events
 * and runs `on:connect` handlers. The names of the refs in it go in `names`.
 * @param {Element} root
 * @param {Set<string>} names
 * @param {boolean} [inside] only what is inside `root` joined the page
 */
function scan(root, names, inside = false) {
  for (const template of Array.from(root.getElementsByTagName("template"))) modulesOf(template.content);
  const marked = Array.from(root.querySelectorAll("[data-on]"));
  if (!inside) {
    if (root.hasAttribute("data-on")) marked.unshift(root);
    refName(root, names);
  }
  // Rows cloned from one template carry the same refs, so a name is added once per run of them.
  if (!(inside && fewRefNames(root, names))) {
    const refs = root.querySelectorAll("[data-ref]");
    let last = null;
    for (let i = 0; i < refs.length; i += 1) {
      const ref = refs[i].getAttribute("data-ref");
      if (ref !== last && ref !== null) names.add(ref.slice(ref.indexOf("/") + 1));
      last = ref;
    }
  }
  for (const el of marked) {
    let connects = false;
    for (const b of bindings(el)) {
      loadFor(b);
      if (b.type === "connect") connects = true;
      else if (b.type.startsWith(PAGE)) {
        onPage.add(el);
        listen(b.type.slice(PAGE.length));
      } else listen(b.type);
    }
    if (connects && !connected.has(el)) {
      connected.add(el);
      const event = new Event("connect");
      for (const b of bindings(el)) if (b.type === "connect") run(el, b, event);
    }
  }
}

/**
 * Adds the names of the refs under `root` to `names` when there are only a few, and says whether
 * it found them all. Many children put in at once (rows() or a patch filling a list) are clones of
 * a few components, so each query looks for a ref not yet seen and the last finds none: a few
 * passes of the browser's selector engine instead of reading 20,000 attributes from script.
 * @param {Element} root @param {Set<string>} names
 */
function fewRefNames(root, names) {
  let selector = "[data-ref]";
  for (let seen = 0; seen < FEW_NAMES; seen += 1) {
    const el = root.querySelector(selector);
    if (!el) return true;
    const ref = /** @type {string} */ (el.getAttribute("data-ref"));
    names.add(ref.slice(ref.indexOf("/") + 1));
    selector += `:not([data-ref="${CSS.escape(ref)}"])`;
  }
  return false;
}

/** Distinct ref names fewRefNames looks for, one query each, before scan reads every ref. */
const FEW_NAMES = 8;

/**
 * Ends the life of every element that left the page: its `signal` aborts, and `on:connect` runs
 * again if it comes back. It runs after the mutations that removed it, so an element moved within
 * the page (removed and inserted in one task) keeps its life.
 */
function sweep() {
  for (const [el, life] of lives) {
    if (el.isConnected) continue;
    lives.delete(el);
    life.abort();
  }
  for (const el of connected) if (!el.isConnected) connected.delete(el);
  for (const el of onPage) if (!el.isConnected) onPage.delete(el);
}

/**
 * More elements than this joining in one change, and at least half of their parent's children,
 * are scanned with one query of the parent (see `observed`).
 */
const MANY = 16;

/**
 * Markup that joins the page (a navigation, a patch, a script's insert) is wired;
 * markup that leaves it ends.
 * @param {MutationRecord[]} records
 */
function observed(records) {
  let removed = false;
  /** The names of the refs of the elements that joined. @type {Set<string>} */
  const names = new Set();
  /** Parents that got many children at once: scanned whole, with one query each. @type {Set<Element>} */
  const wide = new Set();
  for (const record of records) {
    if (record.removedNodes.length) removed = true;
    const added = record.addedNodes;
    const parent = record.target;
    if (added.length > MANY && parent instanceof Element && added.length * 2 >= parent.childElementCount) wide.add(parent);
    else for (const node of Array.from(added)) if (node instanceof Element && node.isConnected) scan(node, names);
  }
  // Wiring is the same however often it runs, so a parent scanned whole may hold rows wired before.
  for (const parent of wide) if (parent.isConnected) scan(parent, names, true);
  for (const name of names) refChanges.set(name, (refChanges.get(name) ?? 0) + 1);
  if (removed && (lives.size || connected.size || onPage.size)) sweep();
}

const observer = new MutationObserver(observed);
observer.observe(document.documentElement, { childList: true, subtree: true });

/**
 * @template Row
 * @typedef {{ op: "upsert", id: string, value: Row } | { op: "remove", id: string }} FeedChange
 */

/**
 * The URL of a feed whose rows are `Row`: what a handler parameter `@param {FeedUrl<Level>} feed`
 * gets from a route whose `#[route(..)]` says `rows = Level` (Corten checks the route when the
 * crate compiles), and what `subscribe()` takes. At run time it is the URL, a string; its
 * `"corten.rows"` is a type only, which no script reads.
 * @template Row
 * @typedef {string & { readonly "corten.rows": Row[] }} FeedUrl
 */

/**
 * @template Row
 * @typedef {object} FeedHandlers
 * @property {(rows: Row[], seq: number, epoch: string) => void} snapshot Every row; replaces what the
 * script had. `seq` counts writes within `epoch`, the feed's generation: after a restart or a
 * reset to another log the epoch differs, and its `seq` is not comparable with the old one's.
 * @property {(changes: FeedChange<Row>[], seq: number, receivedAt: number) => void} patch One write, in order, never twice.
 * @property {(status: "syncing" | "live" | "reconnecting") => void} [status]
 */

/**
 * Live rows from a `corten::feed` at `url`: calls `snapshot` with every row, then `patch` with each
 * later write in order. Reconnects resume where they stopped; a gap, an unknown cursor or a client
 * that fell behind loads a new snapshot. It runs until `signal` aborts (in a handler, pass
 * `signal`: the subscription ends when the element leaves the page), or, without one, for the
 * life of the page. `Row` comes from `url`, a `FeedUrl<Row>` handler parameter.
 * @template Row
 * @param {FeedUrl<Row>} url
 * @param {FeedHandlers<Row>} handlers
 * @param {{ signal?: AbortSignal }} [options]
 * @returns {void}
 */
export function subscribe(url, handlers, { signal } = {}) {
  if (signal?.aborted) return;
  /** @type {EventSource | null} */
  let source = null;
  /** @type {AbortController | null} */
  let request = null;
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let timer;
  let seq = 0;
  let failures = 0;
  const status = handlers.status ?? (() => {});
  signal?.addEventListener(
    "abort",
    () => {
      clearTimeout(timer);
      request?.abort();
      source?.close();
    },
    { once: true },
  );

  /** Runs `next` after a backoff that grows with each failure in a row. @param {() => void} next */
  function retry(next) {
    failures += 1;
    clearTimeout(timer);
    timer = setTimeout(next, Math.min(30_000, 500 * 2 ** failures));
  }

  async function sync() {
    source?.close();
    source = null;
    request?.abort();
    if (signal?.aborted) return;
    status("syncing");
    const current = (request = new AbortController());
    try {
      const res = await fetch(url, { headers: { accept: "application/json" }, signal: current.signal });
      if (!res.ok) throw new Error(`corten: feed ${url} answered ${res.status}`);
      /** @type {{ epoch: string, seq: number, rows: Row[] }} */
      const snap = await res.json();
      failures = 0;
      seq = snap.seq;
      handlers.snapshot(snap.rows, snap.seq, snap.epoch);
      listen(`${url}/live?since=${encodeURIComponent(`${snap.epoch}-${snap.seq}`)}`);
    } catch (error) {
      if (current.signal.aborted) return;
      console.error(error);
      retry(sync);
    }
  }

  /** @param {string} liveUrl */
  function listen(liveUrl) {
    const es = new EventSource(liveUrl);
    source = es;
    let cursor = new URL(liveUrl, location.href).searchParams.get("since") ?? "";
    es.addEventListener("open", () => {
      failures = 0;
      status("live");
    });
    // EventSource retries by itself and sends Last-Event-ID, so the server resumes after `seq`.
    // It gives up for good on an HTTP error (a load balancer with no healthy server, say); then
    // listen again from the last event, with backoff, so no snapshot is needed.
    es.addEventListener("error", () => {
      status("reconnecting");
      if (es.readyState === EventSource.CLOSED && source === es) retry(() => listen(`${url}/live?since=${encodeURIComponent(cursor)}`));
    });
    es.addEventListener("sync", () => sync());
    es.addEventListener("patch", (event) => {
      const receivedAt = performance.now();
      /** @type {{ seq: number, changes: FeedChange<Row>[] }} */
      const batch = JSON.parse(/** @type {MessageEvent} */ (event).data);
      if (batch.seq <= seq) return;
      if (batch.seq !== seq + 1) return void sync();
      seq = batch.seq;
      cursor = /** @type {MessageEvent} */ (event).lastEventId;
      handlers.patch(batch.changes, batch.seq, receivedAt);
    });
  }

  sync();
}

/**
 * Markup from the server, as a fragment. A template parses it with scripting off, so a
 * `<noscript>`'s content would come out as live elements (a `<meta http-equiv="refresh">`
 * reloads the page once it is in the document); with scripting on a browser never shows it, so
 * it is dropped.
 * @param {string} html
 */
function parse(html) {
  const template = document.createElement("template");
  template.innerHTML = html;
  for (const noscript of Array.from(template.content.querySelectorAll("noscript"))) noscript.replaceChildren();
  return template.content;
}

/**
 * Imports the modules the handlers in `node` name, including those in its templates' content
 * (rows a script will move into the page).
 * @param {ParentNode} node
 * @returns {Promise<unknown>}
 */
function modulesOf(node) {
  /** @type {Promise<unknown>[]} */
  const pending = [];
  if (node instanceof Element && node.hasAttribute("data-on")) for (const b of bindings(node)) pending.push(loadFor(b));
  for (const el of Array.from(node.querySelectorAll("[data-on]"))) for (const b of bindings(el)) pending.push(loadFor(b));
  for (const template of Array.from(node.querySelectorAll("template"))) pending.push(modulesOf(template.content));
  return Promise.all(pending);
}

/**
 * Applies a patch: every top-level element in `html` replaces the element with its id, a
 * `<template id="row-7" data-remove>` removes it, a `<template id="rows" data-append>` adds
 * its content at the end of it, and a `<template data-corten-read-again>` reads the patch again
 * ({@link readPatchAgain}). The modules the new markup's handlers name are
 * imported first, so its handlers run from the first event. The server's markup wins, except for
 * what the user is doing: a field they edited keeps their text, a dialog, `<details>` or popover
 * they opened stays open (a modal dialog modal) unless the server changed it, and focus stays where
 * it was, or near it when the patch takes its element away ({@link focusNear}).
 * @param {string} html
 * @param {AbortSignal} [signal] Nothing changes once it aborts.
 */
export function patch(html, signal) {
  return apply(html, signal, null);
}

/**
 * {@link patch}, for a form's response: `sent` holds the values the form sent, so a field counts
 * as edited only when it changed after the request left; `took`, that the answer accepted them.
 * @param {string} html @param {AbortSignal | undefined} signal @param {Map<Element, string | undefined> | null} sent @param {boolean} [took]
 */
async function apply(html, signal, sent, took) {
  const content = parse(html);
  await modulesOf(content);
  if (signal?.aborted) return;
  /** The elements this patch puts on the page. @type {Element[]} */
  const put = [];
  let again;
  // Whether it removes a nav link, which can leave another of its nav current.
  let navs = false;
  for (const el of Array.from(content.children)) {
    const id = idOf(el);
    const target = id ? document.getElementById(id) : null;
    if (el.hasAttribute("data-corten-read-again")) again = el;
    else if (el instanceof HTMLTemplateElement && el.hasAttribute("data-remove")) {
      // Focus in it goes to the nearest element around it that can take focus.
      const up = target?.contains(document.activeElement) && target.parentElement;
      navs ||= !!target && holdsNav(target);
      target?.remove();
      if (up) focusNear(up);
    }
    else if (!target) console.error(`corten: patch element <${el.localName}> needs an id that exists on the page`, el);
    else if (el instanceof HTMLTemplateElement && el.hasAttribute("data-append")) {
      put.push(...Array.from(el.content.children));
      serve(el.content);
      target.append(el.content);
    } else {
      replaceKeeping(target, el, sent, took);
      put.push(el);
    }
  }
  // The server marked a nav link in the patch against the request that made it (a form's action),
  // not this page: mark it as a navigation would. Only then: a patch without one (a ticker's)
  // only looks through what it put.
  if (navs || put.some(holdsNav)) markCurrent();
  if (again) readPatchAgain(again, put);
}

/**
 * A patch's `ReadAgain` (`<template data-corten-read-again="2" data-from="/alerts?after=r7">`):
 * after that many seconds, once the tab is visible and no form's answer is on its way, fetches
 * `data-from` as a GET `data-patch` form would and applies the patch it answers, which may ask
 * again, whatever its status (a handler that cannot read says so as a patch). It lasts while the
 * page and the elements the patch put on it are there: a later patch of them or a page swapped in
 * stops it, and so does a read that fails without a patch (the server logged it) or offline.
 * @param {Element} tag @param {Element[]} put
 */
function readPatchAgain(tag, put) {
  const from = /** @type {string} */ (tag.getAttribute("data-from"));
  const page = swapped;
  const here = () => page === swapped && put.every((el) => el.isConnected);
  readLater(Number(tag.getAttribute("data-corten-read-again")), here, async () => {
    // Offline or unreachable: the part stays as it is.
    const res = await fetch(from, { headers: { corten: "action" } }).catch(() => {});
    if (!res?.headers.has("corten-patch")) {
      // A success that is not a patch is the app's mistake (the route answered its page).
      if (res?.ok) console.error("corten: ReadAgain needs a patch from", from, res);
      return;
    }
    const html = await res.text();
    if (here()) await apply(html, undefined, null);
  });
}

/** A page's request to read one of its parts again (`ReadAgain::from` in the page). */
const PART_AGAIN = "template[data-corten-read-again][data-from]";

/**
 * The parts a page asks to have read again (`ReadAgain::from` in the page, as in a patch): each
 * such tag in `<main>` starts the same read a patch's does once the page is on the screen, and the
 * patch it answers may ask again. It lasts while the page, the tag and the elements with an id
 * beside it are there: a patch of one of them (a form's answer, which brings its own read) stops it.
 * @param {Element} main
 */
function readPartsAgain(main) {
  for (const tag of Array.from(main.querySelectorAll(PART_AGAIN))) {
    readPatchAgain(tag, [tag, ...Array.from(/** @type {Element} */ (tag.parentElement).children).filter((el) => el.id)]);
  }
}

/**
 * A field's state as text: what the user has (or, with `initial`, what the markup set; a file
 * input's is its first file's name). Buttons and hidden inputs are not fields here: undefined.
 * @param {Element} f @param {boolean} [initial] @returns {string | undefined}
 */
function stateOf(f, initial) {
  if (f instanceof HTMLSelectElement) {
    const marks = Array.from(f.options, (o) => +(initial ? o.defaultSelected : o.selected));
    if (initial && !f.multiple) {
      // What the browser selects from the markup: the last option marked selected, else the
      // first enabled one (none in a list box).
      const last = marks.lastIndexOf(1);
      const i = last >= 0 ? last : f.size > 1 ? -1 : Array.from(f.options).findIndex((o) => !o.disabled);
      marks.fill(0);
      if (i >= 0) marks[i] = 1;
    }
    return marks.join("");
  }
  if (f instanceof HTMLTextAreaElement) return initial ? f.defaultValue : f.value;
  if (!(f instanceof HTMLInputElement) || /^(hidden|submit|button|reset|image)$/.test(f.type)) return;
  if (/^(checkbox|radio)$/.test(f.type)) return String(initial ? f.defaultChecked : f.checked);
  return initial ? f.defaultValue : f.value;
}

/**
 * An element's id, read from its attribute: a form's `id` property is its control named `id`
 * when it has one (see formProperty), and any element may be a form.
 * @param {Element} el
 */
const idOf = (el) => el.getAttribute("id");

/**
 * The form's own property `name`, past a control named like it: in the DOM `<button name="action">`
 * makes `form.action` that button, and `<input name="id">` makes `form.id` the input, so the
 * runtime never reads a form's properties or calls its methods directly.
 * @param {HTMLFormElement} form
 * @param {string} name
 * @returns {any}
 */
const formProperty = (form, name) => Reflect.get(HTMLFormElement.prototype, name, form);

/**
 * Calls the form's own method `name` (see formProperty).
 * @param {HTMLFormElement} form
 * @param {string} name
 * @param {...any} args
 */
const formCall = (form, name, ...args) => formProperty(form, name).call(form, ...args);

/** A random UUID v4. `crypto.randomUUID` is only there in secure contexts. */
function uuid() {
  const b = crypto.getRandomValues(new Uint8Array(16));
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const hex = Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/**
 * Gives the form's submit keys (`SubmitKey::field`, `data-submit`) new values once a submission
 * has its answer, so the next submit is a new one. Until then a retry sends the same key.
 * @param {HTMLFormElement} form
 */
function renewSubmitKeys(form) {
  for (const field of Array.from(/** @type {HTMLFormControlsCollection} */ (formProperty(form, "elements")))) {
    if (field instanceof HTMLInputElement && field.hasAttribute("data-submit")) {
      field.value = uuid();
    }
  }
}

/** The fields of a form and their state, when it is sent. @param {HTMLFormElement} form */
function sentState(form) {
  /** @type {Map<Element, string | undefined>} */
  const sent = new Map();
  for (const f of Array.from(/** @type {HTMLFormControlsCollection} */ (formProperty(form, "elements")))) sent.set(f, stateOf(f));
  return sent;
}

/**
 * The element in `fresh` (the new markup for `root`) that stands for `el` (inside `root`). An
 * element with an id is the one with that id. Otherwise, within its nearest ancestor with an id
 * (or the root): the one with its name (and value, for a checkbox or radio) when exactly one has
 * it on both sides, else (unless `exact`) the one at the same place.
 * @param {Element} root @param {Element} fresh @param {Element} el @param {boolean} [exact]
 * @returns {Element | null}
 */
function counterpart(root, fresh, el, exact) {
  if (el === root) return fresh;
  const id = idOf(el);
  if (id) return fresh.querySelector(`#${CSS.escape(id)}`);
  let anchor = /** @type {Element} */ (el.parentElement);
  while (anchor !== root && !idOf(anchor)) anchor = /** @type {Element} */ (anchor.parentElement);
  const home = anchor === root ? fresh : counterpart(root, fresh, anchor);
  const name = el.getAttribute("name");
  if (!home) return null;
  if (name) {
    const selector = named(el, name);
    const theirs = home.querySelectorAll(selector);
    if (theirs.length === 1 && anchor.querySelectorAll(selector).length === 1) return theirs[0];
  }
  if (exact) return null;
  /** @type {Element | undefined} */
  let found = home;
  for (let at = el, path = []; ; at = /** @type {Element} */ (at.parentElement)) {
    if (at === anchor) {
      for (const i of path) found = found?.children[i];
      return found?.localName === el.localName ? found : null;
    }
    path.unshift(Array.prototype.indexOf.call(/** @type {Element} */ (at.parentElement).children, at));
  }
}

/** Whether each `<details>` and `<dialog>` was open as the server sent it (its `open` attribute
 * follows the user, so the markup's is kept here). */
const served = new WeakMap();

/** Records what `root` and the elements in it were sent as. @param {ParentNode} root */
function serve(root) {
  for (const el of [root, ...Array.from(root.querySelectorAll("details, dialog"))]) {
    if (el instanceof HTMLDetailsElement || el instanceof HTMLDialogElement) served.set(el, el.open);
  }
}

/**
 * What the user opened or closed in `root` (a `<dialog>`, modal or not, a `<details>`, a shown
 * popover), applied to its counterpart in `fresh` once that is in the page. One the user left as
 * the server sent it takes the fresh markup's state, so a server that sends a dialog closed after
 * sending it open closes it.
 * @param {Element} root @param {Element} fresh @returns {() => void}
 */
function opened(root, fresh) {
  /** @type {(() => void)[]} */
  const later = [];
  for (const el of [root, ...Array.from(root.querySelectorAll("details, dialog, [popover]"))]) {
    const dialog = el instanceof HTMLDialogElement;
    // A dialog or popover only by id: by place, another row's could open over the page.
    const to = counterpart(root, fresh, el, dialog || el.hasAttribute("popover"));
    if (!to || to.localName !== el.localName) continue;
    if (el.hasAttribute("popover") && el.matches(":popover-open") && to.hasAttribute("popover")) {
      later.push(() => /** @type {HTMLElement} */ (to).showPopover());
    }
    if (!(dialog || el instanceof HTMLDetailsElement)) continue;
    const now = /** @type {HTMLDialogElement} */ (el).open;
    // Unchanged since the server sent it (or unknown, markup a script added): the fresh markup
    // says. Changed by the user: theirs, which is also what a server that changed it now says.
    if (now === (served.has(el) ? served.get(el) : /** @type {HTMLDialogElement} */ (to).open)) continue;
    if (!dialog) /** @type {HTMLDetailsElement} */ (to).open = now;
    else if (!now) to.removeAttribute("open");
    else {
      const modal = el.matches(":modal");
      const dialogTo = /** @type {HTMLDialogElement} */ (to);
      // Modal again, not just open: the top layer, the backdrop and the rest of the page inert.
      later.push(() => {
        if (!modal) return dialogTo.show();
        dialogTo.removeAttribute("open");
        dialogTo.showModal();
      });
    }
  }
  return () => {
    for (const open of later) {
      try {
        open();
      } catch {}
    }
  };
}

/**
 * Replaces `root` with `fresh`, carrying over the fields the user edited (since `sent`, for a
 * field the form sent; else since the markup set them), what the user opened, the focus and the
 * caret. Markup cannot give back a file input's files, so it keeps them unless the answer `took`
 * the ones it sent.
 * @param {Element} root @param {Element} fresh @param {Map<Element, string | undefined> | null} sent @param {boolean} [took]
 */
function replaceKeeping(root, fresh, sent, took) {
  serve(fresh);
  const reopen = opened(root, fresh);
  const focused = document.activeElement;
  const inside = focused && focused !== document.body && root.contains(focused);
  const next = inside ? counterpart(root, fresh, focused) : null;
  /** File inputs of `fresh` and the files to give them once they are in the page. @type {[any, FileList | null][]} */
  const files = [];
  for (const f of [root, ...Array.from(root.querySelectorAll("input, textarea, select"))]) {
    const now = stateOf(f);
    // Only a field found by id or name gets the user's text: by place, rows that moved could
    // put it into another row's field.
    const field = /** @type {HTMLInputElement} */ (f);
    const file = field.type === "file";
    const to = now !== undefined && (now !== (sent?.has(f) ? sent.get(f) : stateOf(f, true)) || (file && !took)) && counterpart(root, fresh, f, true);
    if (!to || to.localName !== f.localName) continue;
    if (file) files.push([to, field.files]);
    else if (f instanceof HTMLSelectElement) {
      // Every chosen option, by value, so a multiple select keeps them all.
      const chosen = new Set(Array.from(f.selectedOptions, (o) => o.value));
      for (const o of Array.from(/** @type {HTMLSelectElement} */ (to).options)) o.selected = chosen.has(o.value);
    } else if (/^(checkbox|radio)$/.test(field.type)) /** @type {HTMLInputElement} */ (to).checked = field.checked;
    else /** @type {HTMLInputElement} */ (to).value = field.value;
  }
  root.replaceWith(fresh);
  for (const [to, chosen] of files) to.files = chosen;
  reopen();
  if (next instanceof HTMLElement) {
    next.focus({ preventScroll: true });
    try {
      const { selectionStart, selectionEnd, selectionDirection } = /** @type {HTMLInputElement} */ (focused);
      if (selectionStart !== null) /** @type {HTMLInputElement} */ (next).setSelectionRange(selectionStart, selectionEnd, selectionDirection ?? undefined);
    } catch {}
  }
  // The focused element went with the patch, or its counterpart cannot take focus.
  if (inside && document.activeElement === document.body) focusNear(focused, root, fresh);
}

/**
 * Focuses `el`, or else the nearest element around it that can take focus, so a keyboard or
 * screen reader user whose element a patch took away stays where they were rather than at the
 * top of the document. With `fresh`, `el` is in `root`, which `fresh` replaced: each element up
 * to `root` stands for its counterpart in `fresh`, then the walk goes on from `fresh`'s parent.
 * One that is not focusable is made so for the move (`tabindex="-1"`), until focus leaves it
 * for another element (not for another window, which leaves it focused).
 * @param {Element | null | undefined} el @param {Element} [root] @param {Element} [fresh]
 */
function focusNear(el, root, fresh) {
  while (el && el !== document.body) {
    const to = fresh ? counterpart(/** @type {Element} */ (root), fresh, el) : el;
    if (to instanceof HTMLElement) {
      const made = !to.hasAttribute("tabindex");
      if (made) to.tabIndex = -1;
      to.focus({ preventScroll: true });
      if (document.activeElement === to) {
        if (made) {
          const off = () => document.activeElement !== to && (to.removeAttribute("tabindex"), to.removeEventListener("blur", off));
          to.addEventListener("blur", off);
        }
        return;
      }
      if (made) to.removeAttribute("tabindex");
    }
    if (fresh && el === root) {
      el = fresh.parentElement;
      fresh = undefined;
    } else el = el.parentElement;
  }
}

/**
 * Swaps a page response (`<title>`, the page's head tags, then the page) into <main>, once the
 * modules its handlers name are imported and the section stylesheets it links are loaded. A
 * stylesheet that fails to load makes the runtime load the page in full rather than show it
 * unstyled.
 * @param {string} html
 * @param {AbortSignal} [signal]
 * @param {boolean} [stay] keep the user's place: the scroll position (Chromium can reset it when
 *   <main> empties), what scrolls inside the page and the `<details>` open on screen
 */
async function swapPage(html, signal, stay) {
  const content = parse(html);
  const [, styled] = await Promise.all([modulesOf(content), stylesOf(content)]);
  signal?.throwIfAborted();
  if (!styled) {
    location.reload();
    return;
  }
  const title = content.querySelector("title");
  if (title) {
    document.title = title.textContent ?? "";
    title.remove();
  }
  const tags = Array.from(content.querySelectorAll("[data-corten-head]"));
  for (const old of Array.from(document.head.querySelectorAll("[data-corten-head]"))) old.remove();
  document.head.append(...tags);
  const main = document.querySelector("main");
  if (!main) throw new Error("corten: the shell must render a <main> element");
  const view = stay ? viewOf(main, content) : null;
  serve(content);
  const [x, y] = [scrollX, scrollY];
  main.replaceChildren(content);
  scrolledIn.clear();
  view?.();
  if (stay) scrollTo(x, y);
  swapped = html;
  markCurrent();
  readAgainLater();
  readPartsAgain(main);
}

/**
 * The elements the user (or a script) scrolled since the page was swapped in: noted as they
 * scroll, so keeping a page's place does not read the scroll of every element in it.
 * @type {Set<Element>}
 */
const scrolledIn = new Set();
document.addEventListener("scroll", (event) => event.target instanceof Element && scrolledIn.add(event.target), { capture: true, passive: true });

/**
 * What the user sees in `root` that a swap to `fresh` keeps when it keeps their place: a
 * `<details>` open on screen stays open, one they closed stays closed (the rest take the new
 * markup's state), and an element that scrolls on its own (a filter rail) keeps its scroll, so
 * the control focus returns to is open and in view. Each is matched as a patch matches it. Read
 * before `fresh` is inserted; the function it returns applies it after. The scrolled elements are
 * those a `scroll` event named and the ones around the focused control, since that event comes at
 * the next frame and a swap can land first (a control focus scrolled into view, a slow machine).
 * @param {Element} root @param {DocumentFragment} fresh @returns {() => void}
 */
function viewOf(root, fresh) {
  const to = /** @type {Element} */ (/** @type {unknown} */ (fresh));
  /** @type {[HTMLDetailsElement, boolean][]} */
  const open = [];
  for (const el of Array.from(root.querySelectorAll("details"))) {
    if (!(el.open || served.get(el))) continue;
    const there = counterpart(root, to, el);
    if (there instanceof HTMLDetailsElement) open.push([there, el.open]);
  }
  /** @type {[Element, number, number][]} */
  const scrolled = [];
  const around = new Set(scrolledIn);
  for (let el = document.activeElement; el && el !== root; el = el.parentElement) around.add(el);
  for (const el of around) {
    if (!root.contains(el) || !(el.scrollTop || el.scrollLeft)) continue;
    const there = counterpart(root, to, el);
    if (there?.localName === el.localName) scrolled.push([/** @type {Element} */ (there), el.scrollTop, el.scrollLeft]);
  }
  return () => {
    for (const [el, now] of open) el.open = now;
    // A restored scroll's event comes later too, and the next swap may come first.
    for (const [el, top, left] of scrolled) el.scrollTo(left, top), scrolledIn.add(el);
  };
}

/**
 * Moves the section stylesheets a page response links (`Area::components`) out of it, adds those this
 * document lacks to <head> and resolves when they have loaded: true, or false when one failed.
 * They are never removed: their rules only match their own components' classes.
 * @param {DocumentFragment} content
 * @returns {Promise<boolean>}
 */
async function stylesOf(content) {
  const have = Array.from(document.head.querySelectorAll("link[data-corten-style]"), (link) => link.getAttribute("href"));
  const links = /** @type {NodeListOf<HTMLLinkElement>} */ (content.querySelectorAll("link[data-corten-style]"));
  const loads = Array.from(links, (link) => {
    link.remove();
    if (have.includes(link.getAttribute("href"))) return true;
    document.head.append(link);
    return new Promise((resolve) => {
      link.onload = () => resolve(true);
      link.onerror = () => resolve(false);
    });
  });
  return (await Promise.all(loads)).every(Boolean);
}

/** A nav link the runtime keeps `aria-current` on (`Req::nav`). */
const NAV = "a[href][data-nav]";

/**
 * True when `el` is or holds a nav link. Called through the prototype: a form's field named
 * `matches` would hide the form's own.
 * @param {Element} el
 */
const holdsNav = (el) => Element.prototype.matches.call(el, NAV) || !!Element.prototype.querySelector.call(el, NAV);

/**
 * Sets `aria-current="page"` on `<a data-nav>` links to this page (`data-nav="section"`: to this
 * page or any page under it) and removes it from the rest, after a navigation and after a patch
 * that puts one on the page or removes one. Of the links of one `data-nav-group` (`Req::nav_of`),
 * only the deepest that holds the page is current. The shell is not re-rendered by client
 * navigation, so its menu would otherwise keep pointing at the first page.
 */
function markCurrent() {
  const here = location.pathname;
  /** Each link, its group (itself when it has none) and how deep it is when it holds the page. @type {[Element, Element | string, number][]} */
  const links = [];
  /** The deepest link that holds the page in each group. @type {Map<Element | string, number>} */
  const deepest = new Map();
  for (const link of Array.from(document.querySelectorAll(NAV))) {
    const path = new URL(/** @type {HTMLAnchorElement} */ (link).href).pathname;
    const base = path.replace(/\/+$/, "");
    const holds = here === path || (link.getAttribute("data-nav") === "section" && here.startsWith(`${base}/`));
    const group = link.getAttribute("data-nav-group") ?? link;
    const depth = holds ? base.length : -1;
    links.push([link, group, depth]);
    deepest.set(group, Math.max(deepest.get(group) ?? -1, depth));
  }
  for (const [link, group, depth] of links) {
    if (depth >= 0 && depth === deepest.get(group)) {
      if (link.getAttribute("aria-current") !== "page") link.setAttribute("aria-current", "page");
    } else if (link.hasAttribute("aria-current")) link.removeAttribute("aria-current");
  }
}

/** The shell key the server printed for this document. */
const shellKey = document.querySelector('meta[name="corten-shell"]')?.getAttribute("content") ?? null;

/**
 * True when a page response belongs to another shell: the document must be loaded in full.
 * @param {Response} res
 */
function shellChanged(res) {
  const key = res.headers.get("corten-shell");
  return key !== null && shellKey !== null && key !== shellKey;
}

/**
 * Moves focus to the first field the server marked invalid in `form` (or in its replacement,
 * found by id), else in <main>; unless the user has moved on to something outside it.
 * @param {HTMLFormElement} form
 */
function focusInvalid(form) {
  const id = formProperty(form, "id");
  const home = (formProperty(form, "isConnected") ? form : id && document.getElementById(id)) || document.querySelector("main");
  const active = document.activeElement;
  if (active && active !== document.body && !home?.contains(active)) return;
  const field = home?.querySelector('[aria-invalid="true"]') ?? document.querySelector('main [aria-invalid="true"]');
  if (field instanceof HTMLElement) field.focus();
}

/**
 * Navigation responses fetched ahead of a likely click, by URL without the hash, oldest first.
 * At most `PREFETCH_MAX`: a page hovered and never followed is dropped (its body cancelled) once
 * newer ones push it out or it goes stale, so a long session does not hold every page it hovered.
 * @type {Map<string, { at: number, response: Promise<Response> }>}
 */
const prefetched = new Map();
const PREFETCH_FRESH_MS = 10_000;
const PREFETCH_MAX = 8;

/** @param {{ response: Promise<Response> } | undefined} entry */
function discard(entry) {
  entry?.response.then((res) => res.body?.cancel()).catch(() => {});
}

/** @param {URL} url */
function pageKey(url) {
  const key = new URL(url);
  key.hash = "";
  return key.href;
}

/** @param {EventTarget | null} target */
function prefetch(target) {
  // Without the Navigation API a click is a full page load, which cannot use a prefetched swap.
  if (!("navigation" in window)) return;
  const link = target instanceof Element ? target.closest("a[href]") : null;
  if (!(link instanceof HTMLAnchorElement) || link.target || link.hasAttribute("download")) return;
  if (link.closest("[data-reload], [data-no-prefetch]")) return;
  const url = new URL(link.href, location.href);
  if (url.origin !== location.origin || pageKey(url) === pageKey(new URL(location.href))) return;
  const key = pageKey(url);
  // The user asked the browser to save data.
  if (/** @type {any} */ (navigator).connection?.saveData) return;
  const now = performance.now();
  const hit = prefetched.get(key);
  if (hit && now - hit.at < PREFETCH_FRESH_MS) return;
  prefetched.delete(key);
  discard(hit);
  for (const [old, entry] of prefetched) {
    if (prefetched.size < PREFETCH_MAX && now - entry.at < PREFETCH_FRESH_MS) break;
    prefetched.delete(old);
    discard(entry);
  }
  const response = fetch(key, { headers: { corten: "nav" }, priority: "low" });
  response.catch(() => prefetched.delete(key));
  prefetched.set(key, { at: now, response });
}

/** @type {ReturnType<typeof setTimeout> | undefined} */
let hoverTimer;
document.addEventListener("pointerover", (event) => {
  clearTimeout(hoverTimer);
  // A short dwell, so moving the pointer across a list does not fetch every link.
  hoverTimer = setTimeout(() => prefetch(event.target), 65);
});
document.addEventListener("pointerout", () => clearTimeout(hoverTimer));
document.addEventListener("touchstart", (event) => prefetch(event.target), { passive: true });

/**
 * @param {URL} url
 * @param {AbortSignal} signal
 * @param {Response} [redirected] the answer to this URL, already fetched by a navigation that
 *   the server redirected here
 * @param {boolean} [again] the page reading itself again (`ReadAgain`): never a prefetched
 *   answer, not marked busy, and not swapped when nothing changed
 * @param {ReturnType<typeof placeOf>} [place] where a `data-keep-place` navigation leaves the
 *   user, handed on when the server redirects
 */
async function visit(url, signal, redirected, again, place) {
  const key = pageKey(url);
  const hit = again ? undefined : prefetched.get(key);
  prefetched.delete(key);
  const fresh = hit && performance.now() - hit.at < PREFETCH_FRESH_MS;
  if (!fresh) discard(hit);
  /** @type {Response} */
  let res;
  const main = again ? null : document.querySelector("main");
  main?.setAttribute("aria-busy", "true");
  try {
    res = redirected
      ? redirected
      : hit && fresh
        ? await hit.response.catch(() => fetch(url, { headers: { corten: "nav" }, signal }))
        : await fetch(url, { headers: { corten: "nav" }, signal });
  } catch (error) {
    if (signal.aborted) throw error;
    // Offline or the server is unreachable: load the URL as a normal navigation, which the
    // service worker (if any) answers with the offline page, and the browser otherwise explains.
    location.reload();
    return;
  } finally {
    main?.removeAttribute("aria-busy");
  }
  signal.throwIfAborted();
  // Sent to a place on a page (`route.at(id)`): the server names it here, since `fetch` follows a
  // redirect but drops its `#id`.
  const to = res.headers.get("corten-location");
  if (to) {
    navigation.navigate(to, { history: "replace", info: { [LOAD]: true, [KEEP_PLACE]: place } });
    return;
  }
  if (!res.headers.has("corten-page") || shellChanged(res)) {
    location.reload();
    return;
  }
  // The server redirected (a `Caller` sending someone to sign in). Changing the address with
  // history.replaceState would abort this navigation and the body with it, so read the body,
  // then navigate again to where it came from, in place of this entry, with the page in hand.
  if (res.redirected && !redirected) {
    const page = new Response(await res.text(), { headers: res.headers });
    signal.throwIfAborted();
    navigation.navigate(res.url, { history: "replace", info: { [REDIRECTED]: page, [KEEP_PLACE]: place } });
    return;
  }
  const html = await res.text();
  signal.throwIfAborted();
  // Read again with nothing new (the write has not landed yet): the page stays as it is, with
  // what the user typed and its live streams.
  if (again && html === swapped) {
    readAgainLater();
    return;
  }
  await swapPage(html, signal, again || place !== undefined);
}

/** The last page `visit` swapped in, as the server sent it. */
let swapped = "";

/** Bumped by each page, so a read-again timer of an earlier one does nothing. */
let readAgainPage = 0;
const READ_AGAIN = Symbol("corten.readAgain");

/**
 * When this page's head asks for it (`<noscript data-corten-read-again="2">`, written by
 * `ReadAgain`), reads the page again after that many seconds, once the tab is visible:
 * the same URL, in place of this history entry, keeping scroll and focus. A navigation away (or
 * a page that no longer asks) stops it. A page that also asks for parts of it to be read again
 * ({@link readPartsAgain}) is not read whole: its head's tag is for a browser without JavaScript.
 */
function readAgainLater() {
  const page = ++readAgainPage;
  const tag = document.head.querySelector("noscript[data-corten-read-again]");
  const seconds = Number(tag?.getAttribute("data-corten-read-again"));
  if (!(seconds > 0) || document.querySelector(`main ${PART_AGAIN}`)) return;
  const url = location.href;
  readLater(
    seconds,
    () => page === readAgainPage && location.href === url,
    () => (navigation ? navigation.navigate(url, { history: "replace", info: { [READ_AGAIN]: true } }) : location.reload()),
  );
}

/**
 * Calls `read` after `seconds` while `wanted()`, once the tab is visible and no form's answer is
 * on its way (it will change the page: read after it).
 * @param {number} seconds @param {() => boolean} wanted @param {() => unknown} read
 */
function readLater(seconds, wanted, read) {
  const wait = () => {
    if (!wanted()) return;
    if (document.hidden) document.addEventListener("visibilitychange", wait, { once: true });
    else if (submits.size) setTimeout(wait, seconds * 1000);
    else read();
  };
  setTimeout(wait, seconds * 1000);
}

/** @type {any} */
const navigation = /** @type {any} */ (window).navigation;
/** Key of the response a redirected navigation hands to the next one, in its `info`. */
const REDIRECTED = Symbol("corten.redirected");
/** Key of the place a `data-keep-place` navigation hands to the one a redirect starts. */
const KEEP_PLACE = Symbol("corten.keepPlace");
/**
 * Key of a navigation the server sent the browser on (after a write): its page is fetched even
 * when only the `#id` differs from the address, since the write may have changed the page.
 */
const LOAD = Symbol("corten.load");

/**
 * Whether the first document has fired `load`. Before it, a navigation is the browser's own (a
 * full load): a script's form submit (one made from on:connect) makes Chromium stop loading the
 * document before `navigate` fires, so `readyState` reads "complete" but `load` never comes, and
 * a page taken over then would never load.
 */
let loaded = document.readyState === "complete";
if (!loaded) addEventListener("load", () => (loaded = true), { once: true });

/**
 * How, where and in what encoding `form` goes when `submitter` sends it, read as the browser
 * reads them: the button's `formmethod`, `formaction` and `formenctype` when it has them (an
 * invalid method is "get", an empty action the document's address), else the form's. Lowercase
 * method, absolute action. `target` is not read: a `data-patch` answer patches this page.
 * @param {HTMLFormElement} form
 * @param {Element | null | undefined} submitter
 */
function sendsAs(form, submitter) {
  const button = submitter instanceof HTMLButtonElement || submitter instanceof HTMLInputElement ? submitter : null;
  return {
    method: button?.hasAttribute("formmethod") ? button.formMethod : formProperty(form, "method"),
    action: button?.hasAttribute("formaction") ? button.formAction : formProperty(form, "action"),
    enctype: button?.hasAttribute("formenctype") ? button.formEnctype : formProperty(form, "enctype"),
  };
}

/**
 * Whether a navigation is a form sent by POST, which the browser sends itself. Chromium names
 * the submit button as the source but sets `formData` from the form's own method, so a POST
 * button in a GET form has none (#190); Firefox names the form there, and its `formData`
 * follows the button.
 * @param {any} event
 */
function posts(event) {
  const button = event.sourceElement;
  const form = button?.form;
  if (!(form instanceof HTMLFormElement) || !(button instanceof HTMLButtonElement || button instanceof HTMLInputElement)) return !!event.formData;
  return sendsAs(form, button).method === "post";
}

navigation?.addEventListener("navigate", (/** @type {any} */ event) => {
  // A script's history.pushState or replaceState (a tab that puts itself in the address) is the
  // page's own: nothing to fetch. Back and forward between pages this runtime swapped are
  // same-document traversals too, and do need the page.
  const skip =
    !event.canIntercept ||
    !loaded ||
    // A navigation the server sent, or the page reading itself again, is fetched even when only
    // the `#id` differs from the address, or nothing does.
    (!event.info?.[LOAD] && !event.info?.[READ_AGAIN] && (event.hashChange || (event.destination.sameDocument && event.navigationType !== "traverse"))) ||
    event.downloadRequest !== null ||
    posts(event) ||
    event.navigationType === "reload" ||
    event.sourceElement?.closest?.("[data-reload]");
  if (skip) return;
  const url = new URL(event.destination.url);
  const again = event.info?.[READ_AGAIN];
  // The page is about to change: replies to its pending submits would patch the wrong page.
  if (!again) for (const pending of submits.values()) pending.controller.abort();
  const redirected = event.info?.[REDIRECTED];
  // The same page read again, or a form or link marked `data-keep-place` (filters applied as they
  // change): the page changes under the user, who stays where they were reading, on the control
  // they used. Kept across a redirect the server answers with.
  const place = again || event.sourceElement?.closest?.("[data-keep-place]") ? placeOf(document.activeElement) : event.info?.[KEEP_PLACE];
  if (place === undefined) {
    // Back and forward: the browser puts the user back where they were on that entry.
    if (event.navigationType === "traverse") {
      event.intercept({ handler: () => visit(url, event.signal, redirected) });
      return;
    }
    // A new page starts at its `#id`, else at the top. Scrolled here, not left to the browser:
    // Chromium leaves a swapped page where the last one was scrolled, and Firefox does not scroll
    // to an `#id` the address already had.
    const handler = async () => {
      await visit(url, event.signal, redirected);
      if (event.signal.aborted) return;
      let id = url.hash.slice(1);
      try {
        id = decodeURIComponent(id);
      } catch {}
      const at = id ? document.getElementById(id) : null;
      if (at) at.scrollIntoView();
      else scrollTo(0, 0);
    };
    event.intercept({ handler, scroll: "manual" });
    return;
  }
  const handler = async () => {
    await visit(url, event.signal, redirected, again, place);
    // Back to the control, unless the user has moved on meanwhile.
    const now = document.activeElement;
    const el = /** @type {HTMLInputElement | undefined} */ (place && (!now || now === document.body) ? document.querySelectorAll(place.at)[place.nth] : undefined);
    if (!el) return;
    el.focus({ preventScroll: true });
    try {
      const [start, end, direction] = place.caret;
      if (start !== null) el.setSelectionRange(start, end, direction);
    } catch {}
  };
  event.intercept({ handler, scroll: "manual", focusReset: "manual" });
});

/**
 * The control that has focus, as a selector that finds it again in a swapped page (by id, else by
 * its name in <main>, and which of the controls with that name it is), and its caret. Null when
 * nothing is focused.
 * @param {Element | null} el
 * @returns {{ at: string, nth: number, caret: [number | null, number | null, any] } | null}
 */
function placeOf(el) {
  const name = el?.getAttribute("name");
  const at = el?.id ? `#${CSS.escape(el.id)}` : name && el ? `main ${named(el, name)}` : "";
  if (!at) return null;
  const f = /** @type {HTMLInputElement} */ (el);
  /** @type {[number | null, number | null, any]} */
  let caret = [null, null, null];
  try {
    caret = [f.selectionStart, f.selectionEnd, f.selectionDirection];
  } catch {}
  return { at, nth: Array.prototype.indexOf.call(document.querySelectorAll(at), el), caret };
}

/**
 * The selector of the controls named `name` like `el` (with its value attribute, for a checkbox
 * or radio: one without it has the value "on" but does not match `[value="on"]`).
 * @param {Element} el @param {string} name
 */
function named(el, name) {
  const value = el.getAttribute("value");
  const choice = /^(checkbox|radio)$/.test(el.getAttribute("type") ?? "") ? (value === null ? ":not([value])" : `[value="${CSS.escape(value)}"]`) : "";
  return `${el.localName}[name="${CSS.escape(name)}"]${choice}`;
}
/**
 * The pending submit of each `data-patch` form. Like a browser's own form navigation, the latest
 * submit wins: a newer one aborts the older request, and a navigation aborts them all, so a late
 * reply never patches a page it was not meant for. The same submit again while it is pending (a
 * double click) is ignored rather than sent twice.
 * @type {Map<HTMLFormElement, { key: string, controller: AbortController }>}
 */
const submits = new Map();

/**
 * A form's entries as the browser sends them in a body that is not multipart, or in a GET's
 * query: a file is its name, and every line break in a name or value is CRLF.
 * @param {FormData} data
 * @returns {[string, string][]}
 */
const sentEntries = (data) => {
  const crlf = (/** @type {string} */ text) => text.replace(/\r\n?|\n/g, "\r\n");
  return Array.from(data, ([name, value]) => [crlf(name), crlf(typeof value === "string" ? value : value.name)]);
};

// On the window, so it runs after every other handler of the submit (an `on:submit` listens on
// the document, and calls it itself when one stops the submit's propagation): one that cancels
// it stops the form, as it would without the runtime.
addEventListener("submit", patchSubmit);

/** Sends a `data-patch` form's submit as a patch, unless a handler cancelled it. @param {Event} event */
async function patchSubmit(event) {
  const form = event.target;
  if (event.defaultPrevented || !(form instanceof HTMLFormElement) || !formCall(form, "hasAttribute", "data-patch")) return;
  const submitter = /** @type {SubmitEvent} */ (event).submitter;
  const sends = sendsAs(form, submitter);
  // A dialog's form closes the dialog and sends nothing: the browser's to do.
  if (sends.method === "dialog") return;
  event.preventDefault();
  const data = new FormData(form, submitter);
  const method = sends.method.toUpperCase();
  const url = new URL(sends.action);
  const entries = sentEntries(data);
  const fields = new URLSearchParams(entries);
  const sent = sentState(form);
  // A file by its size and time too: another file of the same name is a new submit.
  const key = `${method} ${url.href} ${fields} ${Array.from(data.values(), (v) => (typeof v === "string" ? "" : `${v.size}/${v.lastModified}`))}`;
  const previous = submits.get(form);
  if (previous?.key === key) return;
  previous?.controller.abort();
  const controller = new AbortController();
  const { signal } = controller;
  submits.set(form, { key, controller });
  /** @type {RequestInit} */
  const init = { method, headers: { corten: "action" }, signal };
  if (method === "GET") url.search = fields.toString();
  else if (sends.enctype === "multipart/form-data") init.body = data;
  else {
    // The browser's content types, which name no charset (fetch adds one to a URLSearchParams).
    const plain = sends.enctype === "text/plain";
    init.body = plain ? entries.map(([name, value]) => `${name}=${value}\r\n`).join("") : fields.toString();
    init.headers = { corten: "action", "content-type": plain ? "text/plain" : "application/x-www-form-urlencoded" };
  }
  formCall(form, "setAttribute", "aria-busy", "true");
  // A write can change any page, so nothing fetched before it is reused.
  if (method !== "GET") {
    for (const entry of prefetched.values()) discard(entry);
    prefetched.clear();
  }
  try {
    const res = await fetch(url, init);
    // A whole answer below 500 is definitive: the write applied or was refused, and the form gets
    // a new key. A 5xx (a 502 from a proxy can hide a committed write), a network error, an abort
    // or a body cut off on the way keeps the key, so a retry is the same submission.
    // Only the submit still in flight for the form renews its key: a newer one of the same form
    // (which aborted this one) went with this key, and its answer is still to come.
    const renew = () => res.status < 500 && submits.get(form)?.controller === controller && renewSubmitKeys(form);
    const location_ = res.headers.get("corten-location");
    if (location_) {
      renew();
      submits.delete(form);
      if (navigation) navigation.navigate(location_, { info: { [LOAD]: true } });
      else {
        // Without the Navigation API, a URL that differs only in its `#id` would only scroll.
        const to = new URL(location_, location.href);
        const same = pageKey(to) === pageKey(new URL(location.href));
        location.assign(to);
        if (same) location.reload();
      }
    } else if (res.headers.has("corten-page") && shellChanged(res)) {
      renew();
      location.reload();
    } else if (res.headers.has("corten-patch") || res.headers.has("corten-page")) {
      const body = await res.text();
      renew();
      signal.throwIfAborted();
      await (res.headers.has("corten-patch") ? apply(body, signal, sent, res.ok) : swapPage(body, signal));
      signal.throwIfAborted();
      if (res.status === 422) focusInvalid(form);
    } else {
      // An answer the runtime does not draw is whole once its body has arrived too.
      await res.arrayBuffer();
      signal.throwIfAborted();
      renew();
      formCall(form, "dispatchEvent", new CustomEvent("corten:error", { bubbles: true, detail: { status: res.status } }));
    }
  } catch (error) {
    if (signal.aborted) return;
    // Offline or the server is unreachable: the form stays as it is, and the page can say so.
    formCall(form, "dispatchEvent", new CustomEvent("corten:error", { bubbles: true, detail: { status: 0, error } }));
  } finally {
    if (submits.get(form)?.controller === controller) {
      submits.delete(form);
      formCall(form, "removeAttribute", "aria-busy");
    }
  }
}

/**
 * Sets a `<corten-live>`'s `status` and says so with a bubbling `corten-status` event, which a
 * handler takes like any other (`on:corten-status={js::show(target)}`) and reads the attributes.
 * @param {Element} el @param {string} status
 */
function liveStatus(el, status) {
  el.setAttribute("status", status);
  el.dispatchEvent(new Event("corten-status", { bubbles: true }));
}

customElements.define(
  "corten-live",
  class extends HTMLElement {
    /** @type {EventSource | null} */
    source = null;
    /** Patches apply in the order they arrive, each after the modules it needs. */
    applied = Promise.resolve();
    connectedCallback() {
      const src = this.getAttribute("src");
      if (!src) throw new Error("corten: <corten-live> needs a src attribute");
      // `since` is the app's cursor the page was rendered at: the first connection sends it, so
      // the server only sends what changed after it. A reconnect sends the last fragment's id as
      // Last-Event-ID, which the server reads first.
      const since = this.getAttribute("since");
      const url = new URL(src, location.href);
      if (since) url.searchParams.set("since", since);
      // The connection shows on the element, for CSS and scripts: `status` is connecting, live,
      // reconnecting (the browser retries), closed (it gave up) or ended (the server finished), and
      // `stale-since` is when a live stream dropped, until it is live again. Each change of
      // `status` dispatches `corten-status`.
      const source = (this.source = new EventSource(url));
      liveStatus(this, "connecting");
      source.addEventListener("open", () => {
        this.removeAttribute("stale-since");
        liveStatus(this, "live");
      });
      source.addEventListener("error", () => {
        if (this.getAttribute("status") === "ended") return;
        if (this.getAttribute("status") === "live") this.setAttribute("stale-since", new Date().toISOString());
        liveStatus(this, source.readyState === EventSource.CLOSED ? "closed" : "reconnecting");
      });
      source.addEventListener("patch", (event) => {
        const html = /** @type {MessageEvent} */ (event).data;
        this.applied = this.applied.then(() => patch(html)).catch(reportError);
      });
      // The server's stream finished: stay as it is instead of reconnecting.
      source.addEventListener("end", () => {
        source.close();
        liveStatus(this, "ended");
      });
    }
    disconnectedCallback() {
      this.source?.close();
    }
  },
);

markCurrent();
scan(document.documentElement, new Set());
serve(document);
readAgainLater();
{
  const main = document.querySelector("main");
  if (main) readPartsAgain(main);
}

/** Resolves once the script modules the first page's handlers use are imported. */
export const ready = Promise.all(loading.values()).then(() => undefined);

// Under `corten dev` the server sends the time it started; after a restart (a rebuild) the stream
// reconnects to a server that started later, and the page reloads.
const devStart = document.querySelector('meta[name="corten-dev"]')?.getAttribute("content");
if (devStart) {
  new EventSource("/corten/dev").addEventListener("message", (event) => {
    if (/** @type {MessageEvent} */ (event).data !== devStart) location.reload();
  });
}

// App::client_zone: the browser's time zone, kept in a cookie for the server's ClientZone. Written
// on each full load, so a visitor who travels is followed from their next page.
if (document.querySelector('meta[name="corten-zone"]')) {
  document.cookie = `corten-zone=${Intl.DateTimeFormat().resolvedOptions().timeZone};path=/;max-age=31536000;samesite=lax`;
}

const worker = document.querySelector('meta[name="corten-sw"]')?.getAttribute("content");
if (worker && "serviceWorker" in navigator) {
  addEventListener("load", () => {
    navigator.serviceWorker.register(worker).catch((error) => console.error("corten: service worker registration failed", error));
  });
}
