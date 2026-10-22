// @ts-check
// Corten runtime. Served by the corten crate at /corten/{version}/corten.js, minified from this
// file into dist/corten.js by `npm run build:js`, and imported as "corten".
//
// What it does, and nothing else:
// 1. Same-origin navigations: fetch the next page with `corten: nav` and swap it into <main>.
// 2. <form data-patch>: submit with `corten: action` and apply the returned fragments by id (a
//    `<template id data-remove>` removes its element). The latest submit of a form wins; a
//    navigation abandons pending ones.
// 3. <corten-live src>: apply fragments streamed as server-sent `patch` events.
// 4. Handlers: `data-on="click:cart_line.confirm($e,@dialog)"` (what `on:click={js::confirm(event, dialog)}`
//    prints) runs the export `confirm` of `{scripts dir}/cart_line.js` with the event and the
//    element with `data-ref="dialog"` in the same component. One listener per event type on the
//    document; modules are imported when markup that names them joins the page, and markup the
//    runtime inserts (a navigation, a patch) waits for its modules, so handlers always run while
//    the event is dispatched. `on:connect` runs when the element joins, and its `signal` aborts
//    when it leaves.
// 5. subscribe(): live rows from a corten::feed (snapshot + ordered patches) for scripts that keep state.
// 6. Hover prefetch of same-origin links, the page's head tags on navigation, `aria-current` on
//    shell links with `data-nav`, and the service worker when the shell names one (corten::pwa).
// 7. A page response whose shell key differs from this page's (the user signed in or out, or a
//    deploy changed the runtime or assets) is loaded in full instead of swapped.
// Without JavaScript, links and forms still work as plain HTML.

const scriptsDir = document.querySelector('meta[name="corten-scripts"]')?.getAttribute("content") ?? "/assets/scripts/";

/**
 * One handler call of a `data-on` attribute: `click:cart_line.confirm($e,@dialog,"Lamp")`.
 * @typedef {{ type: string, module: string, name: string, args: Arg[] }} Binding
 * @typedef {{ kind: "@" | "*" | "#" | "^", owner: string | null, name: string } | { kind: "$e" | "$t" | "$s" | "$c", owner: string | null } | { kind: "v", value: unknown } | { kind: "{", fields: [string, Arg][] }} Arg
 */

/** Script modules by name, once imported. @type {Map<string, Record<string, unknown>>} */
const modules = new Map();
/** @type {Map<string, Promise<unknown>>} */
const loading = new Map();

/**
 * Imports the script module `name` (`{scripts dir}/name.js`) once. The first page's modules are
 * preloaded and imported as soon as the runtime starts; markup the runtime inserts later waits
 * for its modules, so a handler always runs while its event is dispatched (and can call
 * `event.preventDefault()`). An event that arrives before its module (a click while the first
 * page still loads) gets the browser's default, as it would without JavaScript.
 * @param {string} name
 */
function load(name) {
  let pending = loading.get(name);
  if (!pending) {
    const url = `${scriptsDir}${name}.js`;
    pending = import(url).then(
      (module) => void modules.set(name, module),
      (error) => console.error(`corten: no script module at ${url}; list its component in App::components`, error),
    );
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
    parsed.set(el, entry);
  }
  return entry.list;
}

/**
 * Parses `type:module.name(arg,arg) type:module.name()`. Arguments are refs (`@rows`, every one:
 * `*rows`, every one by its `data-key`: `#rows`, the one around the event's target: `^rows`,
 * another component's: `@offer-list/rows`), the browser's values (`$e` event, `$t` the element
 * with the attribute, `$s` its signal, `$c` the component's root, `$c/offer-list`), a group of
 * arguments (`{rows:*row,all:@all}`), or JSON. A module name may have dots (`ui.combo_box`).
 * @param {string} source
 * @returns {Binding[]}
 */
