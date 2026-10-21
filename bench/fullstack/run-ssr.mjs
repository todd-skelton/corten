// Server-side rendering under load, one app at a time on one CPU and 1 GiB.
// Usage: node run-ssr.mjs <round> <api|fixture> [fixedRate=150] [apps=all]
//   api:     every request fetches the item from the Rust item API (Redis-warm), like production.
//   fixture: items come from memory (fixtures/items.json), so only the web framework is measured.
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { apps as catalog, network, runArgs } from "./apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const [round = "r1", source = "api", fixedRate = "150", appsArg = Object.keys(catalog).join(",")] = process.argv.slice(2);
const results = join(here, "results", round, source);
mkdirSync(results, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const docker = (...args) => spawnSync("docker", args, { encoding: "utf8" });
const log = (msg) => console.log(new Date().toISOString(), msg);

function k6(name, base, envs) {
  const args = ["run", "--rm", "--network", network, "--cpus", "4", "-v", `${here}:/bench`, "-e", `BASE=${base}`];
  for (const [k, v] of Object.entries(envs)) args.push("-e", `${k}=${v}`);
  args.push("grafana/k6:latest", "run", "--quiet", "--summary-export", `/bench/results/${round}/${source}/${name}.json`, "/bench/ssr-load.js");
  const r = docker(...args);
  writeFileSync(join(results, `${name}.log`), (r.stdout ?? "") + (r.stderr ?? ""));
}

for (const name of appsArg.split(",")) {
  const app = catalog[name];
  for (const other of Object.keys(catalog)) docker("rm", "-f", `fsw-${other}`);
  await sleep(3000);
  const started = Date.now();
  const run = docker(...runArgs(name, app, source, join(here, "fixtures")));
  if (run.status !== 0) throw new Error(`${name}: ${run.stderr}`);
  let ready = null;
  for (let i = 0; i < 2400 && ready === null; i += 1) {
    try {
      const r = await fetch(`http://localhost:${app.port}/items/item_1`);
      if (r.ok) ready = Date.now() - started;
    } catch {}
    if (ready === null) await sleep(25);
  }
  if (ready === null) throw new Error(`${name} never answered`);
  await sleep(5000);
  const idle = docker("stats", "--no-stream", "--format", "{{.MemUsage}}", `fsw-${name}`).stdout.trim();
  writeFileSync(join(results, `${name}-startup.json`), JSON.stringify({ firstOkMsFromDockerRun: ready, idleMem: idle }, null, 1));
  const base = `http://fsw-${name}:8080`;
  log(`${name} ${source} warmup`);
  k6(`${name}-warmup`, base, { MODE: "closed", VUS: "32", DURATION: "20s" });
  for (const [phase, envs] of [
    ["fixed", { MODE: "open", RATE: fixedRate, DURATION: "60s" }],
    ["sat64", { MODE: "closed", VUS: "64", DURATION: "60s" }],
    ["sat256", { MODE: "closed", VUS: "256", DURATION: "60s" }],
  ]) {
    log(`${name} ${source} ${phase}`);
    const sampled = [`fsw-${name}`, ...(source === "api" ? ["rb-rust"] : [])];
    const s = spawn("node", [join(here, "sample-stats.mjs"), join(results, `${name}-${phase}-stats.json`), "62", ...sampled], { stdio: "ignore", windowsHide: true });
    k6(`${name}-${phase}`, base, envs);
    await new Promise((r) => (s.exitCode !== null ? r() : s.on("exit", r)));
  }
  docker("rm", "-f", `fsw-${name}`);
}
log("done");
