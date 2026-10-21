// Streams `docker stats` for the named containers and writes per-container
// CPU% and memory summaries when stopped (SIGINT/SIGTERM or after --seconds).
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";
const [out, seconds, ...names] = process.argv.slice(2);
const samples = Object.fromEntries(names.map((n) => [n, []]));
const child = spawn("docker", ["stats", "--format", "{{json .}}", ...names], { stdio: ["ignore", "pipe", "inherit"] });
let buffer = "";
const toMiB = (text) => { const m = /([\d.]+)\s*([KMG]i?B)/.exec(text); if (!m) return NaN; const f = { KiB: 1 / 1024, KB: 1 / 1024, MiB: 1, MB: 1, GiB: 1024, GB: 1024 }[m[2]]; return Number(m[1]) * f; };
child.stdout.on("data", (chunk) => {
  buffer += chunk.toString().replace(/\x1b\[[0-9;]*[A-Za-z]/g, "");
  let index;
  while ((index = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, index).trim(); buffer = buffer.slice(index + 1);
    if (!line.startsWith("{")) continue;
    const s = JSON.parse(line);
    samples[s.Name]?.push({ t: Date.now(), cpu: parseFloat(s.CPUPerc), mem: toMiB(s.MemUsage.split("/")[0]) });
  }
});
const summarize = (list) => {
  const steady = list.slice(Math.floor(list.length * 0.15), Math.ceil(list.length * 0.95));
  const avg = (xs) => { const ok = xs.filter(Number.isFinite); return ok.reduce((a, b) => a + b, 0) / Math.max(ok.length, 1); };
  return { samples: list.length, cpuAvgPct: +avg(steady.map((x) => x.cpu)).toFixed(1), cpuMaxPct: Math.max(...list.map((x) => x.cpu).filter(Number.isFinite)), memAvgMiB: +avg(steady.map((x) => x.mem)).toFixed(1), memMaxMiB: +Math.max(...list.map((x) => x.mem).filter(Number.isFinite)).toFixed(1) };
};
const finish = () => { child.kill(); writeFileSync(out, JSON.stringify(Object.fromEntries(Object.entries(samples).map(([n, l]) => [n, summarize(l)])), null, 1)); process.exit(0); };
setTimeout(finish, Number(seconds) * 1000);
process.on("SIGINT", finish); process.on("SIGTERM", finish);
