// One round of the browser benchmark, in order, on a quiet machine: build every app, then the
// operations (two rounds, the second in reverse order), the server-rendered pages and the
// dashboard. Results land in results/<round>/; summarize.mjs prints the README table from them.
// Usage: node run-all.mjs <round>
// droplet.mjs (bench/fullstack) runs it on a Linux droplet with `--suite browser`.
import { spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const round = process.argv[2];
if (!round) throw new Error("usage: node run-all.mjs <round>");
const dir = join(here, "results", round);
mkdirSync(dir, { recursive: true });
let failed = 0;
const step = (...args) => {
  console.log(new Date().toISOString(), "node", args.join(" "));
  const r = spawnSync("node", args, { cwd: here, stdio: "inherit" });
  if (r.status !== 0) {
    failed += 1;
    console.log(`step failed with ${r.status}`);
  }
};
step("build.mjs");
step("run.mjs", "--iterations", "15", "--rounds", "2", "--apps", "vanilla,view,solid,react,svelte,vue,remix", "--out", join(dir, "ops.json"));
step("ssr.mjs", "--loads", "30", "--apps", "vanilla-ssr,view-ssr,solid-ssr,react-ssr,svelte-ssr,vue-ssr,remix-ssr", "--out", join(dir, "ssr.json"));
step("dashboard.mjs", "--iterations", "30", "--apps", "dashboard-vanilla,dashboard-view,dashboard-solid,dashboard-react,dashboard-svelte,dashboard-vue,dashboard-remix", "--out", join(dir, "dashboard.json"));
step("summarize.mjs", round);
console.log(new Date().toISOString(), failed ? `browser suite finished with ${failed} failed steps` : "browser suite done");
