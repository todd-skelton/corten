// The whole comparison, in order, on a quiet machine: cold start, the item page and the item as
// JSON without a database and against the item API, instructions per request, then all apps up
// together for parity, byte counts and browser timing.
// Usage: node run-all.mjs <round> [browserRuns=10] [from=cold-start|fixture|api|count|browser] [--unattended]
//   from: start at that step and keep the results already in results/<round> from earlier ones.
//   --unattended: how droplet.mjs runs a round on a droplet, under a systemd unit that starts it
//     again when the droplet restarts. Each call has a time limit (round.mjs) and may not go an
//     hour without printing anything; one that does is stopped and the rest of its step skipped.
//     Progress goes to results-<round>.progress: started again after a restart (a lockup panics
//     the kernel, which reboots), the round skips the step that was running and goes on from the
//     next. A skipped step is written to the log and to results/<round>/skipped.txt
//     ("<time> step <name> skipped: <why>"), which summarize.mjs copies into summary.json.
// On a bench host set BENCH_APP_CPUS, BENCH_LOAD_CPUS and BENCH_API_CONTAINER (droplet.mjs does).
import { spawn, spawnSync } from "node:child_process";
import { appendFileSync, closeSync, existsSync, fsyncSync, mkdirSync, openSync, readFileSync, writeSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { apiContainer, apps, network, runArgs } from "./apps.mjs";
import { calls, idleMinutes, mayStart, resumeAt, steps } from "./round.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const unattended = process.argv.includes("--unattended");
const [round = "r1", browserRuns = "10", from = "cold-start"] = process.argv.slice(2).filter((a) => a !== "--unattended");
if (!steps.includes(from)) throw new Error(`from must be one of ${steps.join(", ")}`);
// It removes every container on the host, so only where droplet.mjs runs it.
if (unattended && (process.platform !== "linux" || !process.env.BENCH_APP_CPUS)) throw new Error("--unattended is for a bench droplet (droplet.mjs runs it there)");
const minute = 60_000;
const now = () => new Date().toISOString();
const docker = (/** @type {string[]} */ ...args) => spawnSync("docker", args, { encoding: "utf8", timeout: 5 * minute });
const sleep = (/** @type {number} */ ms) => new Promise((r) => setTimeout(r, ms));
const progressFile = join(here, `results-${round}.progress`);
// Synced to disk (the file and, when it is new, its directory), so a line written just before a
// kernel panic is still there after the reboot.
const progress = (/** @type {string} */ what) => {
  if (!unattended) return;
  const created = !existsSync(progressFile);
  const fd = openSync(progressFile, "a");
  try { writeSync(fd, `${now()} ${what}\n`); fsyncSync(fd); } finally { closeSync(fd); }
  if (created) { const dir = openSync(here, "r"); try { fsyncSync(dir); } finally { closeSync(dir); } }
};
const skippedFile = join(here, "results", round, "skipped.txt");
const skip = (/** @type {string} */ step, /** @type {string} */ why) => {
  const line = `${now()} step ${step} skipped: ${why}`;
  console.log(line);
  mkdirSync(dirname(skippedFile), { recursive: true });
  appendFileSync(skippedFile, `${line}\n`);
};

// Runs one call and resolves to its exit code and, unattended, why it was stopped: past its
// limit or quiet for too long, it is stopped with everything it started (its process group: perf,
// the docker clients; the containers they started go in reset()).
const call = (/** @type {number} */ limit, /** @type {string[]} */ args) => /** @type {Promise<{ stopped: string | null, code: number | null }>} */ (new Promise((done) => {
  console.log(now(), "node", args.join(" "));
  // Unattended, its output passes through here so a quiet call can be told from a busy one.
  const child = spawn("node", args, { cwd: here, stdio: unattended ? ["ignore", "pipe", "pipe"] : "inherit", detached: unattended });
  const started = Date.now();
  let heard = started;
  /** @type {string | null} */
  let stopped = null;
  for (const stream of [child.stdout, child.stderr]) stream?.on("data", (chunk) => { heard = Date.now(); process.stdout.write(chunk); });
  const kill = (/** @type {NodeJS.Signals} */ signal) => { try { process.kill(-(child.pid ?? 0), signal); } catch {} };
  const watch = unattended ? setInterval(() => {
    if (stopped) return;
    if (Date.now() - started > limit * minute) stopped = `${args[0]} ran longer than ${limit} minutes, so it was stopped`;
    else if (Date.now() - heard > idleMinutes * minute) stopped = `${args[0]} printed nothing for ${idleMinutes} minutes, so it was stopped`;
    else return;
    kill("SIGTERM");
    setTimeout(() => kill("SIGKILL"), 30_000).unref();
  }, 15_000) : null;
  child.on("close", (code) => {
    if (watch) clearInterval(watch);
    if (!stopped && code !== 0) console.log(`step failed with ${code}`);
    done({ stopped, code });
  });
}));

// Unattended only: a clean slate after a stopped call or a restart. The benchmark's containers go
// (a stopped k6 client leaves its container running; a restart leaves them all stopped): the apps,
// cold start's, the item API, and k6's and cpu-time.mjs's by image. Then the item API starts again
// on its cores.
const reset = () => {
  const ids = [["name=fsw-", "name=cs-", "name=item-api"], ["ancestor=grafana/k6:latest", "ancestor=alpine"]]
    .flatMap((filters) => docker("ps", "-aq", ...filters.flatMap((f) => ["--filter", f])).stdout?.split("\n").filter(Boolean) ?? []);
  if (ids.length) docker("rm", "-f", ...new Set(ids));
  if (apiContainer !== "item-api") return;
  const r = docker("run", "-d", "--name", "item-api", "--network", network, "--cpuset-cpus", "2-3", "-v", `${here}:/b:ro`,
    "node:24-bookworm-slim", "node", "/b/item-api/server.mjs", "/b/fixtures/items.json");
  if (r.status !== 0) throw new Error(`could not start the item API: ${r.stderr || r.error?.message}`);
};

// Every app up at once, for the browser step's calls.
const startEveryApp = async () => {
  console.log(now(), "starting every app for parity, bytes and browser timing");
  for (const [name, app] of Object.entries(apps)) {
    docker("rm", "-f", `fsw-${name}`);
    docker(...runArgs(name, app, "api", join(here, "fixtures")));
  }
  // Every app shares the app core here, so slow starters (the JVM) take a while: wait up to two
  // minutes for each, however its port behaves.
  for (const [name, app] of Object.entries(apps)) {
    const deadline = Date.now() + 2 * minute;
    let up = false;
    while (!up && Date.now() < deadline) {
      const left = Math.max(1, Math.min(5000, deadline - Date.now()));
      try { up = (await fetch(`http://localhost:${app.port}/items/item_1`, { signal: AbortSignal.timeout(left) })).ok; } catch {}
      if (!up) await sleep(500);
    }
    if (!up) console.log(`${name} did not answer within two minutes`);
  }
};
const stopEveryApp = () => { for (const name of Object.keys(apps)) docker("rm", "-f", `fsw-${name}`); };

try {
  /** @type {string | null} */
  let first = from;
  if (unattended) {
    const state = resumeAt(existsSync(progressFile) ? readFileSync(progressFile, "utf8") : "", from);
    if (state.ended) process.exit(0);
    // Written before anything else, so every start counts, even one cut short in reset().
    progress("boot");
    if (state.boots) {
      console.log(now(), "the round starts again after the droplet restarted");
      // A restart in a step skips it, so a working round starts about once a step at most.
      if (!mayStart(state.boots)) throw new Error(`the round has started ${state.boots + 1} times`);
      if (state.skip) skip(state.skip, "the droplet restarted while it ran");
    }
    reset();
    first = state.next;
  }
  const all = calls(round, browserRuns, Object.keys(apps), Object.values(apps).map((a) => a.port));
  for (const step of first ? steps.slice(steps.indexOf(first)) : []) {
    if (step === "count" && process.platform !== "linux") continue;
    progress(`start ${step}`);
    if (step === "browser") await startEveryApp();
    for (const [, limit, ...args] of all.filter(([s]) => s === step)) {
      const { stopped } = await call(limit, args);
      if (stopped) {
        skip(step, stopped);
        if (unattended) reset();
        break;
      }
    }
    if (step === "browser") stopEveryApp();
    progress(`end ${step}`);
  }
  progress("start summary");
  // A round whose summary failed has not finished.
  const summary = await call(10, ["summarize.mjs", round]);
  if (summary.stopped || summary.code !== 0) throw new Error(summary.stopped ?? `summarize.mjs exited with ${summary.code}`);
  console.log(now(), "all done");
  progress("finished");
} catch (e) {
  console.log(now(), `round failed: ${e instanceof Error ? e.message : e}`);
  progress("failed");
  process.exit(1);
}
