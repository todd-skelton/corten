// The server-rendered page: 1,000 rows arrive in the HTML and each app takes them over (adopt,
// hydrate). Measures, over fresh browser contexts: when the app is ready (and how long it took
// after the HTML was parsed), rows the app removed
// or added while starting (0 means it kept the server's), the JS heap, the page's HTML size, and
// the first interactions on the taken-over rows.
// Usage: node ssr.mjs [--loads 15]
import { createRequire } from "node:module";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { serve } from "./serve.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const { chromium } = createRequire(join(here, "..", "..", "package.json"))("@playwright/test");
const args = Object.fromEntries(process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith("--") ? [...acc, [a.slice(2), all[i + 1]]] : acc), []));
const LOADS = Number(args.loads ?? 15);
const PORT = 44201;
const APPS = args.apps ? args.apps.split(",") : ["vanilla-ssr", "state-ssr", "view-ssr", "solid-ssr", "remix-ssr"];
const median = (xs) => { const s = [...xs].sort((a, b) => a - b); const m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
const round = (x, d = 2) => Math.round(x * 10 ** d) / 10 ** d;

// Counts rows the app takes out of or puts into the table while it starts: everything after the
// parser is done (module scripts run after that) and before the app says it is ready.
const watch = () => {
  const counts = { removed: 0, added: 0 };
  /** @type {any} */ (window).__rows = counts;
  let parsed = false;
  const observer = new MutationObserver((records) => {
    if (!parsed || document.documentElement.dataset.ready !== undefined) return;
    for (const r of records) {
      for (const n of r.removedNodes) if (n.nodeName === "TR") counts.removed++;
      for (const n of r.addedNodes) if (n.nodeName === "TR") counts.added++;
    }
  });
  observer.observe(document, { childList: true, subtree: true });
  document.addEventListener("readystatechange", () => {
    if (document.readyState !== "interactive") return;
    observer.takeRecords();
    parsed = true;
  });
};

async function interact(page, selector) {
  return page.evaluate(async (sel) => {
    const t0 = performance.now();
    document.querySelector(sel).click();
    await new Promise((r) => queueMicrotask(() => queueMicrotask(r)));
    return performance.now() - t0;
  }, selector);
}

const server = await serve(PORT);
const browser = await chromium.launch({ headless: true });
const out = { date: new Date().toISOString(), chromium: browser.version(), loads: LOADS, apps: {} };
for (const app of APPS) {
  const html = readFileSync(join(here, "out", app, "index.html"));
  const runs = [];
  let heapKB = 0;
  let errors = 0;
  for (let i = 0; i < LOADS; i++) {
    const context = await browser.newContext();
    const page = await context.newPage();
    page.on("pageerror", () => errors++);
    await page.addInitScript(watch);
    await page.goto(`http://127.0.0.1:${PORT}/${app}/`);
    await page.waitForFunction(() => document.documentElement.dataset.ready);
    const start = await page.evaluate(() => ({ ready: Number(document.documentElement.dataset.ready), takeover: Number(document.documentElement.dataset.ready) - performance.getEntriesByType("navigation")[0].domInteractive, rows: document.querySelectorAll("tbody tr").length, ...(/** @type {any} */ (window).__rows) }));
    if (i === 0) {
      const cdp = await context.newCDPSession(page);
      for (let k = 0; k < 3; k++) await cdp.send("HeapProfiler.collectGarbage");
      heapKB = round((await cdp.send("Runtime.getHeapUsage")).usedSize / 1024, 0);
    }
    const select = await interact(page, "tbody tr:nth-child(2) a.select");
    const ok1 = await page.evaluate(() => (document.querySelector("tbody tr:nth-child(2)").classList.contains("danger") || document.querySelector("tbody tr:nth-child(2)").hasAttribute("data-selected")));
    const update = await interact(page, "#update");
    const ok2 = await page.evaluate(() => document.querySelector("tbody tr:nth-child(1) a.select").textContent.endsWith(" !!!"));
    const remove = await interact(page, "tbody tr:nth-child(4) a.remove");
    const ok3 = await page.evaluate(() => document.querySelectorAll("tbody tr").length === 999);
    if (!(ok1 && ok2 && ok3)) throw new Error(`${app}: interactions did not take effect (${ok1} ${ok2} ${ok3})`);
    runs.push({ ...start, select, update, remove });
    await context.close();
  }
  out.apps[app] = {
    htmlB: html.length,
    htmlGzipB: gzipSync(html, { level: 9 }).length,
    readyMs: round(median(runs.map((r) => r.ready))),
    // From the parser finishing (domInteractive) to the app being ready: the app's own start.
    takeoverMs: round(median(runs.map((r) => r.takeover))),
    rowsAtReady: runs[0].rows,
    rowsRemoved: Math.max(...runs.map((r) => r.removed)),
    rowsAdded: Math.max(...runs.map((r) => r.added)),
    heapKB,
    firstSelectMs: round(median(runs.map((r) => r.select))),
    firstUpdateMs: round(median(runs.map((r) => r.update))),
    firstRemoveMs: round(median(runs.map((r) => r.remove))),
    errors,
  };
  console.error(app, JSON.stringify(out.apps[app]));
}
await browser.close();
server.close();
mkdirSync(join(here, "results"), { recursive: true });
writeFileSync(join(here, "results", `ssr-${out.date.slice(0, 10)}.json`), JSON.stringify(out, null, 2));
const keys = Object.keys(out.apps[APPS[0]]);
console.log(`chromium ${out.chromium}; ${LOADS} fresh loads each; medians`);
console.log("".padEnd(16), ...APPS.map((a) => a.padStart(12)));
for (const k of keys) console.log(k.padEnd(16), ...APPS.map((a) => String(out.apps[a][k]).padStart(12)));
