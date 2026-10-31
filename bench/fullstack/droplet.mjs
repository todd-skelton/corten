// Runs the whole comparison on a fresh DigitalOcean droplet with dedicated CPUs, then destroys it.
// A dedicated host keeps other work off the cores, pins each piece to its own cores, and exposes
// hardware counters for `count.mjs`. Needs `doctl` (authenticated), `ssh`, `scp` and `tar`.
// Usage (from the repository root or anywhere):
//   node bench/fullstack/droplet.mjs <round> --ssh-key <DO key id> [--size c-8] [--region sfo2]
//   node bench/fullstack/droplet.mjs <round> --host <ip> [--keep]   (reuse a droplet)
//   --from <step>: start run-all.mjs at that step and keep the droplet's earlier results.
//   --attach: with --host, only wait for the round already running there and download it.
//   --suite live: run bench/live (run.mjs, then summarize.mjs) instead; results in bench/live/results.
//   --suite browser: only the browser pass of this comparison (run-all.mjs from "browser": parity,
//     bytes, browser timing), then bench/browser's run-all.mjs; results in both results folders.
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
if (!["fullstack", "live", "browser"].includes(suite)) throw new Error("--suite is fullstack, live or browser");
const suiteDir = `/bench/bench/${suite === "live" ? "live" : "fullstack"}`;
// The last line of a finished run: bench/browser's run-all.mjs prints its own after fullstack's.
const doneMarker = suite === "browser" ? /browser suite (done|finished)/ : /all done/;
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
// rest the load; with items in memory the item API is idle, so the load also gets 2-3.
const env = suite === "live"
  ? "BENCH_APP_CPUS=1 BENCH_LOAD_CPUS=2-$(($(nproc)-1)) BENCH_LIVE_WORKERS=$(($(nproc)-2))"
  : "BENCH_APP_CPUS=1 BENCH_API_CONTAINER=item-api BENCH_LOAD_CPUS=4-$(($(nproc)-1)) BENCH_FIXTURE_LOAD_CPUS=2-$(($(nproc)-1))";
const ssh = (script, options) => run("ssh", ["-o", "StrictHostKeyChecking=accept-new", "-o", "ServerAliveInterval=30", `root@${host}`, script], options);

try {
  if (!host) {
    const key = flag("ssh-key");
    if (!key) throw new Error("--ssh-key <DigitalOcean key id> is required to create a droplet");
    const created = run("doctl", ["compute", "droplet", "create", `corten-bench-${suite === "fullstack" ? "" : `${suite}-`}${round}`, "--size", flag("size", "c-8"), "--region", flag("region", "sfo2"),
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
      // No package updates during the round: r8's droplet installed a new kernel while it ran
      // (#466), so a power cycle came back on a kernel `perf` was not installed for, and an apt run
      // takes CPU and memory from what is being measured. Then wait for an apt run already started
      // (both services are oneshot: a unit ordered after them starts when they finish).
      "(systemctl disable --now apt-daily.timer apt-daily-upgrade.timer >/dev/null 2>&1 || true)",
      "timeout 900 systemd-run --quiet --property=After=apt-daily.service --property=After=apt-daily-upgrade.service --wait /bin/true",
      "command -v docker >/dev/null || curl -fsSL https://get.docker.com | sh >/dev/null",
      "command -v node >/dev/null || (curl -fsSL https://deb.nodesource.com/setup_24.x | bash - >/dev/null && apt-get install -y -qq nodejs >/dev/null)",
      "command -v perf >/dev/null || apt-get install -y -qq linux-tools-common linux-tools-$(uname -r) >/dev/null",
      // bench/browser renders its Corten pages with a small Rust program (gen/).
      ...(suite === "browser"
        ? ["command -v cc >/dev/null || apt-get install -y -qq build-essential >/dev/null", "command -v cargo >/dev/null || (curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null)"]
        : []),
    ].join(" && "));

    console.log("uploading the working tree");
    const tarball = join(here, "results", ".upload.tgz");
    mkdirSync(dirname(tarball), { recursive: true });
    run("tar", ["-czf", tarball, "--exclude=target", "--exclude=node_modules", "--exclude=./bench/fullstack/results", "--exclude=./bench/live/results", "--exclude=./bench/browser/results", "--exclude=./bench/browser/out", "--exclude=./.git", "-C", repo, "."]);
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
        ...(suite === "browser" ? ["(cd bench/browser && npm ci --silent)"] : []),
        "docker build -q -f examples/card-market/Dockerfile -t cb-corten .",
        ...fullstackImages(Object.keys(apps)),
        "docker network create rustbench >/dev/null 2>&1 || true",
        "docker rm -f item-api >/dev/null 2>&1 || true",
        "docker run -d --name item-api --network rustbench --cpuset-cpus 2-3 -v /bench/bench/fullstack:/b:ro node:24-bookworm-slim node /b/item-api/server.mjs /b/fixtures/items.json",
      ].join(" && "));

    // Detached, so a dropped connection does not stop the run; polled until it finishes.
    console.log(`running round ${round} (${{ live: "about an hour", browser: "about an hour", fullstack: "about six hours for fourteen apps" }[suite]})`);
    // `;`, not `&&`: only the node command goes to the background, so ssh returns at once.
    const command = {
      live: `sh -c 'node run.mjs ${round}; node summarize.mjs ${round}; echo all done'`,
      browser: `sh -c '. "$HOME/.cargo/env"; node run-all.mjs ${round} 10 browser; cd /bench/bench/browser && node run-all.mjs ${round}'`,
      fullstack: `node run-all.mjs ${round} 10 ${from ?? ""}`,
    }[suite];
    ssh(`cd ${suiteDir}; ${env} nohup ${command} ${from ? ">>" : ">"} ${log} 2>&1 < /dev/null &`);
  }
  let shown = 0;
  for (;;) {
    await sleep(60_000);
    const r = ssh(`cat ${log}`, { quiet: true, allowFail: true });
    if (r.status !== 0) continue;
    const lines = r.stdout.split("\n").filter((l) => /^\d{4}-|step failed|all done|browser suite|^\|/.test(l));
    for (const l of lines.slice(shown)) console.log(l);
    shown = lines.length;
    if (doneMarker.test(r.stdout)) break;
  }

  console.log("downloading the results");
  // host.txt: the CPU model and the rest, since droplets of one size are not all the same machine.
  const hostInfo = "(lscpu; free -m; uname -r; docker --version; node --version; npx playwright --version)";
  const download = (remoteDir, localName) => {
    const local = join(here, "..", localName, "results");
    mkdirSync(local, { recursive: true });
    ssh(`cd ${remoteDir} && cp ${log} results/${round}/run.log && ${hostInfo} > results/${round}/host.txt 2>&1; tar -czf /tmp/results.tgz -C results ${round}`);
    run("scp", ["-q", `root@${host}:/tmp/results.tgz`, join(local, ".results.tgz")]);
    run("tar", ["-xzf", join(local, ".results.tgz"), "-C", local]);
    rmSync(join(local, ".results.tgz"));
    console.log(`results in ${join(local, round)}`);
  };
  download(suiteDir, suite === "live" ? "live" : "fullstack");
  if (suite === "browser") download("/bench/bench/browser", "browser");
} finally {
  destroy();
}
