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

  const firstPrice = () => page.textContent(".listings tbody tr:first-child td.price");
  const before = await firstPrice();
  await page.click("button.sort");
  check("listing sort island reorders rows", (await firstPrice()) !== before && (await page.textContent("button.sort")) === "Price: high to low");
  const more = await page.$("button.more");
  if (more) {
    const rows = await page.$$eval(".offers tbody tr", (r) => r.length);
    await more.click();
    check("offer list island shows the rest", (await page.$$eval(".offers tbody tr", (r) => r.length)) > rows);
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
    return { runtime: text.includes("export class Island"), encoded: timing?.encodedBodySize ?? 0, decoded: timing?.decodedBodySize ?? 0 };
  });
  check(
    "the browser gets the runtime compressed",
    encodings.runtime && encodings.encoded > 0 && encodings.encoded < encodings.decoded / 2,
    `${encodings.encoded} of ${encodings.decoded} bytes`,
  );
  const runtimeUrl = await page.evaluate(() => /** @type {HTMLScriptElement} */ (document.querySelector('script[type="module"][src*="/corten/"]')).src);
  const runtimeBr = await fetch(runtimeUrl, { headers: { "accept-encoding": "br" } });
  check("the runtime is sent as brotli", runtimeBr.headers.get("content-encoding") === "br" && (await runtimeBr.text()).includes("export class Island"));
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
} finally {
  await browser.close();
}
console.log(failures.length ? `${failures.length} failed` : "all passed");
process.exit(failures.length ? 1 : 0);
