// @ts-check
// Corten runtime. Served by the corten crate at /corten/{version}/corten.js, minified from this
// file into dist/corten.js by `npm run build:js`, and imported as "corten".
//
// What it does, and nothing else:
// 1. Same-origin navigations: fetch the next page with `corten: nav` and swap it into <main>.
// 2. <form data-patch>: submit with `corten: action` and apply the returned fragments by id. The
//    latest submit of a form wins; a navigation abandons pending ones.
// 3. <corten-live src>: apply fragments streamed as server-sent `patch` events.
// 4. Islands: an undefined <tag-name> loads `{islands dir}/tag-name.js` (the script of an #[island]
//    component) and defines its `island()`; `on:event={Action}` markup runs its actions.
// 5. subscribe(): live rows from a corten::feed (snapshot + ordered patches) for islands that keep state.
// 6. Hover prefetch of same-origin links, the page's head tags on navigation, `aria-current` on
//    shell links with `data-nav`, and the service worker when the shell names one (corten::pwa).
// 7. A page response whose shell key differs from this page's (the user signed in or out, or a
//    deploy changed the runtime or assets) is loaded in full instead of swapped.
// Without JavaScript, links and forms still work as plain HTML.

const islandsDir = document.querySelector('meta[name="corten-islands"]')?.getAttribute("content") ?? "/static/islands/";
/** @type {Map<string, Promise<void>>} */
const loading = new Map();

// Island wrappers are layout-neutral. The server prints this rule for the first page's islands;
// islands that arrive later (navigation, patches) get it here. A constructed stylesheet is CSSOM,
// so it works under a nonce-only CSP.
const islandStyles = new CSSStyleSheet();
document.adoptedStyleSheets = [...document.adoptedStyleSheets, islandStyles];

/** Every element whose class island() made. */
const islandClasses = new WeakSet();

/**
 * The behavior of an `#[island]` component, in the script next to it:
 * `island("offer-list", { state() {}, connect(ctx) {}, actions: { showAll(ctx) {} } })`.
 * `on:click={ShowAll}` in the markup runs `actions.showAll`; `ref={Rows}` is `ctx.refs.rows`.
 * `ctx.signal` aborts when the element leaves the page: pass it to `addEventListener`, `fetch`,
 * `subscribe` and `mount`, or listen for its `abort` event. It is the only cleanup hook.
 * Types (checked against the Rust markup) and docs: islands.d.ts.
 * @template {keyof CortenIslands} T
 * @template [S=undefined]
 * @param {T} tag
 * @param {CortenSpec<CortenIslands[T], S>} spec
 * @returns {{ new (): HTMLElement, tag: T }}
 */
