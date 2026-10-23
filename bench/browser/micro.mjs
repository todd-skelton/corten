// Many repetitions of one operation, for differences under a millisecond that run.mjs's 5 to 15
// samples cannot resolve. Usage: node micro.mjs <app> <prep selector|-> <click selector> [n]
//   node micro.mjs state "#create10k" "#patch1" 200
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { serve } from "./serve.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const { chromium } = createRequire(join(here, "..", "..", "package.json"))("@playwright/test");
const [app, prep, click, n = "200"] = process.argv.slice(2);
const server = await serve(44210);
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();
await page.goto(`http://127.0.0.1:44210/${app}/`);
await page.waitForFunction(() => document.documentElement.dataset.ready);
const r = await page.evaluate(async ({ prep, click, n }) => {
  const tick = () => new Promise((r) => setTimeout(r, 0));
  if (prep !== "-") { document.querySelector(prep).click(); await tick(); }
  const times = [];
  for (let i = 0; i < 20 + n; i++) {
    const el = document.querySelector(click);
    const t0 = performance.now();
    el.click();
    await new Promise((r) => queueMicrotask(() => queueMicrotask(r)));
    if (i >= 20) times.push(performance.now() - t0);
    await tick();
  }
  times.sort((a, b) => a - b);
  const q = (p) => times[Math.floor(p * (times.length - 1))];
  return { n, p25: q(0.25), median: q(0.5), p75: q(0.75), mean: times.reduce((a, b) => a + b, 0) / times.length };
}, { prep, click, n: Number(n) });
console.log(app.padEnd(18), click.padEnd(10), Object.entries(r).map(([k, v]) => k + " " + (k === "n" ? v : v.toFixed(3))).join("  "));
await browser.close();
server.close();
