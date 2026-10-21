// @ts-check
// Corten runtime. Served by the corten crate at /corten/corten.js and imported as "corten".
//
// What it does, and nothing else:
// 1. Same-origin navigations: fetch the next page with `corten: nav` and swap it into <main>.
// 2. <form data-patch>: submit with `corten: action` and apply the returned fragments by id.
// 3. <corten-live src>: apply fragments streamed as server-sent `patch` events.
// 4. Islands: an undefined <tag-name> loads `{islands dir}/tag-name.js` and defines its default export.
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

/**
 * Base class for islands. Write `setup()`; it runs when the element connects. When the element
 * leaves the page (a navigation swapped it out), everything registered with `on`, `subscribe`
 * and `cleanup` is torn down, and `setup()` runs again if it comes back.
 */
export class Island extends HTMLElement {
  #ready = false;
  /** @type {AbortController | null} */
  #life = null;
  /** @type {unknown} */
  #props = undefined;

  connectedCallback() {
    if (this.#ready) return;
    this.#ready = true;
    this.#life = new AbortController();
    this.setup();
  }

  disconnectedCallback() {
    // A move within the page disconnects and reconnects in one task; only tear down real removals.
    queueMicrotask(() => {
      if (this.isConnected || !this.#ready) return;
      this.#ready = false;
      this.#life?.abort();
      this.#life = null;
    });
  }

  setup() {}

  /** Aborted when the island leaves the page. */
  get signal() {
    if (!this.#life) throw new Error(`<${this.localName}> is not connected`);
    return this.#life.signal;
  }

  /**
   * Runs `fn` when the island leaves the page.
   * @param {() => void} fn
   */
  cleanup(fn) {
    this.signal.addEventListener("abort", fn, { once: true });
  }

  /**
   * The first descendant with `data-ref="name"`. Throws when it is missing, so a template
   * change that breaks an island fails loudly.
   * @template {Element} [T=HTMLElement]
   * @param {string} name
   * @returns {T}
   */
  ref(name) {
    const el = this.querySelector(`[data-ref="${name}"]`);
    if (!el) throw new Error(`<${this.localName}> has no element with data-ref="${name}"`);
    return /** @type {T} */ (/** @type {unknown} */ (el));
  }

  /**
   * The JSON the server printed in this island with `corten::Props` (a child
   * `<script type="application/json" data-props>`). Read once, then cached. Throws when missing.
   * Give the result a JSDoc `@type` where it is read, so the island is type-checked against it.
   * @returns {any}
   */
  props() {
    if (this.#props === undefined) {
      const script = this.querySelector(":scope > script[data-props]");
      if (!script) throw new Error(`<${this.localName}> has no corten::Props script`);
      this.#props = JSON.parse(script.textContent ?? "null");
    }
    return this.#props;
  }

  /**
   * Listens for `type` on descendants with `data-ref="name"`, including ones added later.
   * @param {string} type
   * @param {string} name
   * @param {(target: HTMLElement, event: Event) => void} handler
   */
  on(type, name, handler) {
    this.addEventListener(
      type,
      (event) => {
        const target = /** @type {Element} */ (event.target).closest(`[data-ref="${name}"]`);
        if (target instanceof HTMLElement && this.contains(target)) handler(target, event);
      },
      { signal: this.signal },
    );
  }

  /**
   * `subscribe()` for as long as the island is on the page.
   * @template Row
   * @param {string} url
   * @param {FeedHandlers<Row>} handlers
   */
  subscribe(url, handlers) {
    this.cleanup(subscribe(url, handlers));
  }
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
 * that fell behind loads a new snapshot. Returns a function that stops the subscription.
 * @template Row
 * @param {string} url
 * @param {FeedHandlers<Row>} handlers
 * @returns {() => void}
 */
export function subscribe(url, handlers) {
  /** @type {EventSource | null} */
  let source = null;
  let seq = 0;
  let stopped = false;
  let failures = 0;
  /** @type {AbortController | null} */
  let request = null;
  const status = handlers.status ?? (() => {});

  async function sync() {
    source?.close();
    source = null;
    request?.abort();
    if (stopped) return;
    status("syncing");
    request = new AbortController();
    try {
      const res = await fetch(url, { headers: { accept: "application/json" }, signal: request.signal });
      if (!res.ok) throw new Error(`corten: feed ${url} answered ${res.status}`);
      /** @type {{ epoch: string, seq: number, rows: Row[] }} */
      const snap = await res.json();
      if (stopped) return;
      failures = 0;
      seq = snap.seq;
      handlers.snapshot(snap.rows, snap.seq);
      listen(`${url}/live?since=${encodeURIComponent(`${snap.epoch}-${snap.seq}`)}`);
    } catch (error) {
      if (stopped || (error instanceof DOMException && error.name === "AbortError")) return;
      console.error(error);
      failures += 1;
      setTimeout(sync, Math.min(30_000, 500 * 2 ** failures));
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
      if (es.readyState !== EventSource.CLOSED || source !== es || stopped) return;
      failures += 1;
      setTimeout(() => {
        if (source === es && !stopped) listen(`${url}/live?since=${encodeURIComponent(cursor)}`);
      }, Math.min(30_000, 500 * 2 ** failures));
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
  return () => {
    stopped = true;
    request?.abort();
    source?.close();
  };
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
        if (!customElements.get(tag)) customElements.define(tag, module.default);
      },
      (error) => console.error(`corten: <${tag}> needs an island module at ${url}`, error),
    );
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
    if (target) target.replaceWith(el);
    else console.error(`corten: patch element <${el.localName}> needs an id that exists on the page`, el);
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
  const skip =
    !event.canIntercept ||
    event.hashChange ||
    event.downloadRequest !== null ||
    event.formData ||
    event.navigationType === "reload" ||
    event.sourceElement?.closest?.("[data-reload]");
  if (skip) return;
  event.intercept({ handler: () => visit(new URL(event.destination.url), event.signal) });
});

document.addEventListener("submit", async (event) => {
  const form = event.target;
  if (!(form instanceof HTMLFormElement) || !form.hasAttribute("data-patch")) return;
  event.preventDefault();
  if (form.getAttribute("aria-busy") === "true") return;
  const submitter = /** @type {SubmitEvent} */ (event).submitter;
  const data = new FormData(form, submitter);
  const method = (submitter?.getAttribute("formmethod") || form.method).toUpperCase();
  const url = new URL(submitter?.getAttribute("formaction") || form.action, location.href);
  /** @type {RequestInit} */
  const init = { method, headers: { corten: "action" } };
  const fields = new URLSearchParams(/** @type {any} */ (data));
  if (method === "GET") url.search = fields.toString();
  else init.body = form.enctype === "multipart/form-data" ? data : fields;
  form.setAttribute("aria-busy", "true");
  // A write can change any page, so nothing fetched before it is reused.
  if (method !== "GET") prefetched.clear();
  try {
    const res = await fetch(url, init);
    const location_ = res.headers.get("corten-location");
    if (location_) {
      if (navigation) navigation.navigate(location_);
      else location.assign(location_);
    } else if (res.headers.has("corten-patch")) {
      await patch(await res.text());
      if (res.status === 422) focusInvalid();
    } else if (res.headers.has("corten-page") && shellChanged(res)) {
      location.reload();
    } else if (res.headers.has("corten-page")) {
      await swapPage(await res.text());
      if (res.status === 422) focusInvalid();
    } else {
      form.dispatchEvent(new CustomEvent("corten:error", { bubbles: true, detail: { status: res.status } }));
    }
  } finally {
    form.removeAttribute("aria-busy");
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

const worker = document.querySelector('meta[name="corten-sw"]')?.getAttribute("content");
if (worker && "serviceWorker" in navigator) {
  addEventListener("load", () => {
    navigator.serviceWorker.register(worker).catch((error) => console.error("corten: service worker registration failed", error));
  });
}
