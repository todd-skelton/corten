// The dashboard: a tick changes 20 (or all 200) prices in a shared store; measure from the click
// to the DOM updated and to the next frame, and check every changed card and the total.
// Usage: node dashboard.mjs [--iterations 30] [--apps dashboard-view,dashboard-solid] [--out results/dashboard-<date>.json]
import { createRequire } from "node:module";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { serve } from "./serve.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const { chromium } = createRequire(join(here, "..", "..", "package.json"))("@playwright/test");
const args = Object.fromEntries(process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith("--") ? [...acc, [a.slice(2), all[i + 1]]] : acc), []));
const N = Number(args.iterations ?? 30);
const PORT = 44203;
const APPS = args.apps ? args.apps.split(",") : ["dashboard-vanilla", "dashboard-state", "dashboard-view", "dashboard-solid", "dashboard-remix", "dashboard-react", "dashboard-svelte", "dashboard-vue"];
const median = (xs) => { const s = [...xs].sort((a, b) => a - b); const m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
const round = (x, d = 2) => Math.round(x * 10 ** d) / 10 ** d;
const sizes = JSON.parse(readFileSync(join(here, "out", "sizes.json"), "utf8"));

const server = await serve(PORT);
const browser = await chromium.launch({ headless: true });
const out = { date: new Date().toISOString(), chromium: browser.version(), iterations: N, apps: {} };
for (const app of APPS) {
  const page = await browser.newPage();
  let errors = 0;
  page.on("pageerror", () => errors++);
  await page.goto(`http://127.0.0.1:${PORT}/${app}/`);
  await page.waitForFunction(() => document.documentElement.dataset.ready);
  const result = { jsGzipB: sizes[app].gzip };
  for (const [name, button] of [["tick 20", "#tick20"], ["tick 200", "#tick200"]]) {
    const commit = [];
    const frame = [];
    for (let i = 0; i < 5 + N; i++) {
      const m = await page.evaluate(async (sel) => {
        const spans = () => Array.from(document.querySelectorAll(".card span"), (s) => s.textContent);
        const before = spans();
        const t0 = performance.now();
        document.querySelector(sel).click();
        await new Promise((r) => queueMicrotask(() => queueMicrotask(r)));
        const t1 = performance.now();
        const after = spans();
        await new Promise((r) => requestAnimationFrame(() => setTimeout(r, 0)));
        const total = after.reduce((t, s) => t + Number(s), 0).toFixed(2);
        const shown = document.querySelector("p span").textContent;
        return { commit: t1 - t0, frame: performance.now() - t0, changed: after.filter((s, k) => s !== before[k]).length, // Shown prices are rounded to cents, so their sum can differ from the total by 200 x 0.005.
        totalOk: Math.abs(Number(total) - Number(shown)) <= 1.001 };
      }, button);
      if (!m.totalOk || m.changed === 0) throw new Error(`${app} ${name}: total wrong or nothing changed`);
      if (i >= 5) { commit.push(m.commit); frame.push(m.frame); }
    }
    result[name] = `${round(median(commit))} / ${round(median(frame))}`;
  }
  const cdp = await page.context().newCDPSession(page);
  for (let k = 0; k < 3; k++) await cdp.send("HeapProfiler.collectGarbage");
  result.heapKB = round((await cdp.send("Runtime.getHeapUsage")).usedSize / 1024, 0);
  result.errors = errors;
  out.apps[app] = result;
  await page.close();
}
await browser.close();
server.close();
mkdirSync(join(here, "results"), { recursive: true });
writeFileSync(args.out ?? join(here, "results", `dashboard-${out.date.slice(0, 10)}.json`), JSON.stringify(out, null, 2));
console.log(`chromium ${out.chromium}; ${N} ticks each; median ms commit / frame`);
console.log("".padEnd(10), ...APPS.map((a) => a.replace("dashboard-", "").padStart(14)));
for (const k of Object.keys(out.apps[APPS[0]])) console.log(k.padEnd(10), ...APPS.map((a) => String(out.apps[a][k]).padStart(14)));
