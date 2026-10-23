// Runs every app in Chromium and measures, from outside the frameworks and with the same code:
//   ops     click a button, let queued microtasks run (every app has then updated the DOM): "commit";
//           then the next animation frame plus a macrotask: "frame" (style, layout, paint).
//   memory  JS heap after garbage collection: empty, 1k rows, 10k rows.
//   load    navigation start to the app being ready, over fresh contexts.
// Usage: node run.mjs [--iterations 15] [--rounds 2] [--apps state,solid] [--out results/<date>.json]
import { createRequire } from "node:module";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { serve } from "./serve.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const { chromium } = createRequire(join(here, "..", "..", "package.json"))("@playwright/test");
const args = Object.fromEntries(process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith("--") ? [...acc, [a.slice(2), all[i + 1]]] : acc), []));
const ITERATIONS = Number(args.iterations ?? 15);
const ROUNDS = Number(args.rounds ?? 2);
const LOADS = 10;
const WARMUP = 3;
const PORT = 44200;
const APPS = (args.apps ?? "vanilla,state,view,solid,solid-h,remix").split(",");

const OPS = [
  { name: "create 1k", prep: ["#clear"], click: "#create", rows: 1000 },
  { name: "replace 1k", prep: ["#create"], click: "#create", rows: 1000 },
  { name: "update every 10th", prep: ["#create"], click: "#update", rows: 1000, label: [0, " !!!"] },
  { name: "select row", prep: ["#create"], click: "tbody tr:nth-child(2) a.select", rows: 1000, selected: 1 },
  { name: "swap rows", prep: ["#create"], click: "#swap", rows: 1000, swapped: true },
  { name: "remove row", prep: ["#create"], click: "tbody tr:nth-child(4) a.remove", rows: 999 },
  { name: "create 10k", prep: ["#clear"], click: "#create10k", rows: 10000, iterations: 5 },
  { name: "append 1k", prep: ["#create"], click: "#append", rows: 2000 },
  { name: "clear 1k", prep: ["#create"], click: "#clear", rows: 0 },
  { name: "patch 100 of 1k", prep: ["#create"], click: "#patch100", rows: 1000, label: [0, " *"] },
  { name: "patch 1 of 10k", prep: ["#create10k"], click: "#patch1", rows: 10000, label: [5000, " *"], iterations: 5 },
  { name: "patch 100 of 10k", prep: ["#create10k"], click: "#patch100", rows: 10000, label: [0, " *"], iterations: 5 },
];

async function clickAndMeasure(page, selector) {
  return page.evaluate(async (sel) => {
    const el = document.querySelector(sel);
    if (!el) throw new Error(`missing ${sel}`);
    const t0 = performance.now();
    el.click();
    // Every app has updated the DOM by the time the microtasks queued during the click have run
    // (Solid and plain JavaScript in the click, Remix and state() in a microtask); check() fails
    // an app that has not.
    await new Promise((r) => queueMicrotask(() => queueMicrotask(r)));
    const t1 = performance.now();
    const rowsAtCommit = document.querySelectorAll("tbody tr").length;
    await new Promise((r) => requestAnimationFrame(() => setTimeout(r, 0)));
    return { commit: t1 - t0, frame: performance.now() - t0, rowsAtCommit };
  }, selector);
}

/** Checks the DOM after an op, so an app that skips work fails instead of looking fast. */
async function check(page, op, m) {
  const s = await page.evaluate(({ label, selected }) => {
    const rows = document.querySelectorAll("tbody tr");
    const text = (i) => rows[i]?.querySelector("a.select")?.textContent ?? null;
    return {
      rows: rows.length,
      label: label ? text(label[0]) : null,
      danger: Array.from(rows, (r, i) => (r.classList.contains("danger") ? i : -1)).filter((i) => i >= 0),
      ids: [rows[1]?.firstElementChild?.textContent, rows[998]?.firstElementChild?.textContent].map(Number),
    };
  }, op);
  const problems = [];
  if (s.rows !== op.rows || m.rowsAtCommit !== op.rows) problems.push(`rows ${s.rows} (at commit ${m.rowsAtCommit}) != ${op.rows}`);
  if (op.label && !s.label?.endsWith(op.label[1])) problems.push(`label ${s.label}`);
  if (op.selected !== undefined && (s.danger.length !== 1 || s.danger[0] !== op.selected)) problems.push(`selected ${s.danger}`);
  if (op.swapped && !(s.ids[0] > s.ids[1])) problems.push(`not swapped ${s.ids}`);
  if (problems.length) throw new Error(`${op.name}: ${problems.join(", ")}`);
}

