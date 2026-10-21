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
const summary = { ssr: {}, coldStart: read(join(dir, "cold-start.json")), bytes: read(join(dir, "bytes.json")), browser: {} };

for (const source of ["fixture", "api"]) {
  summary.ssr[source] = {};
  for (const name of Object.keys(apps)) {
    const entry = { startup: read(join(dir, source, `${name}-startup.json`)) };
    for (const phase of ["fixed", "sat64", "sat256"]) {
      const k6 = read(join(dir, source, `${name}-${phase}.json`));
      if (!k6) continue;
      const d = k6.metrics.http_req_duration;
      const stats = read(join(dir, source, `${name}-${phase}-stats.json`))?.[`fsw-${name}`];
      entry[phase] = {
        rps: +k6.metrics.http_reqs.rate.toFixed(0),
        p50: +d["p(50)"].toFixed(1),
        p95: +d["p(95)"].toFixed(1),
        p99: +d["p(99)"].toFixed(1),
        failedPct: +(100 * k6.metrics.http_req_failed.value).toFixed(2),
        cpuPct: stats?.cpuAvgPct ?? null,
        memMiB: stats?.memAvgMiB ?? null,
        memMaxMiB: stats?.memMaxMiB ?? null,
      };
    }
    summary.ssr[source][name] = entry;
  }
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
for (const source of ["fixture", "api"]) {
  console.log(`\nSSR ${source}`);
  console.log(line(["App", "fixed rate: achieved rps, p50/p99 ms, CPU", "64 VUs: rps, p50/p99", "256 VUs: rps, p99, failed", "memory under load"]));
  for (const [name, e] of Object.entries(summary.ssr[source])) {
    const f = e.fixed, s = e.sat64, b = e.sat256;
    if (!f) continue;
    console.log(line([name, `${f.rps}, ${f.p50}/${f.p99}, ${f.cpuPct}%`, `${s?.rps}, ${s?.p50}/${s?.p99}`, `${b?.rps}, ${b?.p99}, ${b?.failedPct}%`, `${s?.memMiB}-${b?.memMaxMiB} MiB`]));
  }
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
