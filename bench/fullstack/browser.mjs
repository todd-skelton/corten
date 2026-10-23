// Browser timing for both apps with Playwright Chromium: cold-cache item-page loads,
// client-side navigation and a form action, on desktop (no throttling) and a
// Lighthouse-style mobile profile (4x CPU slowdown, 150 ms RTT, 1.6 Mbps down).
// Usage: node browser.mjs name=url [name=url ...] [runs=10] [out=r1/browser.json]
import { chromium } from "@playwright/test";
import { createServer } from "node:http";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const argv = process.argv.slice(2);
const named = Object.fromEntries(argv.filter((a) => a.includes("=")).map((a) => [a.slice(0, a.indexOf("=")), a.slice(a.indexOf("=") + 1)]));
const { runs: runsNamed, out: outName = "browser.json", ...namedApps } = named;
const positional = argv.filter((a) => !a.includes("="));
const apps = Object.keys(namedApps).length ? namedApps : { react: positional[0], leptos: positional[1] };
const runs = Number(runsNamed ?? positional[2] ?? 10);
const profiles = {
  desktop: { cpu: 1, network: null },
  mobile: { cpu: 4, network: { offline: false, latency: 150, downloadThroughput: (1.6 * 1024 * 1024) / 8, uploadThroughput: (750 * 1024) / 8 } },
};
const items = ["item_3", "item_17", "item_250", "item_1999", "item_12345"];
const SORT = "button.sort, [data-scope=listing-sort] > button, listing-sort > button";
const FIRST_PRICE = ".listings tbody tr:first-child td.price, [data-scope=listing-sort] tbody tr:first-child td.price__amount, listing-sort tbody tr:first-child td.price__amount";

// Each cold load starts on a page from another site, like arriving from a search result or a
// link. Starting from about:blank instead would make Chromium switch processes only for apps
// that send Cross-Origin-Opener-Policy, which is a harness artifact, not a user-visible cost.
const startPage = createServer((_, res) => res.end("<!DOCTYPE html><title>start</title><p>start</p>"));
await new Promise((resolve) => startPage.listen(0, "127.0.0.1", resolve));
const startUrl = `http://127.0.0.1:${/** @type {any} */ (startPage.address()).port}/`;

const observe = () => {
  window.__lcp = 0;
  window.__longTasks = [];
  new PerformanceObserver((list) => { for (const e of list.getEntries()) window.__lcp = e.startTime; }).observe({ type: "largest-contentful-paint", buffered: true });
  new PerformanceObserver((list) => { for (const e of list.getEntries()) window.__longTasks.push([e.startTime, e.duration]); }).observe({ type: "longtask", buffered: true });
};

async function newPage(browser, profile) {
  const context = await browser.newContext();
  await context.addInitScript(observe);
  const page = await context.newPage();
  const cdp = await context.newCDPSession(page);
  await cdp.send("Network.enable");
  await cdp.send("Network.setCacheDisabled", { cacheDisabled: true });
  if (profile.network) await cdp.send("Network.emulateNetworkConditions", profile.network);
  if (profile.cpu > 1) await cdp.send("Emulation.setCPUThrottlingRate", { rate: profile.cpu });
  await cdp.send("Performance.enable");
  return { context, page, cdp };
}

const hydrated = (page) => page.waitForFunction(() => typeof window.__hydratedAt === "number", null, { timeout: 60_000 });

