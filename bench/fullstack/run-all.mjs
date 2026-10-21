// The whole comparison, in order, on a quiet machine: cold start, SSR without a database,
// SSR against the item API, then all apps up together for browser timing and byte counts.
// Usage: node run-all.mjs <round> [browserRuns=10]
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { apps, runArgs } from "./apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const [round = "r1", browserRuns = "10"] = process.argv.slice(2);
const step = (...args) => {
  console.log(new Date().toISOString(), "node", args.join(" "));
  const r = spawnSync("node", args, { cwd: here, stdio: "inherit" });
  if (r.status !== 0) console.log(`step failed with ${r.status}`);
};
const docker = (...args) => spawnSync("docker", args, { encoding: "utf8" });

step("cold-start.mjs", round, "10");
step("run-ssr.mjs", round, "fixture", "300");
step("run-ssr.mjs", round, "api", "150");
for (const [name, app] of Object.entries(apps)) {
  docker("rm", "-f", `fsw-${name}`);
  docker(...runArgs(name, app, "api", ""));
}
await new Promise((r) => setTimeout(r, 8000));
step("parity.mjs");
step("bytes.mjs", round);
step("browser.mjs", ...Object.entries(apps).map(([n, a]) => `${n}=http://localhost:${a.port}`), `runs=${browserRuns}`, `out=${round}/browser.json`);
step("summarize.mjs", round);
for (const name of Object.keys(apps)) docker("rm", "-f", `fsw-${name}`);
console.log(new Date().toISOString(), "all done");
