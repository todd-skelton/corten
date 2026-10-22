// Prints results/<round> as the README's table and writes results/<round>/summary.json.
// Usage: node summarize.mjs <round>
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { live } from "./apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const round = process.argv[2] ?? "r1";
const dir = join(here, "results", round);
const rows = readdirSync(dir)
  .filter((f) => /^[a-z-]+-\d+\.json$/.test(f))
  .map((f) => JSON.parse(readFileSync(join(dir, f), "utf8")))
  .sort((a, b) => a.clients - b.clients || Object.keys(live).indexOf(a.app) - Object.keys(live).indexOf(b.app));
for (const r of rows) r.result = verdict(r);
writeFileSync(join(dir, "summary.json"), JSON.stringify(rows, null, 1));
/**
 * Apps run with their usual settings, not tuned to fit one CPU and 1 GiB. An app that refused
 * clients, lost messages, left publishes unanswered or stopped failed; one that delivered every
 * message, in order, but more slowly than it was asked to fell behind.
 */
export function verdict(r) {
  if (r.startFailed) return "failed: did not start";
  const why = [];
  if ((r.connected ?? 0) < r.clients) why.push(`${r.clients - (r.connected ?? 0)} of ${r.clients} clients not connected`);
  if (r.oomKilled) why.push("killed out of memory");
  if (r.publishErrors) why.push(`${r.publishErrors} of ${r.published} publishes unanswered`);
  if ((r.delivered ?? 0) < (r.expected ?? 0) || r.outOfOrder || r.duplicates) why.push("messages lost or out of order");
  if (why.length) return `failed: ${why.join(", ")}`;
  const planned = r.planned ?? r.published;
  return r.published === planned && r.publishSeconds <= r.seconds * 1.05 ? "kept up" : `fell behind: ${r.published} of ${planned} in ${r.publishSeconds} s`;
}
const pct = (n, d) => (d ? `${((100 * n) / d).toFixed(n === d ? 0 : 2)}%` : "-");
for (const clients of [...new Set(rows.map((r) => r.clients))]) {
  console.log(`\n${clients} clients`);
  console.log("| App | Transport | Result | Connected | Delivered | In order | Delivered per second | Latency p50 / p99 / max ms | CPU | Memory idle / connected / max MiB | KiB per client |");
  console.log("|---|---|---|---|---|---|---|---|---|---|---|");
  for (const r of rows.filter((x) => x.clients === clients)) {
    const l = r.latencyMs ?? {};
    const inOrder = r.delivered ? pct(r.delivered - (r.outOfOrder ?? 0) - (r.duplicates ?? 0), r.delivered) : "-";
    const planned = r.planned ?? r.published;
    // A publish that timed out (10 s) may never have reached the clients; say how the rest did.
    const accepted = (r.published ?? 0) - (r.publishErrors ?? 0);
    const ofAccepted = r.publishErrors && accepted > 0 ? `, ${pct(Math.min(r.delivered, r.connected * accepted), r.connected * accepted)} of the ${accepted} accepted` : "";
    const extra = [r.dropped ? `${r.dropped} dropped` : "", r.resyncs ? `${r.resyncs} resyncs` : "", r.oomKilled ? "OOM killed" : "", r.publishErrors ? `${r.publishErrors} of ${r.published} publishes timed out${ofAccepted}` : ""].filter(Boolean).join(", ");
    const perSecond = r.delivered && r.publishSeconds ? Math.round(r.delivered / r.publishSeconds).toLocaleString("en-US") : "-";
    console.log(`| ${r.app} | ${r.transport} | ${r.result} | ${r.connected ?? 0} of ${r.clients} | ${pct(r.delivered ?? 0, r.expected ?? 0)}${extra ? ` (${extra})` : ""} | ${inOrder} | ${perSecond} | ${l.p50 ?? "-"} / ${l.p99 ?? "-"} / ${l.max ?? "-"} | ${r.cpuPct ?? "-"}% | ${r.idleMiB ?? "-"} / ${r.connectedMiB ?? "-"} / ${r.memMaxMiB ?? "-"} | ${r.kibPerClient ?? "-"} |`);
  }
}
