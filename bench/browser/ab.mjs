// Interleaved comparison: the apps take turns, a block of clicks each, in one browser, so machine
// drift hits all of them alike. Reports each app's mean over every block.
// Usage: node ab.mjs <prep selector|-> <click selector> <app> <app> [...] [--blocks 20] [--per 25]
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { serve } from "./serve.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const { chromium } = createRequire(join(here, "..", "..", "package.json"))("@playwright/test");
const argv = process.argv.slice(2);
const opt = (name, d) => { const i = argv.indexOf("--" + name); return i < 0 ? d : Number(argv.splice(i, 2)[1]); };
const BLOCKS = opt("blocks", 20);
const PER = opt("per", 25);
const [prep, click, ...apps] = argv;
const server = await serve(44212);
const browser = await chromium.launch({ headless: true });
const pages = [];
for (const app of apps) {
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:44212/${app}/`);
  await page.waitForFunction(() => document.documentElement.dataset.ready);
  if (prep !== "-") await page.evaluate(async (p) => { document.querySelector(p).click(); await new Promise((r) => setTimeout(r, 0)); }, prep);
  pages.push(page);
}
const sums = apps.map(() => []);
for (let b = 0; b < BLOCKS + 1; b++) {
  for (let a = 0; a < apps.length; a++) {
    const k = b % 2 ? apps.length - 1 - a : a;
    await pages[k].bringToFront();
    const t = await pages[k].evaluate(async ({ click, per }) => {
      let total = 0;
      for (let i = 0; i < per; i++) {
        const el = document.querySelector(click);
        const t0 = performance.now();
        el.click();
        await new Promise((r) => queueMicrotask(() => queueMicrotask(r)));
        total += performance.now() - t0;
        await new Promise((r) => setTimeout(r, 0));
      }
      return total / per;
    }, { click, per: PER });
    if (b > 0) sums[k].push(t);
  }
}
const mean = (xs) => xs.reduce((x, y) => x + y, 0) / xs.length;
const sd = (xs) => Math.sqrt(mean(xs.map((x) => (x - mean(xs)) ** 2)) / xs.length);
console.log(`${click} after ${prep}: ${BLOCKS} blocks of ${PER}, mean ms (± standard error)`);
apps.forEach((app, i) => console.log("  " + app.padEnd(24), mean(sums[i]).toFixed(3), "±", sd(sums[i]).toFixed(3)));
await browser.close();
server.close();
