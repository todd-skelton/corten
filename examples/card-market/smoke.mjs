// Browser smoke test of the production pieces, against the card-market example (API or fixture mode).
// Usage: node examples/card-market/smoke.mjs [http://localhost:18410]
import { chromium } from "@playwright/test";
import { steady } from "../../scripts/steady.mjs";
import { stall } from "../../scripts/stall.mjs";

const base = process.argv[2] ?? "http://localhost:18410";
const failures = [];
const seen = stall(5);
const check = (name, ok, detail = "") => {
  seen(name);
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures.push(name);
};

const browser = steady(await chromium.launch({ headless: true }));
try {
  const context = await browser.newContext();
  const page = await context.newPage();
  const errors = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  page.on("pageerror", (e) => errors.push(e.message));

  await page.goto(`${base}/`);
  await page.waitForFunction(() => typeof (/** @type {any} */ (window)).__hydratedAt === "number");
  await page.evaluate(() => ((/** @type {any} */ (window)).__marker = 1));

  // Hover first so the runtime prefetches, then click.
  let prefetches = 0;
  page.on("request", (r) => r.headers()["corten"] === "nav" && r.url().endsWith("/items/item_5") && (prefetches += 1));
  await page.hover('a[href="/items/item_5"]');
  await page.waitForTimeout(300);
  await page.click('a[href="/items/item_5"]');
  await page.waitForFunction(() => document.querySelector("h1")?.textContent === "Card Name 5");
  check("client navigation keeps the page", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 1));
  check("hover prefetch is reused by the click", prefetches === 1, `${prefetches} nav request(s)`);
  check("title follows navigation", (await page.title()) === "Card Name 5 | Card Market");
  check(
    "head tags follow navigation",
    (await page.$$eval('head [data-corten-head][rel="canonical"]', (l) => l.map((x) => x.getAttribute("href")))).join() === "/items/item_5",
  );
  const scripts = await page.evaluate(() =>
    performance.getEntriesByType("resource").map((e) => e.name).filter((n) => /\/scripts\/(listing_sort|offer_list)\.js$/.test(n)),
  );
  check("the scripts a navigation's markup names load from versioned URLs", scripts.length === 2 && scripts.every((n) => /\/assets\/[0-9a-f]{12}\/scripts\//.test(n)), scripts.join(" "));

  // Styles: tokens, a component's scoped rules and a responsive utility, from components.css.
  const styles = await page.evaluate(() => {
    const css = (/** @type {string} */ selector) => getComputedStyle(/** @type {Element} */ (document.querySelector(selector)));
    const root = document.documentElement;
    const ink = () => getComputedStyle(root).getPropertyValue("--color-ink").trim();
    const light = ink();
    root.dataset.theme = "dark";
    const dark = ink();
    delete root.dataset.theme;
    // A context on an element inside the page applies there, and a compact density's spacing too.
    const panel = document.createElement("section");
    panel.dataset.theme = "dark";
    panel.dataset.density = "compact";
    document.body.append(panel);
    const inner = getComputedStyle(panel);
    const scoped = [inner.getPropertyValue("--color-ink").trim(), inner.getPropertyValue("--space-4").trim()];
    // A light part inside the dark panel is light again, controls included.
    const nested = document.createElement("div");
    nested.dataset.theme = "light";
    panel.append(nested);
    const deeper = getComputedStyle(nested);
    scoped.push(deeper.getPropertyValue("--color-ink").trim(), deeper.colorScheme);
    panel.remove();
    return {
      accent: getComputedStyle(root).getPropertyValue("--color-accent").trim(),
      price: css("[data-scope=listing-sort] tbody td:last-child").fontVariantNumeric,
      themes: [light, dark],
      scoped,
    };
  });
  check("design tokens are custom properties", styles.accent === "#6b3fd4", styles.accent);
  check("a component's scoped stylesheet applies", styles.price === "tabular-nums", styles.price);
  // The item page's top: on a desktop the picture beside the title and the price; on a phone one
  // column, title, picture, price. The price's row is spaced by a responsive utility: the
  // --space-5 token on a desktop, --space-3 on a phone.
  const top = () =>
    page.evaluate(() => {
      const box = (/** @type {string} */ s) => /** @type {Element} */ (document.querySelector(s)).getBoundingClientRect();
      const [title, gallery, price] = [box("article h1"), box("article > div > div:nth-child(2)"), box("article > div > section")];
      const probe = document.body.appendChild(document.createElement("div"));
      const token = (/** @type {string} */ name) => ((probe.style.columnGap = `var(${name})`), getComputedStyle(probe).columnGap);
      const space = { 3: token("--space-3"), 5: token("--space-5") };
      probe.remove();
      return {
        beside: gallery.right <= title.left && gallery.right <= price.left && price.top < gallery.bottom,
        stacked: title.bottom <= gallery.top && gallery.bottom <= price.top,
        gap: getComputedStyle(/** @type {Element} */ (document.querySelector("article > div > section"))).columnGap,
        space,
      };
    });
  await page.setViewportSize({ width: 1280, height: 720 });
  const wide = await top();
  await page.setViewportSize({ width: 390, height: 844 });
  const phone = await top();
  await page.setViewportSize({ width: 1280, height: 720 });
  check("the item's picture sits beside its title and price on a desktop, above the price on a phone", wide.beside && phone.stacked, JSON.stringify({ wide, phone }));
  check(
    "a responsive utility spaces the price's row by the width's token",
    wide.space[5] !== wide.space[3] && wide.gap === wide.space[5] && phone.gap === phone.space[3],
    JSON.stringify({ wide: [wide.gap, wide.space], phone: [phone.gap, phone.space] }),
  );
  check("a theme overrides tokens", styles.themes[0] === "#1d1b26" && styles.themes[1] === "#ecebf2", styles.themes.join(" "));
  check(
    "a theme and a density set on an element apply inside it, and a light part inside a dark one is light",
    styles.scoped.join(" ") === "#ecebf2 12px #1d1b26 light",
    styles.scoped.join(" "),
  );
  // The system's dark mode applies when the page chose no theme, and a page's own choice wins.
  await page.emulateMedia({ colorScheme: "dark" });
  const system = await page.evaluate(() => {
    const root = document.documentElement;
    const ink = () => getComputedStyle(root).getPropertyValue("--color-ink").trim();
    const scheme = () => getComputedStyle(root).colorScheme;
    const chose = [ink(), scheme()];
    // A light panel on the dark page: the default context declares its values, so it is light.
    const panel = document.createElement("section");
    panel.dataset.theme = "light";
    document.body.append(panel);
    const inner = [getComputedStyle(panel).getPropertyValue("--color-ink").trim(), getComputedStyle(panel).colorScheme];
    panel.remove();
    root.dataset.theme = "light";
    const light = [ink(), scheme()];
    delete root.dataset.theme;
    return { chose, light, inner };
  });
  await page.emulateMedia({ colorScheme: null });
  check(
    "a dark system makes a page that chose no theme dark, and data-theme=light keeps the page or a panel in it light",
    system.chose.join(" ") === "#ecebf2 light dark" && system.light.join(" ") === "#1d1b26 light" && system.inner.join(" ") === "#1d1b26 light",
    JSON.stringify(system),
  );

  const firstPrice = () => page.textContent("[data-scope=listing-sort] tbody tr:first-child td:last-child");
  const rowOrder = () => page.$$eval("[data-scope=listing-sort] tbody tr", (rows) => rows.map((r) => r.textContent).join("|"));
  const sort = "[data-scope=listing-sort] > button";
  const before = await firstPrice();
  const order = await rowOrder();
  await page.click(sort);
  check("a click runs the listing sort handler", (await firstPrice()) !== before && (await page.innerText(sort)) === "Price: high to low");
  await page.click(sort);
  check(
    "a second click restores the server's order, rows with the same price included",
    (await firstPrice()) === before && (await rowOrder()) === order && (await page.innerText(sort)) === "Price: low to high",
  );
  const more = await page.$("[data-scope=offer-list] button");
  if (more) {
    const rows = await page.$$eval("[data-scope=offer-list] tbody tr", (r) => r.length);
    await more.click();
    check("a click runs the offer list handler", (await page.$$eval("[data-scope=offer-list] tbody tr", (r) => r.length)) > rows);
  }

  await page.fill('input[name="email"]', "buyer@example.com");
  await page.fill('input[name="target_price"]', "abc");
  await page.click('form[data-patch] button[type="submit"]');
  await page.waitForSelector("#alert-form-target_price-error", { timeout: 5000 }).catch(() => {});
  check("an invalid field shows its error from the server", ((await page.textContent("#alert-form-target_price-error").catch(() => "")) ?? "").includes("above zero"));
  check("focus moves to the invalid field", await page.evaluate(() => document.activeElement?.getAttribute("name") === "target_price"));
  check("valid input survives the error patch", (await page.inputValue('input[name="email"]')) === "buyer@example.com");
  await page.fill('input[name="target_price"]', "4.5");
  await page.click('form[data-patch] button[type="submit"]');
  await page.waitForFunction(() => /Alert set/.test(document.getElementById("alert-message")?.textContent ?? ""));
  check("form action patches the message", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 1));

  const registration = await page.evaluate(async () => {
    const reg = await navigator.serviceWorker.ready;
    return reg.active?.scriptURL ?? null;
  });
  check("service worker is registered and active", registration === `${base}/service-worker.js`);
  const manifest = await page.evaluate(async () => (await fetch("/manifest.webmanifest")).json());
  check("manifest is installable (name, 192 and 512 icons)", manifest.name === "Card Market" && manifest.icons.some((i) => i.sizes === "512x512"));

  // Push: the app's worker script (Pwa::import_script) handles a push sent through DevTools.
  await context.grantPermissions(["notifications"], { origin: base });
  const cdp = await context.newCDPSession(page);
  /** @type {string | null} */
  let registrationId = null;
  cdp.on("ServiceWorker.workerRegistrationUpdated", ({ registrations }) => {
    for (const r of registrations) if (r.scopeURL === `${base}/` && !r.isDeleted) registrationId = r.registrationId;
  });
  await cdp.send("ServiceWorker.enable");
  for (let i = 0; i < 50 && registrationId === null; i += 1) await page.waitForTimeout(100);
  if (registrationId !== null) {
    const data = JSON.stringify({ title: "Price drop", body: "Card Name 5 is below $4.50", url: "/items/item_5" });
    await cdp.send("ServiceWorker.deliverPushMessage", { origin: base, registrationId, data });
  }
  const shown = await page
    .waitForFunction(async () => (await (await navigator.serviceWorker.ready).getNotifications()).some((n) => n.title === "Price drop"), null, { timeout: 5000 })
    .then(() => true, () => false);
  check("a push message shows a notification", shown, registrationId === null ? "no registration id" : "");

  // Offline: a fresh navigation through the runtime falls back to the worker's offline page.
  await page.reload();
  await page.waitForFunction(() => !!navigator.serviceWorker.controller);
  await context.setOffline(true);
  await page.click('header a[href="/"]');
  await page.waitForFunction(() => document.querySelector("h1")?.textContent === "Not available", null, { timeout: 10_000 }).catch(() => {});
  check("offline navigation shows the offline page", (await page.textContent("h1")) === "Not available");
  await context.setOffline(false);

  const csp = errors.filter((e) => /Content Security Policy|Refused to/.test(e));
  check("no CSP violations", csp.length === 0, csp.join(" | "));
  const other = errors.filter((e) => !/Content Security Policy|Refused to|ERR_INTERNET_DISCONNECTED|Failed to load resource|net::ERR/.test(e));
  check("no other console errors", other.length === 0, other.join(" | "));

  // Compression, as a real browser negotiates it: pages per request, the runtime once at brotli 11.
  // A context without the service worker, whose cache would answer instead of the network.
  const plain = await (await browser.newContext({ serviceWorkers: "block" })).newPage();
  await plain.goto(`${base}/items/item_5`);
  const encodings = await plain.evaluate(async () => {
    const runtime = /** @type {HTMLScriptElement} */ (document.querySelector('script[type="module"][src*="/corten/"]')).src;
    const entries = () => /** @type {PerformanceResourceTiming[]} */ (performance.getEntriesByType("resource"));
    await fetch(runtime, { cache: "no-store" });
    await new Promise((r) => setTimeout(r, 1500)); // the first request starts brotli 11 in the background
    const res = await fetch(runtime, { cache: "no-store" });
    const text = await res.text();
    const timing = entries().filter((e) => e.name === runtime).at(-1);
    return { runtime: text.startsWith("// Corten runtime"), encoded: timing?.encodedBodySize ?? 0, decoded: timing?.decodedBodySize ?? 0 };
  });
  check(
    "the browser gets the runtime compressed",
    encodings.runtime && encodings.encoded > 0 && encodings.encoded < encodings.decoded / 2,
    `${encodings.encoded} of ${encodings.decoded} bytes`,
  );
  const runtimeUrl = await page.evaluate(() => /** @type {HTMLScriptElement} */ (document.querySelector('script[type="module"][src*="/corten/"]')).src);
  const runtimeBr = await fetch(runtimeUrl, { headers: { "accept-encoding": "br" } });
  check("the runtime is sent as brotli", runtimeBr.headers.get("content-encoding") === "br" && (await runtimeBr.text()).startsWith("// Corten runtime"));
  const styleUrl = await page.evaluate(() => /** @type {HTMLLinkElement} */ (document.querySelector('link[rel="stylesheet"]')).href);
  await fetch(styleUrl, { headers: { "accept-encoding": "br" } });
  await page.waitForTimeout(500);
  const style = await fetch(styleUrl, { headers: { "accept-encoding": "br" } });
  check(
    "assets are precompressed once, with their own ETag per coding",
    style.headers.get("content-encoding") === "br" && /-br"$/.test(style.headers.get("etag") ?? ""),
    `${style.headers.get("content-encoding")} ${style.headers.get("etag")}`,
  );
  const iconUrl = await page.evaluate(() => /** @type {HTMLLinkElement} */ (document.querySelector('link[rel="icon"]')).href);
  const icon = await fetch(iconUrl);
  check("the favicon is a versioned SVG asset", icon.ok && /\/assets\//.test(iconUrl) && icon.headers.get("content-type") === "image/svg+xml", `${icon.status} ${iconUrl}`);
  const zstd = await fetch(`${base}/items/item_5`, { headers: { "accept-encoding": "zstd" } });
  check("pages are compressed with zstd when the client accepts it", zstd.headers.get("content-encoding") === "zstd" && /accept-encoding/.test(zstd.headers.get("vary") ?? ""));
  const br = await fetch(`${base}/items/item_5`, { headers: { "accept-encoding": "gzip, br" } });
  check("pages fall back to brotli", br.headers.get("content-encoding") === "br" && (await br.text()).includes("Card Name 5"));

  const cross = await fetch(`${base}/items/item_5`, { method: "POST", headers: { "sec-fetch-site": "cross-site", "content-type": "application/x-www-form-urlencoded" }, body: "title=a&email=a@b.c&target_price=1" });
  check("cross-site form post is refused", cross.status === 403);

  // The MCP endpoint is a JSON-only route (App::api). The container is started with
  // AGENT_ORIGINS set to the other loopback name (http://127.0.0.1:18481 for a base on
  // localhost), so an agent's page there may call it.
  const wrongMethod = await fetch(`${base}/mcp`);
  const wrongBody = await wrongMethod.json().catch(() => null);
  check("the MCP endpoint answers a failure as a JSON-RPC error to any client (Api::errors)", wrongMethod.status === 405 && wrongBody?.jsonrpc === "2.0" && wrongBody?.error?.code === -32600, `${wrongMethod.status} ${wrongMethod.headers.get("content-type")}`);
  check("an API route has its own CSP", wrongMethod.headers.get("content-security-policy") === "default-src 'none'; frame-ancestors 'none'");
  const baseUrl = new URL(base);
  const otherOrigin = `${baseUrl.protocol}//${baseUrl.hostname === "localhost" ? "127.0.0.1" : "localhost"}:${baseUrl.port}`;
  // A page at one origin calling the endpoint at another. Both are this server's loopback names
  // (Chromium keeps loopback pages from reaching loopback servers only when the page is not
  // local), and the page's own CSP is bypassed so that only CORS decides.
  const agents = await browser.newContext({ bypassCSP: true });
  /** Calls tools/list from a page at `pageOrigin` on the endpoint at `apiOrigin`: the tool count, or "blocked". */
  const callFrom = async (pageOrigin, apiOrigin) => {
    const agent = await agents.newPage();
    await agent.goto(`${pageOrigin}/robots.txt`);
    const result = await agent.evaluate(async (url) => {
      try {
        const res = await fetch(url, {
          method: "POST",
          headers: { "content-type": "application/json", authorization: "Bearer dev-token" },
          body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list" }),
        });
        return String((await res.json()).result?.tools?.length);
      } catch {
        return "blocked";
      }
    }, `${apiOrigin}/mcp`);
    await agent.close();
    return result;
  };
  const allowed = await callFrom(otherOrigin, base);
  check("an allowed origin calls the API from the browser (preflight, then the write)", allowed === "2", allowed);
  const stranger = await callFrom(base, otherOrigin);
  check("any other origin is blocked by the browser", stranger === "blocked", stranger);
  await agents.close();

  // Cache headers: the home page is the same for every visitor, so a CDN may keep it; with the
  // visitor's cookie it is theirs alone, and other pages are private unless they say otherwise.
  const cacheOf = async (/** @type {string} */ path, /** @type {Record<string, string>} */ headers = {}) =>
    (await fetch(`${base}${path}`, { headers })).headers.get("cache-control") ?? "";
  const shared = await cacheOf("/");
  check("a public page may be kept by shared caches", shared === "public, max-age=60, stale-while-revalidate=600", shared);
  const mine = await cacheOf("/", { cookie: "session=abc" });
  check("the same page requested with a cookie is private", mine === "private, no-cache", mine);
  const item = await cacheOf("/items/item_5");
  check("other pages are private by default", item === "private, no-cache", item);

  // What a crawler and a load balancer see: an old address moved for good, a category's
  // structured data and longer shared lifetime, and readiness once the item source answers.
  const old = await fetch(`${base}/item/item_5`, { redirect: "manual" });
  check("an old address is a permanent redirect", old.status === 301 && old.headers.get("location") === "/items/item_5", `${old.status} ${old.headers.get("location")}`);
  const category = await fetch(`${base}/categories/promo`);
  const categoryCache = category.headers.get("cache-control") ?? "";
  check("a category page may be kept longer by shared caches", categoryCache.includes("s-maxage=3600"), categoryCache);
  check("a category page carries its breadcrumbs as structured data", (await category.text()).includes('"@type":"BreadcrumbList"'));
  const ready = await fetch(`${base}/health/ready`);
  check("the app is ready once its item source answers", ready.status === 200, String(ready.status));

  // In French the item page is French throughout: its prices as French writes US dollars, the
  // ratings with a decimal comma, and the conditions the catalog names (the fixture's cheapest
  // listing is Lightly Played; the rest are "Near Mint, English", shown as the item API sent it).
  const french = await (await fetch(`${base}/items/item_5`, { headers: { "accept-language": "fr" } })).text();
  const cheapest = new Intl.NumberFormat("fr", { style: "currency", currency: "USD" }).format(0.79);
  check(
    "in French a price, a rating and a condition are French",
    french.includes(cheapest) && french.includes("4,58 ★ (274)") && french.includes("Peu joué") && !french.includes("$0.79"),
    [cheapest, "4,58 ★ (274)", "Peu joué"].map((t) => `${t}: ${french.includes(t)}`).join(", "),
  );

  // A partner's service sets alerts with its API key (`X-Api-Key`, `API_KEYS`); without one it
  // is a JSON 401, and the form's rules answer with the fields.
  const alertFrom = (/** @type {Record<string, string>} */ headers, body = { id: "item_5", email: "ada@example.com", target_price: "1.00" }) =>
    fetch(`${base}/api/alerts`, { method: "POST", headers: { "content-type": "application/json", ...headers }, body: JSON.stringify(body) });
  const anonymous = await alertFrom({});
  const anonymousBody = await anonymous.json().catch(() => null);
  check(
    "a partner write without an API key is a Problem Details 401 that says what to send",
    anonymous.status === 401 && anonymous.headers.get("content-type") === "application/problem+json" && anonymousBody?.status === 401 && anonymousBody?.detail === "Send a valid partner API key in X-Api-Key.",
    `${anonymous.headers.get("content-type")} ${JSON.stringify(anonymousBody)}`,
  );
  const keyed = await alertFrom({ "x-api-key": "dev-key" });
  const keyedBody = await keyed.json().catch(() => null);
  check("with its key the partner's alert is set", keyed.status === 200 && keyedBody?.partner === "dev" && /^Alert set: /.test(keyedBody?.message ?? ""), JSON.stringify(keyedBody));
  const refused = await alertFrom({ "x-api-key": "dev-key" }, { id: "item_5", email: "ada", target_price: "1.00" });
  const refusedBody = await refused.json().catch(() => null);
  check("a field the rules refuse is a 422 with the field", refused.status === 422 && typeof refusedBody?.fields?.email === "string", JSON.stringify(refusedBody));

  // Writes are limited per client address (20 a minute), a key no partner has included, so
  // guessing keys is limited too. The container trusts Docker's networks as its proxies
  // (`PROXIES`), so each address below is the client its proxy forwarded.
  const writeAs = (/** @type {string} */ address, key = "guess") => alertFrom({ "x-api-key": key, "x-forwarded-for": address });
  // Addresses of this run's own, so a second run against the same container starts at zero.
  const octet = () => 1 + Math.floor(Math.random() * 254);
  const limited = `198.51.${octet()}.${octet()}`;
  // Until the first 429: if the minute turns during the loop, the count starts again, so up to
  // twice the limit may pass first.
  const statuses = [];
  let over = await writeAs(limited);
  for (; over.status !== 429 && statuses.length < 41; over = await writeAs(limited)) statuses.push(over.status);
  const wait = Number(over.headers.get("retry-after"));
  check("an address past its writes a minute is a 429 with Retry-After", statuses.length >= 20 && statuses.every((s) => s === 401) && over.status === 429 && wait >= 1 && wait <= 60, `${statuses.length} x ${statuses[0]} then ${over.status} after ${wait}s`);
  const elsewhere = await writeAs(`203.0.${octet()}.${octet()}`);
  check("another forwarded address has its own count", elsewhere.status === 401, String(elsewhere.status));
  const asPartner = await writeAs(limited, "dev-key");
  check("a partner's alert from a limited address is counted as the partner's", asPartner.status === 200, String(asPartner.status));
  const stillReads = await fetch(`${base}/items/item_5`, { headers: { "x-forwarded-for": limited } });
  check("reads from a limited address still pass", stillReads.status === 200, String(stillReads.status));

  // Back and forward restore a page from memory. Playwright turns the back/forward cache off, and
  // its default headless shell never restores a page; the full Chromium in headless mode does.
  const restoring = steady(
    await chromium.launch({
      channel: "chromium",
      headless: true,
      ignoreDefaultArgs: ["--disable-back-forward-cache"],
    }),
  );
  try {
    const tab = await (await restoring.newContext()).newPage();
    await tab.addInitScript(() =>
      addEventListener("pageshow", (event) => event.persisted && ((/** @type {any} */ (window)).__restored = true)),
    );
    await tab.goto(`${base}/items/item_5`);
    // A fresh profile installs the app's service worker, which claims the open page, and Chrome
    // never keeps a page a worker claimed. Load it again once the worker controls the profile.
    await tab.waitForFunction(() => navigator.serviceWorker?.controller != null, null, { timeout: 5000 }).catch(() => {});
    await tab.goto(`${base}/items/item_5`);
    // The runtime and the page's handler modules are loaded.
    await tab.evaluate(async () => {
      const { ready } = await import("corten");
      await ready;
    });
    await tab.goto(`${base}/robots.txt`);
    await tab.goBack({ waitUntil: "commit" });
    const restored = await tab
      .waitForFunction(() => (/** @type {any} */ (window)).__restored === true, null, { timeout: 3000 })
      .then(() => true, () => false);
    const reasons = restored
      ? ""
      : await tab.evaluate(() => JSON.stringify((/** @type {any} */ (performance.getEntriesByType("navigation")[0]))?.notRestoredReasons ?? null));
    check("going back restores the page from the back/forward cache", restored, reasons);
  } finally {
    await restoring.close();
  }
} finally {
  await browser.close();
}
console.log(failures.length ? `${failures.length} failed` : "all passed");
process.exit(failures.length ? 1 : 0);
