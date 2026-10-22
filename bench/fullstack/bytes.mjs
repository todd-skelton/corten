// What each app ships for one item page, measured the same way for all: every script, WASM and
// stylesheet the page loads, fetched uncompressed and gzipped here (level 6), so a server that
// does not compress is not penalized and one that does is not flattered.
// Usage: node bytes.mjs <round> [apps=all]   (apps running on their catalog ports)
import { chromium } from "@playwright/test";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { apps } from "./apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const round = process.argv[2] ?? "r1";
const only = process.argv[3]?.split(",");
const browser = await chromium.launch({ headless: true });
const out = {};
try {
  for (const [name, app] of Object.entries(apps).filter(([name]) => !only || only.includes(name))) {
    const page = await browser.newPage();
    const urls = new Set();
    page.on("request", (r) => ["script", "stylesheet", "fetch", "other"].includes(r.resourceType()) && urls.add(r.url()));
    const base = `http://localhost:${app.port}`;
    await page.goto(`${base}/items/item_17`, { waitUntil: "networkidle" });
    const html = await (await fetch(`${base}/items/item_17`, { headers: { "accept-encoding": "identity" } })).arrayBuffer();
    const totals = { html: { raw: html.byteLength, gzip: gzipSync(Buffer.from(html)).length }, js: { raw: 0, gzip: 0, files: 0 }, wasm: { raw: 0, gzip: 0, files: 0 }, css: { raw: 0, gzip: 0, files: 0 } };
    for (const url of urls) {
      const kind = /\.wasm(\?|$)/.test(url) ? "wasm" : /\.css(\?|$)/.test(url) ? "css" : /\.m?js(\?|$)/.test(url) ? "js" : null;
      if (!kind || !url.startsWith(base)) continue;
      const body = Buffer.from(await (await fetch(url, { headers: { "accept-encoding": "identity" } })).arrayBuffer());
      totals[kind].raw += body.length;
      totals[kind].gzip += gzipSync(body).length;
      totals[kind].files += 1;
    }
    out[name] = totals;
    console.log(name, JSON.stringify(totals));
    await page.close();
  }
} finally {
  await browser.close();
}
mkdirSync(join(here, "results", round), { recursive: true });
writeFileSync(join(here, "results", round, "bytes.json"), JSON.stringify(out, null, 1));