const median = (xs) => { const s = [...xs].sort((a, b) => a - b); const m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
const round = (x, d = 2) => Math.round(x * 10 ** d) / 10 ** d;
const url = (app) => `http://127.0.0.1:${PORT}/${app}/`;

async function ops(browser, app) {
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto(url(app));
  await page.waitForFunction(() => document.documentElement.dataset.ready);
  const results = {};
  for (const op of OPS) {
    const n = op.iterations ?? ITERATIONS;
    const commit = [];
    const frame = [];
    for (let i = 0; i < WARMUP + n; i++) {
      for (const p of op.prep) await clickAndMeasure(page, p);
      const m = await clickAndMeasure(page, op.click);
      await check(page, op, m);
      if (i >= WARMUP) { commit.push(m.commit); frame.push(m.frame); }
    }
    results[op.name] = { n, commitMs: round(median(commit)), frameMs: round(median(frame)) };
  }
  await page.close();
  return { results, errors };
}

async function memory(browser, app) {
  const page = await browser.newPage();
  await page.goto(url(app));
  await page.waitForFunction(() => document.documentElement.dataset.ready);
  const cdp = await page.context().newCDPSession(page);
  const heap = async () => {
    for (let i = 0; i < 3; i++) await cdp.send("HeapProfiler.collectGarbage");
    return (await cdp.send("Runtime.getHeapUsage")).usedSize;
  };
  const out = { readyKB: round((await heap()) / 1024, 0) };
  await clickAndMeasure(page, "#create");
  out.rows1kKB = round((await heap()) / 1024, 0);
  await clickAndMeasure(page, "#create10k");
  out.rows10kKB = round((await heap()) / 1024, 0);
  out.perRowB = round(((out.rows10kKB - out.readyKB) * 1024) / 10000, 0);
  await clickAndMeasure(page, "#clear");
  out.clearedKB = round((await heap()) / 1024, 0);
  await page.close();
  return out;
}

async function load(browser, app) {
  const ready = [];
  for (let i = 0; i < LOADS; i++) {
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.goto(url(app), { waitUntil: "load" });
    await page.waitForFunction(() => document.documentElement.dataset.ready);
    ready.push(await page.evaluate(() => Number(document.documentElement.dataset.ready)));
    await context.close();
  }
  return { readyMs: round(median(ready)) };
}

const server = await serve(PORT);
const browser = await chromium.launch({ headless: true });
const sizes = JSON.parse(readFileSync(join(here, "out", "sizes.json"), "utf8"));
const out = { date: new Date().toISOString(), chromium: browser.version(), iterations: ITERATIONS, rounds: [] };
for (let round_ = 0; round_ < ROUNDS; round_++) {
  const apps = round_ % 2 ? [...APPS].reverse() : APPS;
  const result = {};
  for (const app of apps) {
    console.error(`round ${round_ + 1}: ${app}`);
    result[app] = { size: sizes[app], load: await load(browser, app), memory: await memory(browser, app), ...(await ops(browser, app)) };
  }
  out.rounds.push(result);
}
await browser.close();
server.close();

mkdirSync(join(here, "results"), { recursive: true });
const file = args.out ?? join(here, "results", `${out.date.slice(0, 10)}.json`);
writeFileSync(file, JSON.stringify(out, null, 2));

const pad = (s, n) => String(s).padStart(n);
console.log(`chromium ${out.chromium}; median ms, commit / frame; rounds: ${ROUNDS} (values joined with |)`);
console.log("op".padEnd(18), ...APPS.map((a) => pad(a, 22)));
for (const op of OPS) {
  console.log(op.name.padEnd(18), ...APPS.map((a) => pad(out.rounds.map((r) => `${r[a].results[op.name].commitMs}/${r[a].results[op.name].frameMs}`).join(" | "), 22)));
}
const row = (label, f) => console.log(label.padEnd(18), ...APPS.map((a) => pad(out.rounds.map((r) => f(r[a])).join(" | "), 22)));
row("js gzip B", (r) => r.size.gzip);
row("ready ms", (r) => r.load.readyMs);
row("heap ready KB", (r) => r.memory.readyKB);
row("heap 1k KB", (r) => r.memory.rows1kKB);
row("heap 10k KB", (r) => r.memory.rows10kKB);
row("bytes/row", (r) => r.memory.perRowB);
row("heap cleared KB", (r) => r.memory.clearedKB);
row("page errors", (r) => r.errors.length);
