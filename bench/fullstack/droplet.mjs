// Runs the whole comparison on a fresh DigitalOcean droplet with dedicated CPUs, then destroys it.
// A dedicated host keeps other work off the cores, pins each piece to its own cores, and exposes
// hardware counters for `count.mjs`. Needs `doctl` (authenticated), `ssh`, `scp` and `tar`.
// Usage (from the repository root or anywhere):
//   node bench/fullstack/droplet.mjs <round> --ssh-key <DO key id> [--size c-8] [--region sfo2]
//   node bench/fullstack/droplet.mjs <round> --host <ip> [--keep]   (reuse a droplet)
//   --from <step>: start run-all.mjs at that step and keep the droplet's earlier results.
//   --attach: with --host, only wait for the round already running there and download it.
//   --suite live: run bench/live (run.mjs, then summarize.mjs) instead; results in bench/live/results.
// Results land in bench/fullstack/results/<round>/. The droplet is destroyed when the run ends,
// fails or is interrupted, unless --keep (and never one passed with --host).
import { spawnSync } from "node:child_process";
import { mkdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { apps } from "./apps.mjs";
import { live } from "../live/apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../..");
const args = process.argv.slice(2);
const round = args[0];
const flag = (name, fallback) => { const i = args.indexOf(`--${name}`); return i >= 0 ? args[i + 1] : fallback; };
if (!round || round.startsWith("--")) throw new Error("usage: node droplet.mjs <round> (--ssh-key <id> | --host <ip>)");
const suite = flag("suite", "fullstack");
if (!["fullstack", "live"].includes(suite)) throw new Error("--suite is fullstack or live");
const suiteDir = `/bench/bench/${suite}`;
const run = (cmd, cmdArgs, options = {}) => {
  const r = spawnSync(cmd, cmdArgs, { encoding: "utf8", stdio: options.quiet ? "pipe" : "inherit", ...options });
  if (r.status !== 0 && !options.allowFail) throw new Error(`${cmd} ${cmdArgs.join(" ")} failed: ${r.stderr ?? r.status}`);
  return r;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let host = flag("host");
let dropletId = null;
const destroy = () => {
  if (dropletId && !args.includes("--keep")) {
    console.log(`destroying droplet ${dropletId}`);
    run("doctl", ["compute", "droplet", "delete", dropletId, "--force"], { allowFail: true });
    dropletId = null;
  }
};
process.on("SIGINT", () => { destroy(); process.exit(130); });

// Cores: 0 the system and Docker, 1 the app under test, 2-3 the item API (fullstack only), the
// rest the load.
const env = suite === "live"
  ? "BENCH_APP_CPUS=1 BENCH_LOAD_CPUS=2-$(($(nproc)-1)) BENCH_LIVE_WORKERS=$(($(nproc)-2))"
  : "BENCH_APP_CPUS=1 BENCH_API_CONTAINER=item-api BENCH_LOAD_CPUS=4-$(($(nproc)-1))";
const ssh = (script, options) => run("ssh", ["-o", "StrictHostKeyChecking=accept-new", "-o", "ServerAliveInterval=30", `root@${host}`, script], options);

try {
  if (!host) {
    const key = flag("ssh-key");
    if (!key) throw new Error("--ssh-key <DigitalOcean key id> is required to create a droplet");
    const created = run("doctl", ["compute", "droplet", "create", `corten-bench-${suite === "live" ? "live-" : ""}${round}`, "--size", flag("size", "c-8"), "--region", flag("region", "sfo2"),
      "--image", "ubuntu-24-04-x64", "--ssh-keys", key, "--tag-name", "corten-bench", "--wait", "--format", "ID,PublicIPv4", "--no-header"], { quiet: true });
    [dropletId, host] = created.stdout.trim().split(/\s+/);
    console.log(`droplet ${dropletId} at ${host}`);
    for (let i = 0; i < 30 && ssh("true", { quiet: true, allowFail: true }).status !== 0; i += 1) await sleep(5000);
  }

  const attach = args.includes("--attach");
  if (attach && !flag("host")) throw new Error("--attach needs --host");
  const log = `${suiteDir}/results-${round}.log`;
  if (!attach) {
    console.log("provisioning");
    ssh([
      "cloud-init status --wait >/dev/null",
      "command -v docker >/dev/null || curl -fsSL https://get.docker.com | sh >/dev/null",
      "command -v node >/dev/null || (curl -fsSL https://deb.nodesource.com/setup_24.x | bash - >/dev/null && apt-get install -y -qq nodejs >/dev/null)",
      "command -v perf >/dev/null || apt-get install -y -qq linux-tools-common linux-tools-$(uname -r) >/dev/null",
    ].join(" && "));

    console.log("uploading the working tree");
    const tarball = join(here, "results", ".upload.tgz");
    mkdirSync(dirname(tarball), { recursive: true });
    run("tar", ["-czf", tarball, "--exclude=target", "--exclude=node_modules", "--exclude=./bench/fullstack/results", "--exclude=./bench/live/results", "--exclude=./.git", "-C", repo, "."]);
    const from = flag("from");
    ssh(from ? "mkdir -p /bench" : "rm -rf /bench && mkdir -p /bench");
    run("scp", ["-q", tarball, `root@${host}:/bench/src.tgz`]);
    rmSync(tarball);

    console.log("building the images");
    const fullstackImages = (names) => names.filter((app) => app !== "corten").map((app) => `docker build -q -t cb-${app} bench/fullstack/apps/${app}`);
    ssh(suite === "live"
      ? [
        "cd /bench && tar -xzf src.tgz",
        "docker build -q -f bench/live/apps/corten/Dockerfile -t cb-live-corten .",
        ...fullstackImages(Object.keys(live)),
        "docker network create rustbench >/dev/null 2>&1 || true",
      ].join(" && ")
      : [
        "cd /bench && tar -xzf src.tgz && npm ci --silent && npx playwright install --with-deps chromium >/dev/null",
        "docker build -q -f examples/item-page/Dockerfile -t cb-corten .",
        ...fullstackImages(Object.keys(apps)),
        "docker network create rustbench >/dev/null 2>&1 || true",
        "docker rm -f item-api >/dev/null 2>&1 || true",
        "docker run -d --name item-api --network rustbench --cpuset-cpus 2-3 -v /bench/bench/fullstack:/b:ro node:24-bookworm-slim node /b/item-api/server.mjs /b/fixtures/items.json",
      ].join(" && "));

    // Detached, so a dropped connection does not stop the run; polled until it finishes.
    console.log(`running round ${round} (${suite === "live" ? "about an hour" : "about six hours for fourteen apps"})`);
    // `;`, not `&&`: only the node command goes to the background, so ssh returns at once.
    const command = suite === "live"
      ? `sh -c 'node run.mjs ${round}; node summarize.mjs ${round}; echo all done'`
      : `node run-all.mjs ${round} 10 ${from ?? ""}`;
    ssh(`cd ${suiteDir}; ${env} nohup ${command} ${from ? ">>" : ">"} ${log} 2>&1 < /dev/null &`);
  }
  let shown = 0;
  for (;;) {
    await sleep(60_000);
    const r = ssh(`cat ${log}`, { quiet: true, allowFail: true });
    if (r.status !== 0) continue;
    const lines = r.stdout.split("\n").filter((l) => /^\d{4}-|step failed|all done|^\|/.test(l));
    for (const l of lines.slice(shown)) console.log(l);
    shown = lines.length;
    if (r.stdout.includes("all done")) break;
  }

  console.log("downloading the results");
  const local = join(here, "..", suite, "results");
  mkdirSync(local, { recursive: true });
  // host.txt: the CPU model and the rest, since droplets of one size are not all the same machine.
  ssh(`cd ${suiteDir} && cp results-${round}.log results/${round}/run.log && (lscpu; free -m; uname -r; docker --version; node --version) > results/${round}/host.txt 2>&1; tar -czf /tmp/results.tgz -C results ${round}`);
  run("scp", ["-q", `root@${host}:/tmp/results.tgz`, join(local, ".results.tgz")]);
  run("tar", ["-xzf", join(local, ".results.tgz"), "-C", local]);
  rmSync(join(local, ".results.tgz"));
  console.log(`results in ${join(local, round)}`);
} finally {
  destroy();
}
