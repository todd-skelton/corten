// Runs the whole comparison on a fresh DigitalOcean droplet with dedicated CPUs, then destroys it.
// A dedicated host keeps other work off the cores, pins each piece to its own cores, and exposes
// hardware counters for `count.mjs`. Needs `doctl` (authenticated), `ssh`, `scp` and `tar`.
// Usage (from the repository root or anywhere):
//   node bench/fullstack/droplet.mjs <round> --ssh-key <DO key id> [--size c-8] [--region sfo2]
//   node bench/fullstack/droplet.mjs <round> --host <ip> [--keep]   (reuse a droplet)
//   --from <step>: start run-all.mjs at that step and keep the droplet's earlier results.
//   --attach: with --host, only wait for the round already running there and download it.
//   --stall <minutes>: how long a droplet may not answer before it is power-cycled (default 15).
//   --suite live: run bench/live (run.mjs, then summarize.mjs) instead; results in bench/live/results.
//   --suite browser: only the browser pass of this comparison (run-all.mjs from "browser": parity,
//     bytes, browser timing), then bench/browser's run-all.mjs; results in both results folders.
// Results land in bench/fullstack/results/<round>/. The droplet is destroyed when the run ends,
// fails or is interrupted, unless --keep (and never one passed with --host).
// If a full-stack round's droplet stops answering (r8's locked up in count.mjs, #466), it is
// power-cycled, the step it was in is skipped ("step <name> skipped" in run.log) and the round
// goes on from the next step, at most twice; after that the run fails rather than wait forever.
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
const minute = 60_000;
// stdin is never inherited: started from a hidden console, Windows' ssh sometimes keeps waiting on
// that console after the remote command has finished (the r7, r8 and r9 runs each hung for hours
// in one such call; in a test 2 of 6 calls hung with stdin inherited and none of 30 with it
// ignored). Every call also has a time limit, so a hang becomes an error or a retry.
const run = (cmd, cmdArgs, { quiet, allowFail, timeout = 2 * minute, ...options } = {}) => {
  const out = quiet ? "pipe" : "inherit";
  const r = spawnSync(cmd, cmdArgs, { encoding: "utf8", stdio: ["ignore", out, out], timeout, ...options });
  const failure = r.error?.code === "ETIMEDOUT" ? `no answer within ${timeout / 1000} s`
    : r.error ? r.error.message
    : r.status !== 0 ? (r.stderr || `exit ${r.status}`) : null;
  if (failure && !allowFail) throw new Error(`${cmd} ${cmdArgs.join(" ")} failed: ${failure}`);
  return failure ? { ...r, status: r.status || 1 } : r;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// Only for work that can run again safely: up to `times` attempts, a minute apart.
const retry = async (times, work) => {
  for (let attempt = 1; ; attempt += 1) {
    try { return work(); } catch (e) {
      if (attempt >= times) throw e;
      console.log(`retrying (${attempt} of ${times - 1}): ${String(e.message).split("\n")[0]}`);
      await sleep(minute);
    }
  }
};

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
// BatchMode: never wait at a prompt. A connection that goes silent is dropped after two minutes.
const sshOptions = ["-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=accept-new", "-o", "ConnectTimeout=20", "-o", "ServerAliveInterval=30", "-o", "ServerAliveCountMax=4"];
const ssh = (script, options) => run("ssh", [...sshOptions, `root@${host}`, script], options);
const scp = (from, to) => run("scp", ["-q", ...sshOptions, from, to], { timeout: 15 * minute });
const answers = async (tries) => {
  for (let i = 0; i < tries; i += 1) {
    if (ssh("true", { quiet: true, allowFail: true, timeout: 30_000 }).status === 0) return true;
    await sleep(5000);
  }
  return false;
};
// run-all.mjs's steps, in its order, and the step a "<time> node <script> ..." line of its log runs.
const steps = ["cold-start", "fixture", "api", "count", "browser"];
const stepOf = (line) => {
  const m = /^\S+ node (\S+)\.mjs(?: \S+ (fixture|api)\b)?/.exec(line);
  if (!m) return null;
  if (m[1] === "run-ssr") return m[2];
  return { "cold-start": "cold-start", count: "count", parity: "browser", bytes: "browser", browser: "browser", summarize: "browser" }[m[1]] ?? null;
};

const log = `${suiteDir}/results-${round}.log`;
const itemApi = "(docker rm -f item-api >/dev/null 2>&1; docker run -d --name item-api --network rustbench --cpuset-cpus 2-3 -v /bench/bench/fullstack:/b:ro node:24-bookworm-slim node /b/item-api/server.mjs /b/fixtures/items.json >/dev/null)";
const running = `pgrep -f 'run(-all)?\\.mjs ${round}' >/dev/null`;
// Starts `command` in the background on the droplet, detached, so a dropped connection does not
// stop it (`;`, not `&&`: only the command goes to the background, so ssh returns at once). Not
// retried blindly: if the call fails, the round counts as started when its process is there.
const launch = (command, append) => {
  if (ssh(`cd ${suiteDir}; ${env} nohup ${command} ${append ? ">>" : ">"} ${log} 2>&1 < /dev/null &`, { allowFail: true }).status === 0) return;
  if (ssh(running, { quiet: true, allowFail: true }).status === 0) return;
  throw new Error(`could not start the round on ${host}`);
};

try {
  if (!host) {
    const key = flag("ssh-key");
    if (!key) throw new Error("--ssh-key <DigitalOcean key id> is required to create a droplet");
    const created = run("doctl", ["compute", "droplet", "create", `corten-bench-${suite === "fullstack" ? "" : `${suite}-`}${round}`, "--size", flag("size", "c-8"), "--region", flag("region", "sfo2"),
      "--image", "ubuntu-24-04-x64", "--ssh-keys", key, "--tag-name", "corten-bench", "--wait", "--format", "ID,PublicIPv4", "--no-header"], { quiet: true, timeout: 10 * minute });
    [dropletId, host] = created.stdout.trim().split(/\s+/);
    console.log(`droplet ${dropletId} at ${host}`);
    await answers(30);
  }

  const attach = args.includes("--attach");
  if (attach && !flag("host")) throw new Error("--attach needs --host");
  if (!attach) {
    console.log("provisioning");
    await retry(2, () => ssh([
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
    ].join(" && "), { timeout: 30 * minute }));

    console.log("uploading the working tree");
    const tarball = join(here, "results", ".upload.tgz");
    mkdirSync(dirname(tarball), { recursive: true });
    run("tar", ["-czf", tarball, "--exclude=target", "--exclude=node_modules", "--exclude=./bench/fullstack/results", "--exclude=./bench/live/results", "--exclude=./bench/browser/results", "--exclude=./bench/browser/out", "--exclude=./.git", "-C", repo, "."]);
    const from = flag("from");
    await retry(3, () => ssh(from ? "mkdir -p /bench" : "rm -rf /bench && mkdir -p /bench"));
    await retry(3, () => scp(tarball, `root@${host}:/bench/src.tgz`));
    rmSync(tarball);

    console.log("building the images");
    const fullstackImages = (names) => names.filter((app) => app !== "corten").map((app) => `docker build -q -t cb-${app} bench/fullstack/apps/${app}`);
    await retry(2, () => ssh(suite === "live"
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
        itemApi,
      ].join(" && "), { timeout: 90 * minute }));

    console.log(`running round ${round} (${{ live: "about an hour", browser: "about an hour", fullstack: "about six hours for fourteen apps" }[suite]})`);
    launch({
      live: `sh -c 'node run.mjs ${round}; node summarize.mjs ${round}; echo all done'`,
      browser: `sh -c '. "$HOME/.cargo/env"; node run-all.mjs ${round} 10 browser; cd /bench/bench/browser && node run-all.mjs ${round}'`,
      fullstack: `node run-all.mjs ${round} 10 ${from ?? ""}`,
    }[suite], Boolean(from));
  }
  const stall = Number(flag("stall", "15")) * minute;
  let shown = 0;
  let lastLog = "";
  let silentSince = null;
  let recoveries = 0;
  let gone = 0;
  for (;;) {
    await sleep(minute);
    const r = ssh(`cat ${log}; ${running} || echo __stopped__`, { quiet: true, allowFail: true });
    if (r.status === 0) {
      silentSince = null;
      lastLog = r.stdout;
      // Two polls in a row with no round process and no last line: it died without finishing.
      gone = r.stdout.includes("__stopped__") && !doneMarker.test(r.stdout) ? gone + 1 : 0;
      if (gone >= 2) throw new Error(`the round's process on ${host} stopped before it finished; its log is ${log}`);
      const lines = r.stdout.split("\n").filter((l) => /^\d{4}-|step failed|step \S+ skipped|all done|browser suite|^\|/.test(l));
      for (const l of lines.slice(shown)) console.log(l);
      shown = lines.length;
      if (doneMarker.test(r.stdout)) break;
      continue;
    }
    silentSince ??= Date.now();
    if (Date.now() - silentSince < stall) continue;
    // The droplet has not answered for `stall`: restart it and go on without the step it was in.
    const step = lastLog.split("\n").map(stepOf).filter(Boolean).at(-1);
    if (suite !== "fullstack" || !step || recoveries >= 2) {
      throw new Error(`the droplet has not answered for ${stall / minute} minutes${step ? ` (in step ${step})` : ""}`);
    }
    recoveries += 1;
    const id = dropletId ?? run("doctl", ["compute", "droplet", "list", "--tag-name", "corten-bench", "--format", "ID,PublicIPv4", "--no-header"], { quiet: true })
      .stdout.split("\n").map((l) => l.trim().split(/\s+/)).find(([, ip]) => ip === host)?.[0];
    if (!id) throw new Error(`the droplet at ${host} stopped answering and no corten-bench droplet has that address to power-cycle`);
    console.log(`the droplet stopped answering in step ${step}: power-cycling droplet ${id}`);
    run("doctl", ["compute", "droplet-action", "power-cycle", id, "--wait"], { quiet: true, timeout: 10 * minute });
    if (!(await answers(60))) throw new Error(`droplet ${id} did not answer after a power cycle`);
    const next = steps[steps.indexOf(step) + 1];
    const note = `step ${step} skipped: the droplet stopped answering and was power-cycled`;
    console.log(next ? `${note}; going on from ${next}` : note);
    // A reboot stops every container: clear the apps and start the item API again.
    if (ssh(`echo "$(date -u +%Y-%m-%dT%H:%M:%S.000Z) ${note}" >> ${log}; docker ps -aq --filter name=fsw- | xargs -r docker rm -f >/dev/null; ${itemApi}`, { quiet: true, allowFail: true }).status !== 0) {
      console.log("could not clear the app containers or start the item API; the next steps will say what failed");
    }
    launch(next ? `node run-all.mjs ${round} 10 ${next}` : `sh -c 'node summarize.mjs ${round}; echo "$(date -u +%Y-%m-%dT%H:%M:%S.000Z) all done"'`, true);
    silentSince = null;
  }

  console.log("downloading the results");
  // host.txt: the CPU model and the rest, since droplets of one size are not all the same machine.
  const hostInfo = "(lscpu; free -m; uname -r; docker --version; node --version; npx playwright --version)";
  const download = async (remoteDir, localName) => {
    const local = join(here, "..", localName, "results");
    mkdirSync(local, { recursive: true });
    await retry(3, () => ssh(`cd ${remoteDir} && cp ${log} results/${round}/run.log && ${hostInfo} > results/${round}/host.txt 2>&1; tar -czf /tmp/results.tgz -C results ${round}`, { timeout: 5 * minute }));
    await retry(3, () => scp(`root@${host}:/tmp/results.tgz`, join(local, ".results.tgz")));
    run("tar", ["-xzf", join(local, ".results.tgz"), "-C", local], { timeout: 5 * minute });
    rmSync(join(local, ".results.tgz"));
    console.log(`results in ${join(local, round)}`);
  };
  await download(suiteDir, suite === "live" ? "live" : "fullstack");
  if (suite === "browser") await download("/bench/bench/browser", "browser");
} finally {
  destroy();
}