export function island(tag, spec) {
  const { state, connect } = spec;
  const actions = /** @type {Record<string, (ctx: any) => void>} */ (/** @type {any} */ (spec).actions ?? {});
  const made = class extends HTMLElement {
    static tag = tag;
    /** @type {AbortController | null} */
    #life = null;
    /** @type {Set<string>} */
    #listening = new Set();
    /** @type {any} */
    #context = null;

    connectedCallback() {
      if (this.#life) return;
      const life = (this.#life = new AbortController());
      this.#listening = new Set();
      this.#context = {
        el: this,
        refs: refs(this, false),
        all: refs(this, true),
        props: readProps(this),
        state: this.#context ? this.#context.state : state?.(),
        signal: life.signal,
      };
      wire(this);
      const made = /** @type {unknown} */ (connect?.(this.#context));
      if (typeof made === "function") console.error(`corten: <${tag}> connect returned a function, which never runs; clean up on ctx.signal instead`);
    }

    disconnectedCallback() {
      // A move within the page disconnects and reconnects in one task; only tear down real removals.
      queueMicrotask(() => {
        if (this.isConnected || !this.#life) return;
        this.#life.abort();
        this.#life = null;
      });
    }

    /**
     * Starts delegating `type` events to actions (once per type).
     * @param {string} type
     */
    listen(type) {
      if (!this.#life || this.#listening.has(type)) return;
      this.#listening.add(type);
      // Bubbling events in the bubble phase, so actions run innermost first, like event
      // handlers; events that do not bubble (focus, mouseenter) reach the element only by capture.
      const options = { signal: this.#life.signal };
      this.addEventListener(type, (event) => event.bubbles && this.#run(event), options);
      this.addEventListener(type, (event) => !event.bubbles && this.#run(event), { ...options, capture: true });
    }

    /** @param {Event} event */
    #run(event) {
      const start = event.target instanceof Element ? event.target : event.target instanceof Node ? event.target.parentElement : null;
      for (let node = start; node; node = node.parentElement) {
        const on = node.getAttribute("data-on");
        if (on && owner(node) === this) {
          for (const pair of on.split(" ")) {
            const colon = pair.indexOf(":");
            if (pair.slice(0, colon) !== event.type) continue;
            const name = pair.slice(colon + 1);
            const action = actions[name];
            if (!action) {
              console.error(`corten: <${tag}> has no action ${name} (data-on="${on}")`);
              continue;
            }
            const ctx = Object.create(this.#context);
            ctx.target = node;
            ctx.event = event;
            action(ctx);
          }
        }
        if (node === this || event.cancelBubble) return;
      }
    }
  };
  islandClasses.add(made);
  return made;
}

/**
 * The island an element belongs to: the closest custom element around it. An island's markup
 * ends where another custom element starts, so nested islands keep their own refs and actions.
 * @param {Element} el
 * @returns {Element | null}
 */
function owner(el) {
  for (let node = el.parentElement; node; node = node.parentElement) {
    if (node.localName.includes("-")) return node;
  }
  return null;
}

/**
 * Makes each island listen for the events of the `data-on` elements in `root`: on connect, and
 * for markup a patch adds later.
 * @param {Element} root
 */
function wire(root) {
  const marked = Array.from(root.querySelectorAll("[data-on]"));
  if (root.hasAttribute("data-on")) marked.unshift(root);
  for (const el of marked) {
    const host = /** @type {any} */ (owner(el));
    if (!host || !islandClasses.has(host.constructor)) continue;
    for (const pair of /** @type {string} */ (el.getAttribute("data-on")).split(" ")) host.listen(pair.slice(0, pair.indexOf(":")));
  }
}

/**
 * `refs` (the first match, which must exist) or `all` (every match) of an island.
 * @param {Element} host
 * @param {boolean} many
 */
function refs(host, many) {
  return new Proxy(
    {},
    {
      get(_, name) {
        if (typeof name !== "string") return undefined;
        const mine = Array.from(host.querySelectorAll(`[data-ref="${name}"]`)).filter((el) => owner(el) === host);
        if (many) return mine;
        if (!mine[0]) throw new Error(`corten: <${host.localName}> has no element with ref={${name[0].toUpperCase()}${name.slice(1)}} (data-ref="${name}")`);
        return mine[0];
      },
    },
  );
}

/**
 * The `#[prop]` JSON the server printed as the island's first child, read when it connects (so a
 * script that replaces the children, as Solid does, still has it).
 * @param {Element} host
 */
function readProps(host) {
  const script = host.querySelector(":scope > script[data-props]");
  return script ? JSON.parse(script.textContent ?? "{}") : {};
}

/**
 * @template Row
 * @typedef {{ op: "upsert", id: string, value: Row } | { op: "remove", id: string }} FeedChange
 */

/**
 * @template Row
 * @typedef {object} FeedHandlers
 * @property {(rows: Row[], seq: number) => void} snapshot Every row; replaces what the island had.
 * @property {(changes: FeedChange<Row>[], seq: number, receivedAt: number) => void} patch One write, in order, never twice.
 * @property {(status: "syncing" | "live" | "reconnecting") => void} [status]
 */

/**
 * Live rows from a `corten::feed` at `url`: calls `snapshot` with every row, then `patch` with each
 * later write in order. Reconnects resume where they stopped; a gap, an unknown cursor or a client
 * that fell behind loads a new snapshot. It runs until `signal` aborts (in an island, pass
 * `ctx.signal`: the subscription ends when the element leaves the page), or, without one, for the
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

/**
 * Defines every undefined custom element under `root` from its island module.
 * @param {ParentNode} root
 */
export function upgrade(root) {
  const tags = new Set(Array.from(root.querySelectorAll(":not(:defined)"), (el) => el.localName));
  return Promise.allSettled(Array.from(tags, define)).then(() => undefined);
}

/** @param {string} tag */
function define(tag) {
  let pending = loading.get(tag);
  if (!pending) {
    islandStyles.insertRule(`${tag}{display:contents}`, islandStyles.cssRules.length);
    const url = `${islandsDir}${tag}.js`;
    pending = import(url).then(
      (module) => {
        const made = module.default;
        if (made?.tag !== tag) throw new Error(`${url} must end with export default island("${tag}", { ... })`);
        if (!customElements.get(tag)) customElements.define(tag, made);
      },
      (error) => console.error(`corten: <${tag}> has no island module at ${url}; list the #[island] in App::islands`, error),
    ).catch((error) => console.error(`corten: ${error.message}`));
    loading.set(tag, pending);
  }
  return pending;
}

/** @param {string} html */
function parse(html) {
  const template = document.createElement("template");
  template.innerHTML = html;
  return template.content;
}

/**
 * Replaces the element with the same id for every top-level element in `html`.
 * @param {string} html
 */
export function patch(html) {
  for (const el of Array.from(parse(html).children)) {
    const target = el.id ? document.getElementById(el.id) : null;
    if (target) {
      target.replaceWith(el);
      wire(el);
    } else console.error(`corten: patch element <${el.localName}> needs an id that exists on the page`, el);
  }
  return upgrade(document.body);
}

/**
 * Swaps a page response (`<title>`, the page's head tags, then the page) into <main>.
 * @param {string} html
 */
function swapPage(html) {
  const content = parse(html);
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
  return upgrade(main);
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

/** Navigation responses fetched ahead of a likely click, by URL without the hash. */
/** @type {Map<string, { at: number, response: Promise<Response> }>} */
const prefetched = new Map();
const PREFETCH_FRESH_MS = 10_000;

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
  const hit = prefetched.get(key);
  if (hit && performance.now() - hit.at < PREFETCH_FRESH_MS) return;
  const response = fetch(key, { headers: { corten: "nav" }, priority: "low" });
  response.catch(() => prefetched.delete(key));
  prefetched.set(key, { at: performance.now(), response });
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
  /** @type {Response} */
  let res;
  const main = document.querySelector("main");
  main?.setAttribute("aria-busy", "true");
  try {
    res =
      hit && performance.now() - hit.at < PREFETCH_FRESH_MS
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
  await swapPage(await res.text());
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
  if (method !== "GET") prefetched.clear();
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
      await (res.headers.has("corten-patch") ? patch(body) : swapPage(body));
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
    connectedCallback() {
      const src = this.getAttribute("src");
      if (!src) throw new Error("corten: <corten-live> needs a src attribute");
      this.source = new EventSource(src);
      this.source.addEventListener("patch", (event) => patch(/** @type {MessageEvent} */ (event).data));
    }
    disconnectedCallback() {
      this.source?.close();
    }
  },
);

markCurrent();

/** Resolves once the islands on the first page are defined. */
export const ready = upgrade(document.body);

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
