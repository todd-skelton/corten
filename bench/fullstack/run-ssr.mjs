// Server rendering under load, one app at a time on one CPU and 1 GiB.
// Usage: node run-ssr.mjs <round> <api|fixture> [fixedRate=150] [apps=all] [page|json]
//   api:     every request fetches the item from the item API, like production.
//   fixture: items come from memory (fixtures/items.json), so only the web framework is measured.
//   page:    the item page (default); json: the same item from the app's GET /api/items/{id}.
//            JSON results are named <app>-json-<phase>.
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { apiContainer, apps as catalog, k6Net, k6User, loadCpusFor, network, runArgs } from "./apps.mjs";
import { cpuUsec } from "./cpu-time.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const [round = "r1", source = "api", fixedRate = "150", appsArg = Object.keys(catalog).join(","), target = "page"] = process.argv.slice(2);
const results = join(here, "results", round, source);
mkdirSync(results, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const docker = (...args) => spawnSync("docker", args, { encoding: "utf8" });
const log = (msg) => console.log(new Date().toISOString(), msg);

function k6(name, base, envs) {
  const args = ["run", "--rm", "--network", network, ...k6Net, ...loadCpusFor(source), ...k6User, "-v", `${here}:/bench`, "-e", `BASE=${base}`, "-e", `TARGET=${target}`];
  for (const [k, v] of Object.entries(envs)) args.push("-e", `${k}=${v}`);
  args.push("grafana/k6:latest", "run", "--quiet", "--summary-export", `/bench/results/${round}/${source}/${name}.json`, "/bench/ssr-load.js");
  const r = docker(...args);
  writeFileSync(join(results, `${name}.log`), (r.stdout ?? "") + (r.stderr ?? ""));
  // One failed phase is reported and the run goes on to the next, so it does not cost the apps after it.
  if (!existsSync(join(results, `${name}.json`))) log(`k6 wrote no summary for ${name}; see ${name}.log`);
}

for (const name of appsArg.split(",")) {
  const app = catalog[name];
  const file = target === "json" ? `${name}-json` : name;
  for (const other of Object.keys(catalog)) docker("rm", "-f", `fsw-${other}`);
  await sleep(3000);
  const started = Date.now();
  const run = docker(...runArgs(name, app, source, join(here, "fixtures")));
  if (run.status !== 0) throw new Error(`${name}: ${run.stderr}`);
  const path = target === "json" ? "/api/items/item_1" : "/items/item_1";
  let ready = null;
  for (let i = 0; i < 2400 && ready === null; i += 1) {
    try {
      const r = await fetch(`http://localhost:${app.port}${path}`);
      if (r.ok) ready = Date.now() - started;
    } catch {}
    if (ready === null) await sleep(25);
  }
  // An app that cannot serve in one CPU and 1 GiB with its usual settings fails the run; it is
  // reported, not tuned, and the next app goes on.
  if (ready === null) {
    const logs = docker("logs", "--tail", "20", `fsw-${name}`);
    const failed = `never answered ${path} within a minute`;
    writeFileSync(join(results, `${file}-startup.json`), JSON.stringify({ failed, logs: (logs.stdout + logs.stderr).trim().split("\n") }, null, 1));
    log(`${file} ${source} failed: ${failed}`);
    docker("rm", "-f", `fsw-${name}`);
    continue;
  }
  await sleep(5000);
  const idle = docker("stats", "--no-stream", "--format", "{{.MemUsage}}", `fsw-${name}`).stdout.trim();
  writeFileSync(join(results, `${file}-startup.json`), JSON.stringify({ firstOkMsFromDockerRun: ready, idleMem: idle }, null, 1));
  const base = `http://fsw-${name}:8080`;
  log(`${file} ${source} warmup`);
  // 30 s: long enough for the JVM's and .NET's tiered JITs to settle, the same for every app.
  k6(`${file}-warmup`, base, { MODE: "closed", VUS: "32", DURATION: "30s" });
  for (const [phase, envs] of [
    ["fixed", { MODE: "open", RATE: fixedRate, DURATION: "60s" }],
    ["sat64", { MODE: "closed", VUS: "64", DURATION: "60s" }],
    ["sat256", { MODE: "closed", VUS: "256", DURATION: "60s" }],
  ]) {
    log(`${file} ${source} ${phase}`);
    const sampled = [`fsw-${name}`, ...(source === "api" ? [apiContainer] : [])];
    // CPU time each container used during the phase, from its cgroup's counter: what requests per
    // CPU-second divides by. Null where the counter could not be read. Read before the sampler
    // starts, since under Docker Desktop a read takes a few seconds.
    const before = sampled.map(cpuUsec);
    const s = spawn("node", [join(here, "sample-stats.mjs"), join(results, `${file}-${phase}-stats.json`), "62", ...sampled], { stdio: "ignore", windowsHide: true });
    k6(`${file}-${phase}`, base, envs);
    const after = sampled.map(cpuUsec);
    writeFileSync(join(results, `${file}-${phase}-cpu.json`), JSON.stringify(Object.fromEntries(sampled.map((c, i) => [c, before[i] !== null && after[i] !== null ? { usec: after[i] - before[i] } : null])), null, 1));
    await new Promise((r) => (s.exitCode !== null ? r() : s.on("exit", r)));
  }
  docker("rm", "-f", `fsw-${name}`);
}
log("done");