async function loadItem(browser, profile, base, id) {
  const { context, page, cdp } = await newPage(browser, profile);
  try {
    await page.goto(startUrl);
    await page.goto(`${base}/items/${id}`, { waitUntil: "load", timeout: 60_000 });
    await hydrated(page);
    await page.waitForTimeout(300);
    const m = await page.evaluate(() => {
      const nav = performance.getEntriesByType("navigation")[0];
      const fcp = performance.getEntriesByName("first-contentful-paint")[0]?.startTime ?? null;
      const resources = performance.getEntriesByType("resource");
      const bytes = { html: nav.transferSize, js: 0, wasm: 0, css: 0, data: 0 };
      for (const r of resources) {
        const kind = /\.wasm(\?|$)/.test(r.name) ? "wasm" : /\.m?js(\?|$)/.test(r.name) ? "js" : /\.css(\?|$)/.test(r.name) ? "css" : "data";
        bytes[kind] += r.transferSize;
      }
      const hydratedAt = window.__hydratedAt;
      const tbt = window.__longTasks.filter(([s]) => s < hydratedAt).reduce((sum, [, d]) => sum + Math.max(0, d - 50), 0);
      return { ttfb: nav.responseStart, fcp, lcp: window.__lcp, dcl: nav.domContentLoadedEventEnd, load: nav.loadEventEnd, hydratedAt, tbtToHydration: tbt, bytes, requests: resources.length + 1 };
    });
    const metrics = Object.fromEntries((await cdp.send("Performance.getMetrics")).metrics.map((x) => [x.name, x.value]));
    m.jsHeapUsedMB = metrics.JSHeapUsedSize / 1048576;
    // The other apps' class names, or Corten's markup: the `listing-sort` component's scope (since
    // handlers replaced islands) or the older `<listing-sort>` island. Visible text: Corten hides
    // one of two labels.
    const before = await page.locator(SORT).innerText();
    const priceCell = page.locator(FIRST_PRICE);
    const firstPrice = (await priceCell.count()) ? await priceCell.textContent() : null;
    await page.locator(SORT).click();
    await page.waitForFunction(([sel, b]) => /** @type {HTMLElement | null} */ (document.querySelector(sel))?.innerText !== b, [SORT, before], { timeout: 10_000 });
    const t0 = await page.evaluate(() => performance.now());
    await page.locator('input[name="email"]').fill("buyer@example.com");
    await page.locator('input[name="target_price"]').fill("4.5");
    await page.locator('button[type="submit"]').click();
    await page.waitForFunction(() => /Alert set/.test(document.querySelector(".alert-message, #alert-message")?.textContent ?? ""), null, { timeout: 30_000 });
    m.actionMs = (await page.evaluate(() => performance.now())) - t0;
    m.sortWorked = firstPrice === null ? null : (await priceCell.textContent()) !== firstPrice || before !== null;
    return m;
  } finally {
    await context.close();
  }
}

async function clientNav(browser, profile, base) {
  const { context, page } = await newPage(browser, profile);
  try {
    await page.goto(`${base}/`, { waitUntil: "load", timeout: 60_000 });
    await hydrated(page);
    await page.waitForTimeout(300);
    return await page.evaluate(async () => {
      const start = performance.now();
      document.querySelector('a[href="/items/item_5"]').click();
      await new Promise((resolve) => {
        const done = () => document.querySelector("h1")?.textContent === "Card Name 5" && document.querySelectorAll(".listings tbody tr, [data-scope=listing-sort] tbody tr, listing-sort tbody tr").length > 0;
        if (done()) return resolve();
        // The whole document, not document.body: Turbo replaces the <body> element itself.
        new MutationObserver((_, o) => { if (done()) { o.disconnect(); resolve(); } }).observe(document.documentElement, { childList: true, subtree: true, characterData: true });
      });
      return performance.now() - start;
    });
  } finally {
    await context.close();
  }
}

const browser = await chromium.launch({ headless: true });
const results = [];
try {
  for (const [profileName, profile] of Object.entries(profiles)) {
    for (let i = 0; i < runs; i += 1) {
      for (const [app, base] of Object.entries(apps)) {
        const id = items[i % items.length];
        const load = await loadItem(browser, profile, base, id);
        const navMs = await clientNav(browser, profile, base);
        const row = { profile: profileName, app, i, id, ...load, clientNavMs: navMs };
        results.push(row);
        console.log(JSON.stringify(row));
      }
    }
  }
} finally {
  await browser.close();
  startPage.close();
}
mkdirSync(dirname(join(here, "results", outName)), { recursive: true });
writeFileSync(join(here, "results", outName), JSON.stringify(results, null, 1));
console.log("done");
