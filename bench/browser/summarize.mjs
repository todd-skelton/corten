// Prints the README's client-side table from results/<round>/ (ops.json, ssr.json, dashboard.json)
// and writes results/<round>/summary.json. Each cell is the median; with two rounds of the
// operations, the cell is the higher of the two rounds' medians (the slower round), or both as a
// range when they differ by more than half and at least a millisecond.
// Usage: node summarize.mjs <round>
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const round = process.argv[2];
if (!round) throw new Error("usage: node summarize.mjs <round>");
const dir = join(here, "results", round);
const read = (name) => JSON.parse(readFileSync(join(dir, name), "utf8"));
const ops = read("ops.json");
const ssr = read("ssr.json");
const dash = read("dashboard.json");

const columns = [
  ["Corten (views)", "view"],
  ["Solid 1.9", "solid"],
  ["Svelte 5", "svelte"],
  ["Vue 3.5", "vue"],
  ["React 19", "react"],
  ["Remix 3", "remix"],
  ["Plain JS (floor)", "vanilla"],
];
const worst = (app, f) => Math.max(...ops.rounds.map((r) => f(r[app])));
const fmt = (x, d = 1) => (x >= 100 ? Math.round(x).toLocaleString("en-US") : x.toFixed(d));
/** The rounds' medians of an operation: the slower one, or the range when the rounds disagree. */
const op = (name) => (app) => {
  const all = ops.rounds.map((r) => r[app].results[name].commitMs);
  const lo = Math.min(...all);
  const hi = Math.max(...all);
  return hi > lo * 1.5 && hi - lo >= 1 ? `${fmt(lo)}-${fmt(hi)}` : fmt(hi);
};
const rows = [
  ["Take over 1,000 server-rendered rows", (a) => `${fmt(ssr.apps[`${a}-ssr`].takeoverMs)} ms`],
  ["First click on a taken-over row", (a) => `${fmt(ssr.apps[`${a}-ssr`].firstSelectMs)} ms`],
  ["Change 1 row of 10,000", (a) => `${op("patch 1 of 10k")(a)} ms`],
  ["Change 100 rows of 10,000", (a) => `${op("patch 100 of 10k")(a)} ms`],
  ["Update every 10th of 1,000", (a) => `${op("update every 10th")(a)} ms`],
  ["Select a row", (a) => `${op("select row")(a)} ms`],
  ["Swap two rows", (a) => `${op("swap rows")(a)} ms`],
  ["Create 1,000 rows", (a) => `${op("create 1k")(a)} ms`],
  ["Create 10,000 rows", (a) => `${op("create 10k")(a)} ms`],
  ["Dashboard: 200 prices tick", (a) => `${fmt(Number(dash.apps[`dashboard-${a}`]["tick 200"].split(" / ")[0]))} ms`],
  ["Memory per row", (a) => `${fmt(worst(a, (r) => r.memory.perRowB), 0)} B`],
  ["JavaScript, gzip", (a) => `${(ops.rounds[0][a].size.gzip / 1024).toFixed(1)} KB`],
];
const table = [
  `| | ${columns.map(([label]) => (label.startsWith("Corten") ? `**${label}**` : label)).join(" | ")} |`,
  `|---|${columns.map(() => "---").join("|")}|`,
  ...rows.map(([label, f]) => `| ${label} | ${columns.map(([, app], i) => (i === 0 ? `**${f(app)}**` : f(app))).join(" | ")} |`),
];
const summary = { round, chromium: ops.chromium, date: ops.date, table: table.join("\n") };
writeFileSync(join(dir, "summary.json"), JSON.stringify(summary, null, 1));
console.log(`Chromium ${ops.chromium}, ${ops.date.slice(0, 10)}`);
console.log(summary.table);