function parseOn(source) {
  /** @type {Binding[]} */
  const list = [];
  const n = source.length;
  let i = 0;
  while (i < n) {
    if (source[i] === " ") {
      i += 1;
      continue;
    }
    const colon = source.indexOf(":", i);
    const open = source.indexOf("(", colon);
    const dot = source.lastIndexOf(".", open);
    if (colon < 0 || open < 0 || dot < colon) throw new Error(`corten: data-on="${source}" is not type:module.handler(..)`);
    /** @type {Arg[]} */
    const args = [];
    const close = split(source, open + 1, ")", (part) => args.push(parseArg(part)));
    list.push({ type: source.slice(i, colon), module: source.slice(colon + 1, dot), name: source.slice(dot + 1, open), args });
    i = close + 1;
  }
  return list;
}

/**
 * Calls `each` with every comma-separated part of `source` from `start` to the `end` character at
 * depth 0 (strings, arrays and objects nest), and returns where that character is.
 * @param {string} source @param {number} start @param {string} end @param {(part: string) => void} each
 */
function split(source, start, end, each) {
  let depth = 0;
  let j = start;
  for (; j < source.length; j += 1) {
    const c = source[j];
    if (c === '"') {
      for (j += 1; j < source.length && source[j] !== '"'; j += 1) if (source[j] === "\\") j += 1;
    } else if (c === "[" || c === "{") depth += 1;
    else if (depth > 0 && (c === "]" || c === "}")) depth -= 1;
    else if (depth === 0 && (c === "," || c === end)) {
      if (j > start) each(source.slice(start, j));
      start = j + 1;
      if (c === end) break;
    }
  }
  return j;
}

/** @param {string} text @returns {Arg} */
function parseArg(text) {
  const c = text[0];
  if (c === "@" || c === "*" || c === "#" || c === "^") {
    const slash = text.indexOf("/");
    return slash < 0 ? { kind: c, owner: null, name: text.slice(1) } : { kind: c, owner: text.slice(1, slash), name: text.slice(slash + 1) };
  }
  if (c === "$") {
    const kind = /** @type {"$e" | "$t" | "$s" | "$c"} */ (text.slice(0, 2));
    return { kind, owner: text[2] === "/" ? text.slice(3) : null };
  }
  // A group's keys are bare names; a JSON object's are quoted.
  if (c === "{" && text[1] !== '"' && text[1] !== "}") {
    /** @type {[string, Arg][]} */
    const fields = [];
    split(text, 1, "}", (part) => {
      const colon = part.indexOf(":");
      fields.push([part.slice(0, colon), parseArg(part.slice(colon + 1))]);
    });
    return { kind: "{", fields };
  }
  return { kind: "v", value: JSON.parse(text) };
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
  if (!cached || cached.generation !== generation) refCache.set(scope, (cached = { generation, refs: new Map() }));
  const hit = cached.refs.get(name);
  if (hit) return hit.slice();
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
  cached.refs.set(name, found);
  return found.slice();
}

/** Bumped when elements join or leave the page, which can change what a ref resolves to. */
let generation = 0;
/** @type {WeakMap<Element, { generation: number, refs: Map<string, Element[]> }>} */
const refCache = new WeakMap();

/** @type {WeakMap<Element, unknown>} */
const states = new WeakMap();

/**
 * The state a script keeps for `el` (usually a component's root: pass `scope`), made by `init`
 * the first time and kept while the element exists. Handlers are plain functions, so state that
 * lasts between events lives here (or in the DOM) rather than in the module, which every
 * instance of the component shares.
 * @template T
 * @param {Element} el
 * @param {() => T} init
 * @returns {T}
 */
