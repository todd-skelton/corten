// Instructions per request: each app's container (items from memory), warmed up, then exactly
// ITERATIONS requests while `perf stat` counts the instructions the container's processes retire,
// user and kernel separately. Instruction counts barely move with other load on the machine, so
// they compare frameworks where wall-clock numbers are noisy; they do not see cache misses, I/O
// or how well work overlaps, so they complement the load tables rather than replace them.
// Needs Linux with hardware counters (`perf`, root, cgroup v2): a bench host, not Docker Desktop.
// Usage: node count.mjs <round> [repeats=3] [apps=all] [targets=page,json]
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { apps as catalog, k6Net, k6User, loadCpus, network, runArgs } from "./apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const [round = "r1", repeats = "3", appsArg = Object.keys(catalog).join(","), targetsArg = "page,json"] = process.argv.slice(2);
const ITERATIONS = 5000;
const docker = (...args) => spawnSync("docker", args, { encoding: "utf8" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const dir = join(here, "results", round);
mkdirSync(dir, { recursive: true });

function k6(base, target, envs, summary) {
  const args = ["run", "--rm", "--network", network, ...k6Net, ...loadCpus, ...k6User, "-v", `${here}:/bench`, "-e", `BASE=${base}`, "-e", `TARGET=${target}`];
  for (const [k, v] of Object.entries(envs)) args.push("-e", `${k}=${v}`);
  args.push("grafana/k6:latest", "run", "--quiet", "--summary-export", `/bench/${summary}`, "/bench/ssr-load.js");
  const r = docker(...args);
  if (r.status !== 0) throw new Error(r.stderr);
}

/** Counts instructions in the container's cgroup until `work` resolves. */
async function counted(container, work) {
  const id = docker("inspect", "-f", "{{.Id}}", container).stdout.trim();
  const cgroup = `system.slice/docker-${id}.scope`;
  const out = join("/tmp", `perf-${id}.csv`);
  const perf = spawn("perf", ["stat", "-a", "-x", ",", "-o", out, "-e", "instructions:u,instructions:k", "-G", `${cgroup},${cgroup}`]);
  await sleep(500);
  await work();
  perf.kill("SIGINT");
  await new Promise((r) => perf.on("exit", r));
  const counts = {};
  for (const line of readFileSync(out, "utf8").split("\n")) {
    const [value, , event] = line.split(",");
    if (event?.startsWith("instructions:")) counts[event.slice(-1)] = Number(value);
  }
  return counts;
}

// Merged into the round's earlier counts, so one app can be counted again on its own.
const file = join(dir, "instructions.json");
const results = existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : {};
for (const name of appsArg.split(",")) {
  for (const target of targetsArg.split(",")) {
    docker("rm", "-f", `fsw-${name}`);
    const run = docker(...runArgs(name, catalog[name], "fixture", join(here, "fixtures")));
    if (run.status !== 0) throw new Error(`${name}: ${run.stderr}`);
    const path = target === "json" ? "/api/items/item_1" : "/items/item_1";
    let up = false;
    for (let i = 0; i < 2400 && !up; i += 1) {
      try { up = (await fetch(`http://localhost:${catalog[name].port}${path}`)).ok; } catch {}
      if (!up) await sleep(25);
    }
    // Not tuned to fit: an app that does not start with its usual settings is recorded as failed.
    if (!up) {
      console.log(name, target, "failed: never answered");
      results[`${name}/${target}`] = { failed: `never answered ${path} within a minute` };
      docker("rm", "-f", `fsw-${name}`);
      continue;
    }
    const base = `http://fsw-${name}:8080`;
    // Warm up: JIT compilers, connection pools and lazily created threads settle first.
    k6(base, target, { MODE: "closed", VUS: "16", DURATION: "30s" }, `results/${round}/count-warmup.json`);
    const runs = [];
    for (let r = 0; r < Number(repeats); r += 1) {
      const summary = `results/${round}/count-${name}-${target}-${r}.json`;
      const counts = await counted(`fsw-${name}`, async () => k6(base, target, { MODE: "iterations", VUS: "4", ITERATIONS: String(ITERATIONS) }, summary));
      const k = JSON.parse(readFileSync(join(here, summary), "utf8"));
      const requests = k.metrics.http_reqs.count;
      const failed = k.metrics.http_req_failed.passes ?? 0;
      runs.push({ requests, failed, userPerRequest: Math.round(counts.u / requests), kernelPerRequest: Math.round(counts.k / requests) });
      console.log(name, target, JSON.stringify(runs.at(-1)));
    }
    docker("rm", "-f", `fsw-${name}`);
    results[`${name}/${target}`] = runs;
  }
}
writeFileSync(file, JSON.stringify(results, null, 1));
console.log("done");
