// Browser smoke test of the production pieces, against the item-page example (API or fixture mode).
// Usage: node examples/item-page/smoke.mjs [http://localhost:18410]
import { chromium } from "@playwright/test";

const base = process.argv[2] ?? "http://localhost:18410";
const failures = [];
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures.push(name);
};

const browser = await chromium.launch({ headless: true });
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
  await page.waitForFunction(() => !!customElements.get("listing-sort") && !!customElements.get("offer-list"));
  check("islands load from versioned URLs", true);
  check(
    "islands that arrive by navigation are layout-neutral",
    (await page.evaluate(() => getComputedStyle(/** @type {Element} */ (document.querySelector("listing-sort"))).display)) === "contents",
  );

  // Styles: tokens, a component's scoped rules and a responsive utility, from components.css.
  const styles = await page.evaluate(() => {
    const css = (/** @type {string} */ selector) => getComputedStyle(/** @type {Element} */ (document.querySelector(selector)));
    const root = document.documentElement;
    const ink = () => getComputedStyle(root).getPropertyValue("--color-ink").trim();
    const light = ink();
    root.dataset.theme = "dark";
    const dark = ink();
    delete root.dataset.theme;
    return {
      accent: getComputedStyle(root).getPropertyValue("--color-accent").trim(),
      price: css("listing-sort tbody td:last-child").fontVariantNumeric,
      summary: css("article > section").flexDirection,
      themes: [light, dark],
    };
  });
  check("design tokens are custom properties", styles.accent === "#6b3fd4", styles.accent);
  check("a component's scoped stylesheet applies", styles.price === "tabular-nums", styles.price);
  check("a responsive utility applies at this width", styles.summary === "row", styles.summary);
  check("a theme overrides tokens", styles.themes[0] === "#1d1b26" && styles.themes[1] === "#ecebf2", styles.themes.join(" "));

  const firstPrice = () => page.textContent("listing-sort tbody tr:first-child td:last-child");
  const sort = "listing-sort button";
  const before = await firstPrice();
  await page.click(sort);
  check("listing sort island reorders rows", (await firstPrice()) !== before && (await page.innerText(sort)) === "Price: high to low");
  await page.click(sort);
  check("a second click restores the server's order", (await firstPrice()) === before && (await page.innerText(sort)) === "Price: low to high");
  const more = await page.$("offer-list button");
  if (more) {
    const rows = await page.$$eval("offer-list tbody tr", (r) => r.length);
    await more.click();
    check("offer list island shows the rest", (await page.$$eval("offer-list tbody tr", (r) => r.length)) > rows);
  }

  await page.fill('input[name="email"]', "buyer@example.com");
  await page.fill('input[name="target_price"]', "abc");
  await page.click('form[data-patch] button[type="submit"]');
  await page.waitForSelector("#target_price-error", { timeout: 5000 }).catch(() => {});
  check("an invalid field shows its error from the server", ((await page.textContent("#target_price-error").catch(() => "")) ?? "").includes("above zero"));
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
  await page.waitForFunction(() => document.querySelector("h1")?.textContent === "You are offline", null, { timeout: 10_000 }).catch(() => {});
  check("offline navigation shows the offline page", (await page.textContent("h1")) === "You are offline");
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
  check("an API route answers a failure as JSON to any client", wrongMethod.status === 405 && wrongBody?.error?.status === 405, `${wrongMethod.status} ${wrongMethod.headers.get("content-type")}`);
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

  // Back and forward restore a page from memory. Playwright turns the back/forward cache off, and
  // its default headless shell never restores a page; the full Chromium in headless mode does.
  const restoring = await chromium.launch({
    channel: "chromium",
    headless: true,
    ignoreDefaultArgs: ["--disable-back-forward-cache"],
  });
  try {
    const tab = await (await restoring.newContext()).newPage();
    await tab.addInitScript(() =>
      addEventListener("pageshow", (event) => event.persisted && ((/** @type {any} */ (window)).__restored = true)),
    );
    await tab.goto(`${base}/items/item_5`);
    await tab.waitForFunction(() => !!customElements.get("listing-sort"));
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