export function state(el, init) {
  if (states.has(el)) return /** @type {T} */ (states.get(el));
  const made = init();
  states.set(el, made);
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
 * The value of one argument.
 * @param {Arg} arg @param {Element} el @param {Event} event @param {Binding} binding
 * @returns {unknown}
 */
function resolve(arg, el, event, binding) {
  switch (arg.kind) {
    case "v":
      return arg.value;
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
  if (!module) {
    if (event.type === "connect") load(binding.module).then(() => modules.has(binding.module) && el.isConnected && run(el, binding, event));
    return;
  }
  const handler = module[binding.name];
  if (typeof handler !== "function") {
    console.error(`corten: ${binding.module}.js exports no function ${binding.name}`, el);
    return;
  }
  try {
    const args = [];
    for (const arg of binding.args) {
      const value = resolve(arg, el, event, binding);
      if (value === SKIP) return;
      args.push(value);
    }
    const result = handler(...args);
    if (result instanceof Promise) result.catch(reportError);
  } catch (error) {
    reportError(error);
  }
}

/** Event types with a listener on the document. */
const listening = new Set();

/**
 * Listens for `type` on the document, once: an event that bubbles runs the handlers of every
 * element on its path, innermost first, until one stops it; one that does not (`focus`,
 * `mouseenter`) reaches the document only by capture and runs its target's handlers.
 * @param {string} type
 */
function listen(type) {
  if (type === "connect" || listening.has(type)) return;
  listening.add(type);
  document.addEventListener(type, (event) => {
    if (!event.bubbles) return;
    for (const node of event.composedPath()) {
      if (!(node instanceof Element)) continue;
      if (node.hasAttribute("data-on")) for (const b of bindings(node)) if (b.type === type) run(node, b, event);
      if (event.cancelBubble) return;
    }
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
 * and runs `on:connect` handlers.
 * @param {Element} root
 */
function scan(root) {
  for (const template of Array.from(root.querySelectorAll("template"))) modulesOf(template.content);
  const marked = Array.from(root.querySelectorAll("[data-on]"));
  if (root.hasAttribute("data-on")) marked.unshift(root);
  for (const el of marked) {
    let connects = false;
    for (const b of bindings(el)) {
      load(b.module);
      if (b.type === "connect") connects = true;
      else listen(b.type);
    }
    if (connects && !connected.has(el)) {
      connected.add(el);
      const event = new Event("connect");
      for (const b of bindings(el)) if (b.type === "connect") run(el, b, event);
    }
  }
}

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
}

/**
 * Markup that joins the page (a navigation, a patch, a script's insert, Solid's render) is wired;
 * markup that leaves it ends.
 * @param {MutationRecord[]} records
 */
function observed(records) {
  let removed = false;
  let changed = false;
  for (const record of records) {
    for (const node of Array.from(record.removedNodes)) {
      if (node instanceof Element) changed = removed = true;
    }
    for (const node of Array.from(record.addedNodes)) {
      if (!(node instanceof Element)) continue;
      changed = true;
      if (node.isConnected) scan(node);
    }
  }
  // Text changes (a live cell's new value) cannot change what a ref is.
  if (changed) generation += 1;
  if (removed && (lives.size || connected.size)) sweep();
}

const observer = new MutationObserver(observed);
observer.observe(document.documentElement, { childList: true, subtree: true });

/**
 * @template Row
 * @typedef {{ op: "upsert", id: string, value: Row } | { op: "remove", id: string }} FeedChange
 */

/**
 * @template Row
 * @typedef {object} FeedHandlers
 * @property {(rows: Row[], seq: number) => void} snapshot Every row; replaces what the script had.
 * @property {(changes: FeedChange<Row>[], seq: number, receivedAt: number) => void} patch One write, in order, never twice.
 * @property {(status: "syncing" | "live" | "reconnecting") => void} [status]
 */

/**
 * Live rows from a `corten::feed` at `url`: calls `snapshot` with every row, then `patch` with each
 * later write in order. Reconnects resume where they stopped; a gap, an unknown cursor or a client
 * that fell behind loads a new snapshot. It runs until `signal` aborts (in a handler, pass
 * `signal`: the subscription ends when the element leaves the page), or, without one, for the
 * life of the page. `Row` is the feed's row type from the generated declarations; without one,
 * rows are `unknown`.
 * @template [Row=unknown]
 * @param {string} url
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
      handlers.snapshot(snap.rows, snap.seq);
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

/** @param {string} html */
function parse(html) {
  const template = document.createElement("template");
  template.innerHTML = html;
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
  if (node instanceof Element && node.hasAttribute("data-on")) for (const b of bindings(node)) pending.push(load(b.module));
  for (const el of Array.from(node.querySelectorAll("[data-on]"))) for (const b of bindings(el)) pending.push(load(b.module));
  for (const template of Array.from(node.querySelectorAll("template"))) pending.push(modulesOf(template.content));
  return Promise.all(pending);
}

/**
 * Applies a patch: every top-level element in `html` replaces the element with its id, and a
 * `<template id="row-7" data-remove>` removes it. The modules the new markup's handlers name are
 * imported first, so its handlers run from the first event.
 * @param {string} html
 * @param {AbortSignal} [signal] Nothing changes once it aborts.
 */
export async function patch(html, signal) {
  const content = parse(html);
  await modulesOf(content);
  if (signal?.aborted) return;
  for (const el of Array.from(content.children)) {
    const target = el.id ? document.getElementById(el.id) : null;
    if (el instanceof HTMLTemplateElement && el.hasAttribute("data-remove")) target?.remove();
    else if (target) target.replaceWith(el);
    else console.error(`corten: patch element <${el.localName}> needs an id that exists on the page`, el);
  }
}

/**
 * Swaps a page response (`<title>`, the page's head tags, then the page) into <main>, once the
 * modules its handlers name are imported.
 * @param {string} html
 * @param {AbortSignal} [signal]
 */
async function swapPage(html, signal) {
  const content = parse(html);
  await modulesOf(content);
  signal?.throwIfAborted();
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
  main.replaceChildren(content);
  markCurrent();
}

/**
 * Sets `aria-current="page"` on `<a data-nav>` links to this page (`data-nav="section"`: to this
 * page or any page under it) and removes it from the rest. The shell is not re-rendered by
 * client navigation, so its menu would otherwise keep pointing at the first page.
 */
function markCurrent() {
  const here = location.pathname;
  for (const link of Array.from(document.querySelectorAll("a[href][data-nav]"))) {
    const path = new URL(/** @type {HTMLAnchorElement} */ (link).href).pathname;
    const base = path.replace(/\/+$/, "");
    const current = here === path || (link.getAttribute("data-nav") === "section" && here.startsWith(`${base}/`));
    if (current) link.setAttribute("aria-current", "page");
    else link.removeAttribute("aria-current");
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

/** Moves focus to the first field the server marked invalid. */
function focusInvalid() {
  const field = document.querySelector('main [aria-invalid="true"]');
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
 */
async function visit(url, signal) {
  const key = pageKey(url);
  const hit = prefetched.get(key);
  prefetched.delete(key);
  const fresh = hit && performance.now() - hit.at < PREFETCH_FRESH_MS;
  if (!fresh) discard(hit);
  /** @type {Response} */
  let res;
  const main = document.querySelector("main");
  main?.setAttribute("aria-busy", "true");
  try {
    res =
      hit && fresh
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
  if (!res.headers.has("corten-page") || shellChanged(res)) {
    location.reload();
    return;
  }
  if (res.redirected) history.replaceState(history.state, "", res.url);
  await swapPage(await res.text(), signal);
}

/** @type {any} */
const navigation = /** @type {any} */ (window).navigation;

navigation?.addEventListener("navigate", (/** @type {any} */ event) => {
  // A script's history.pushState or replaceState (a tab that puts itself in the address) is the
  // page's own: nothing to fetch. Back and forward between pages this runtime swapped are
  // same-document traversals too, and do need the page.
  const skip =
    !event.canIntercept ||
    event.hashChange ||
    (event.destination.sameDocument && event.navigationType !== "traverse") ||
    event.downloadRequest !== null ||
    event.formData ||
    event.navigationType === "reload" ||
    event.sourceElement?.closest?.("[data-reload]");
  if (skip) return;
  // The page is about to change: replies to its pending submits would patch the wrong page.
  for (const pending of submits.values()) pending.controller.abort();
  event.intercept({ handler: () => visit(new URL(event.destination.url), event.signal) });
});

/**
 * The pending submit of each `data-patch` form. Like a browser's own form navigation, the latest
 * submit wins: a newer one aborts the older request, and a navigation aborts them all, so a late
 * reply never patches a page it was not meant for. The same submit again while it is pending (a
 * double click) is ignored rather than sent twice.
 * @type {Map<HTMLFormElement, { key: string, controller: AbortController }>}
 */
const submits = new Map();

document.addEventListener("submit", async (event) => {
  const form = event.target;
  if (!(form instanceof HTMLFormElement) || !form.hasAttribute("data-patch")) return;
  event.preventDefault();
  const submitter = /** @type {SubmitEvent} */ (event).submitter;
  const data = new FormData(form, submitter);
  const method = (submitter?.getAttribute("formmethod") || form.method).toUpperCase();
  const url = new URL(submitter?.getAttribute("formaction") || form.action, location.href);
  const fields = new URLSearchParams(/** @type {any} */ (data));
  const key = `${method} ${url.href} ${fields}`;
  const previous = submits.get(form);
  if (previous?.key === key) return;
  previous?.controller.abort();
  const controller = new AbortController();
  const { signal } = controller;
  submits.set(form, { key, controller });
  /** @type {RequestInit} */
  const init = { method, headers: { corten: "action" }, signal };
  if (method === "GET") url.search = fields.toString();
  else init.body = form.enctype === "multipart/form-data" ? data : fields;
  form.setAttribute("aria-busy", "true");
  // A write can change any page, so nothing fetched before it is reused.
  if (method !== "GET") {
    for (const entry of prefetched.values()) discard(entry);
    prefetched.clear();
  }
  try {
    const res = await fetch(url, init);
    const location_ = res.headers.get("corten-location");
    if (location_) {
      submits.delete(form);
      if (navigation) navigation.navigate(location_);
      else location.assign(location_);
    } else if (res.headers.has("corten-page") && shellChanged(res)) {
      location.reload();
    } else if (res.headers.has("corten-patch") || res.headers.has("corten-page")) {
      const body = await res.text();
      signal.throwIfAborted();
      await (res.headers.has("corten-patch") ? patch(body, signal) : swapPage(body, signal));
      signal.throwIfAborted();
      if (res.status === 422) focusInvalid();
    } else {
      form.dispatchEvent(new CustomEvent("corten:error", { bubbles: true, detail: { status: res.status } }));
    }
  } catch (error) {
    if (signal.aborted) return;
    // Offline or the server is unreachable: the form stays as it is, and the page can say so.
    form.dispatchEvent(new CustomEvent("corten:error", { bubbles: true, detail: { status: 0, error } }));
  } finally {
    if (submits.get(form)?.controller === controller) {
      submits.delete(form);
      form.removeAttribute("aria-busy");
    }
  }
});

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
      this.source = new EventSource(src);
      this.source.addEventListener("patch", (event) => {
        const html = /** @type {MessageEvent} */ (event).data;
        this.applied = this.applied.then(() => patch(html)).catch(reportError);
      });
    }
    disconnectedCallback() {
      this.source?.close();
    }
  },
);

markCurrent();
scan(document.documentElement);

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

const worker = document.querySelector('meta[name="corten-sw"]')?.getAttribute("content");
if (worker && "serviceWorker" in navigator) {
  addEventListener("load", () => {
    navigator.serviceWorker.register(worker).catch((error) => console.error("corten: service worker registration failed", error));
  });
}
