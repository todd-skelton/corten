// Writes results/<round>/summary.json and prints the tables in README form.
// Usage: node summarize.mjs <round>
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { apps } from "./apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const round = process.argv[2] ?? "r1";
const dir = join(here, "results", round);
const read = (file) => (existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : null);
const pct = (xs, p) => {
  const s = xs.filter(Number.isFinite).sort((a, b) => a - b);
  return s.length ? +s[Math.min(s.length - 1, Math.floor(p * (s.length - 1) + 0.5))].toFixed(0) : null;
};
const summary = { ssr: {}, coldStart: read(join(dir, "cold-start.json")), bytes: read(join(dir, "bytes.json")), instructions: {}, browser: {} };

// "fixture" and "api" are the item page; "fixture-json" and "api-json" the item as JSON.
for (const [key, source, suffix] of [["fixture", "fixture", ""], ["api", "api", ""], ["fixture-json", "fixture", "-json"], ["api-json", "api", "-json"]]) {
  summary.ssr[key] = {};
  for (const name of Object.keys(apps)) {
    const file = `${name}${suffix}`;
    const entry = { startup: read(join(dir, source, `${file}-startup.json`)) };
    for (const phase of ["fixed", "sat64", "sat256"]) {
      const k6 = read(join(dir, source, `${file}-${phase}.json`));
      if (!k6) continue;
      const d = k6.metrics.http_req_duration;
      const stats = read(join(dir, source, `${file}-${phase}-stats.json`))?.[`fsw-${name}`];
      const offered = k6.metrics.http_reqs.count + (k6.metrics.dropped_iterations?.count ?? 0);
      entry[phase] = {
        rps: +k6.metrics.http_reqs.rate.toFixed(0),
        // The fixed rate's requests that were never sent because every client was waiting.
        unsentPct: +((100 * (k6.metrics.dropped_iterations?.count ?? 0)) / offered).toFixed(2),
        p50: +d["p(50)"].toFixed(1),
        p95: +d["p(95)"].toFixed(1),
        p99: +d["p(99)"].toFixed(1),
        failedPct: +(100 * k6.metrics.http_req_failed.value).toFixed(2),
        cpuPct: stats?.cpuAvgPct ?? null,
        memMiB: stats?.memAvgMiB ?? null,
        memMaxMiB: stats?.memMaxMiB ?? null,
      };
    }
    // Apps run with their usual settings, not tuned to fit one CPU and 1 GiB: one that did not
    // start, or could not serve the fixed rate (more than 5% unsent or 1% failed), failed.
    if (entry.startup?.failed) entry.failed = entry.startup.failed;
    else if (entry.fixed && entry.fixed.unsentPct > 5) entry.failed = `served ${entry.fixed.rps}/s of the fixed rate, ${entry.fixed.unsentPct}% never sent`;
    else if (entry.fixed && entry.fixed.failedPct > 1) entry.failed = `${entry.fixed.failedPct}% of requests at the fixed rate failed`;
    if (entry.fixed || entry.failed) summary.ssr[key][name] = entry;
  }
}

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
for (const [key, runs] of Object.entries(read(join(dir, "instructions.json")) ?? {})) {
  if (!Array.isArray(runs)) { summary.instructions[key] = { failed: runs.failed }; continue; }
  summary.instructions[key] = { user: median(runs.map((r) => r.userPerRequest)), kernel: median(runs.map((r) => r.kernelPerRequest)), runs: runs.length, failed: runs.reduce((n, r) => n + r.failed, 0) };
}

const rows = read(join(dir, "browser.json")) ?? [];
for (const row of rows) {
  const key = `${row.profile}/${row.app}`;
  (summary.browser[key] ??= []).push(row);
}
for (const [key, list] of Object.entries(summary.browser)) {
  const m = (k) => ({ med: pct(list.map((r) => r[k]), 0.5), p90: pct(list.map((r) => r[k]), 0.9) });
  summary.browser[key] = { n: list.length, allWorked: list.every((r) => r.sortWorked !== false), ttfb: m("ttfb"), fcp: m("fcp"), lcp: m("lcp"), interactive: m("hydratedAt"), tbt: m("tbtToHydration"), action: m("actionMs"), clientNav: m("clientNavMs"), heapMB: { med: +(pct(list.map((r) => r.jsHeapUsedMB * 10), 0.5) / 10).toFixed(1) } };
}
writeFileSync(join(dir, "summary.json"), JSON.stringify(summary, null, 1));

const line = (cells) => `| ${cells.join(" | ")} |`;
for (const source of Object.keys(summary.ssr)) {
  console.log(`\nSSR ${source}`);
  console.log(line(["App", "fixed rate: achieved rps, p50/p99 ms, CPU", "64 VUs: rps, p50/p99", "256 VUs: rps, p99, failed", "memory under load"]));
  for (const [name, e] of Object.entries(summary.ssr[source])) {
    const f = e.fixed, s = e.sat64, b = e.sat256;
    if (e.failed) console.log(line([name, `FAILED: ${e.failed}`, "", "", ""]));
    if (!f) continue;
    console.log(line([name, `${f.rps}, ${f.p50}/${f.p99}, ${f.cpuPct}%`, `${s?.rps}, ${s?.p50}/${s?.p99}`, `${b?.rps}, ${b?.p99}, ${b?.failedPct}%`, `${s?.memMiB}-${b?.memMaxMiB} MiB`]));
  }
}
if (Object.keys(summary.instructions).length) {
  console.log("\nInstructions per request (median, user / kernel)");
  for (const [key, c] of Object.entries(summary.instructions)) if (c.failed) console.log(line([key, `FAILED: ${c.failed}`])); else console.log(line([key, `${c.user.toLocaleString("en-US")} / ${c.kernel.toLocaleString("en-US")}`, `${c.runs} runs`, `${c.failed} failed`]));
}
if (summary.coldStart) {
  console.log("\nCold start");
  for (const [name, c] of Object.entries(summary.coldStart)) console.log(line([name, `${c.firstOkMs.median} ms (min ${c.firstOkMs.min}, max ${c.firstOkMs.max})`, c.memAfterFirstRequest, `${c.imageMB} MB image`]));
}
if (summary.bytes) {
  console.log("\nBytes (raw / gzip)");
  const kb = (x) => (x / 1024).toFixed(1);
  for (const [name, b] of Object.entries(summary.bytes)) console.log(line([name, `html ${kb(b.html.raw)}/${kb(b.html.gzip)}`, `js ${kb(b.js.raw)}/${kb(b.js.gzip)} (${b.js.files})`, `wasm ${kb(b.wasm.raw)}/${kb(b.wasm.gzip)}`, `css ${kb(b.css.raw)}/${kb(b.css.gzip)}`]));
}
console.log("\nBrowser (median / p90 ms)");
for (const [key, b] of Object.entries(summary.browser)) console.log(line([key, `interactive ${b.interactive.med}/${b.interactive.p90}`, `LCP ${b.lcp.med}`, `TBT ${b.tbt.med}`, `action ${b.action.med}`, `nav ${b.clientNav.med}/${b.clientNav.p90}`, `heap ${b.heapMB.med} MB`, b.allWorked ? "ok" : "FAILED"]));
