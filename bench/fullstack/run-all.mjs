// The whole comparison, in order, on a quiet machine: cold start, the item page and the item as
// JSON without a database and against the item API, instructions per request, then all apps up
// together for parity, byte counts and browser timing.
// Usage: node run-all.mjs <round> [browserRuns=10] [from=cold-start|fixture|api|count|browser]
//   from: start at that step and keep the results already in results/<round> from earlier ones.
// On a bench host set BENCH_APP_CPUS, BENCH_LOAD_CPUS and BENCH_API_CONTAINER (droplet.mjs does).
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { apps, runArgs } from "./apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const [round = "r1", browserRuns = "10", from = "cold-start"] = process.argv.slice(2);
const order = ["cold-start", "fixture", "api", "count", "browser"];
if (!order.includes(from)) throw new Error(`from must be one of ${order.join(", ")}`);
const runs = (name) => order.indexOf(name) >= order.indexOf(from);
const all = Object.keys(apps).join(",");
const step = (...args) => {
  console.log(new Date().toISOString(), "node", args.join(" "));
  const r = spawnSync("node", args, { cwd: here, stdio: "inherit" });
  if (r.status !== 0) console.log(`step failed with ${r.status}`);
};
const docker = (...args) => spawnSync("docker", args, { encoding: "utf8" });

if (runs("cold-start")) step("cold-start.mjs", round, "10");
if (runs("fixture")) step("run-ssr.mjs", round, "fixture", "300", all, "page");
if (runs("fixture")) step("run-ssr.mjs", round, "fixture", "300", all, "json");
if (runs("api")) step("run-ssr.mjs", round, "api", "150", all, "page");
if (runs("api")) step("run-ssr.mjs", round, "api", "150", all, "json");
if (runs("count") && process.platform === "linux") step("count.mjs", round, "3");
for (const [name, app] of Object.entries(apps)) {
  docker("rm", "-f", `fsw-${name}`);
  docker(...runArgs(name, app, "api", join(here, "fixtures")));
}
// Every app shares the app core here, so slow starters (the JVM) take a while: wait for each.
for (const [name, app] of Object.entries(apps)) {
  let up = false;
  for (let i = 0; i < 240 && !up; i += 1) {
    try { up = (await fetch(`http://localhost:${app.port}/items/item_1`)).ok; } catch {}
    if (!up) await new Promise((r) => setTimeout(r, 500));
  }
  if (!up) console.log(`${name} did not answer within two minutes`);
}
step("parity.mjs");
step("bytes.mjs", round);
step("browser.mjs", ...Object.entries(apps).map(([n, a]) => `${n}=http://localhost:${a.port}`), `runs=${browserRuns}`, `out=${round}/browser.json`);
step("summarize.mjs", round);
for (const name of Object.keys(apps)) docker("rm", "-f", `fsw-${name}`);
console.log(new Date().toISOString(), "all done");
