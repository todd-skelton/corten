// Runs the whole comparison on a fresh DigitalOcean droplet with dedicated CPUs, then destroys it.
// A dedicated host keeps other work off the cores, pins each piece to its own cores, and exposes
// hardware counters for `count.mjs`. Needs `doctl` (authenticated), `ssh`, `scp` and `tar`.
// Usage (from the repository root or anywhere):
//   node bench/fullstack/droplet.mjs <round> --ssh-key <DO key id> [--size c-8] [--region sfo2]
//   node bench/fullstack/droplet.mjs <round> --host <ip> [--keep]   (reuse a droplet)
//   --from <step>: start run-all.mjs at that step and keep the droplet's earlier results.
//   --attach: with --host, only wait for the round already running there and download it.
//   --stall <minutes>: how long a droplet may not answer before it is power-cycled (default 15).
//   --idle <minutes>: how long the round's log may not grow before it is power-cycled (default 60;
//     the longest quiet stretch in r5-r8's logs was 26 minutes, in count.mjs).
//   --suite live: run bench/live (run.mjs, then summarize.mjs) instead; results in bench/live/results.
//   --suite browser: only the browser pass of this comparison (run-all.mjs from "browser": parity,
//     bytes, browser timing), then bench/browser's run-all.mjs; results in both results folders.
// Results land in bench/fullstack/results/<round>/. The droplet is destroyed when the run ends,
// fails or is interrupted, unless --keep (and never one passed with --host).
// If a full-stack round's droplet stops answering (r8's locked up in count.mjs, #466) or its log
// stops growing, it is power-cycled, the step it was in is skipped ("step <name> skipped" in
// run.log and skipped.txt, which summarize.mjs copies into summary.json) and the round goes on from
// the next step, at most twice. Otherwise, or after that, the run fails: it downloads what the
// round has first, then destroys the droplet.
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
// The round goes into droplet names, paths, shell commands and a pgrep pattern unquoted.
if (!/^[A-Za-z0-9][A-Za-z0-9_-]*$/.test(round)) throw new Error("the round is letters, digits, - and _ (r10)");
const suite = flag("suite", "fullstack");
if (!["fullstack", "live", "browser"].includes(suite)) throw new Error("--suite is fullstack, live or browser");
const suiteDir = `/bench/bench/${suite === "live" ? "live" : "fullstack"}`;
// The last line of a finished run: bench/browser's run-all.mjs prints its own after fullstack's.
const doneMarker = suite === "browser" ? /browser suite (done|finished)/ : /all done/;
const minute = 60_000;
const minutes = (name, fallback) => {
  const n = Number(flag(name, fallback));
  if (!Number.isFinite(n) || n <= 0) throw new Error(`--${name} is a number of minutes`);
  return n * minute;
};
const stall = minutes("stall", "15");
const idle = minutes("idle", "60");
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
// Windows' antivirus can hold a file it just saw written for a moment: a temporary file that cannot
// be removed is left behind rather than failing the round.
const remove = (file) => { try { rmSync(file, { force: true, maxRetries: 10, retryDelay: 500 }); } catch (e) { console.log(`could not remove ${file}: ${e.code}`); } };
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
let runTag = null;
// The corten-bench droplets as [id, name, ip]. Throws when DigitalOcean cannot list them, so no
// decision about which droplet is ours is made from an empty list.
const tagged = () => run("doctl", ["compute", "droplet", "list", "--tag-name", "corten-bench", "--format", "ID,Name,PublicIPv4", "--no-header"], { quiet: true, timeout: minute })
  .stdout.split("\n").map((l) => l.trim().split(/\s+/)).filter(([id]) => id);
const dropletAt = (ip) => { try { return tagged().find(([, , a]) => a === ip)?.[0] ?? null; } catch { return null; } };
const destroy = () => {
  if (dropletId && !args.includes("--keep")) {
    console.log(`destroying droplet ${dropletId}`);
    // Three tries a minute apart (synchronous: this also runs on SIGINT).
    let deleted = false;
    for (let i = 0; i < 3 && !deleted; i += 1) {
      if (i) Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, minute);
      deleted = run("doctl", ["compute", "droplet", "delete", dropletId, "--force"], { allowFail: true, timeout: minute }).status === 0;
    }
    if (!deleted) console.log(`could not delete droplet ${dropletId}: delete it with doctl compute droplet delete ${dropletId} --force`);
    else dropletId = null;
  }
  // The run's own tag, once its droplet is gone (best effort: an empty tag costs nothing).
  if (runTag && !dropletId && !args.includes("--keep")) {
    run("doctl", ["compute", "tag", "delete", runTag, "--force"], { quiet: true, allowFail: true, timeout: minute });
    runTag = null;
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
const quote = (text) => `'${text.replaceAll("'", "'\\''")}'`;
// Runs `script` on the droplet for at most `limit` minutes there, one such script at a time: a
// retry after a dropped connection waits for the attempt it replaces (or its time limit) rather
// than running beside it (two apt or docker builds at once).
const remote = (script, limit, options) => ssh(
  `flock -w ${limit * 60} /tmp/droplet.lock timeout -k 30 ${limit * 60} bash -c ${quote(script)}`,
  { timeout: (2 * limit + 2) * minute, ...options },
);
const answers = async (tries) => {
  for (let i = 0; i < tries; i += 1) {
    if (ssh("true", { quiet: true, allowFail: true, timeout: 30_000 }).status === 0) return true;
    await sleep(5000);
  }
  return false;
};
// run-all.mjs's steps, in its order, and the step a "<time> node <script> ..." line of its log runs
// ("<time> starting every app" begins the browser step, before parity.mjs).
const steps = ["cold-start", "fixture", "api", "count", "browser"];
const stepOf = (line) => {
  if (/^\S+ starting every app/.test(line)) return "browser";
  const m = /^\S+ node (\S+)\.mjs(?: \S+ (fixture|api)\b)?/.exec(line);
  if (!m) return null;
  if (m[1] === "run-ssr") return m[2];
  return { "cold-start": "cold-start", count: "count", parity: "browser", bytes: "browser", browser: "browser", summarize: "browser" }[m[1]] ?? null;
};

const log = `${suiteDir}/results-${round}.log`;
const itemApi = "(docker rm -f item-api >/dev/null 2>&1; docker run -d --name item-api --network rustbench --cpuset-cpus 2-3 -v /bench/bench/fullstack:/b:ro node:24-bookworm-slim node /b/item-api/server.mjs /b/fixtures/items.json >/dev/null)";
// The round's process: this suite's runner (run.mjs for live, run-all.mjs otherwise), or the
// summarize.mjs that ends a round whose last step was skipped. "([^0-9]|$)" keeps r9 from r90.
const running = `pgrep -f '(${suite === "live" ? "run" : "run-all"}|summarize)\\.mjs ${round}([^0-9]|$)' >/dev/null`;
// Starts `command` in the background on the droplet, detached, so a dropped connection does not
// stop it (`;`, not `&&`: only the command goes to the background, so ssh returns at once). Not
// retried blindly: if the call fails, the round counts as started when its process is there.
const launch = (command, append) => {
  if (ssh(`cd ${suiteDir}; ${env} nohup ${command} ${append ? ">>" : ">"} ${log} 2>&1 < /dev/null &`, { allowFail: true }).status === 0) return;
  if (ssh(running, { quiet: true, allowFail: true }).status === 0) return;
  throw new Error(`could not start the round on ${host}`);
};
// host.txt: the CPU model and the rest, since droplets of one size are not all the same machine.
const hostInfo = "(lscpu; free -m; uname -r; docker --version; node --version; npx playwright --version)";
const download = async (remoteDir, localName, label) => {
  const local = join(here, "..", localName, "results");
  mkdirSync(local, { recursive: true });
  await retry(3, () => remote(`cd ${remoteDir} && mkdir -p results/${round} && cp ${log} results/${round}/run.log && ${hostInfo} > results/${round}/host.txt 2>&1; tar -czf /tmp/results.tgz -C results ${round}`, 5));
  await retry(3, () => scp(`root@${host}:/tmp/results.tgz`, join(local, ".results.tgz")));
  run("tar", ["-xzf", join(local, ".results.tgz"), "-C", local], { timeout: 5 * minute });
  remove(join(local, ".results.tgz"));
  console.log(`${label} in ${join(local, round)}`);
};
const downloadAll = async (label) => {
  await download(suiteDir, suite === "live" ? "live" : "fullstack", label);
  if (suite === "browser") await download("/bench/bench/browser", "browser", label);
};

let launched = false;
try {
  if (!host) {
    const key = flag("ssh-key");
    if (!key) throw new Error("--ssh-key <DigitalOcean key id> is required to create a droplet");
    const name = `corten-bench-${suite === "fullstack" ? "" : `${suite}-`}${round}`;
    // A tag only this run's droplet has: after a failed create, a droplet carrying it is certainly
    // this run's, whatever else was created meanwhile.
    runTag = `corten-bench-run-${Date.now().toString(36)}-${process.pid}`;
    // A droplet already named so (a --keep run's) is never reused or, below, mistaken for this one.
    const before = tagged();
    if (before.some(([, n]) => n === name)) throw new Error(`a corten-bench droplet named ${name} already exists: delete it or pick another round`);
    const created = run("doctl", ["compute", "droplet", "create", name, "--size", flag("size", "c-8"), "--region", flag("region", "sfo2"),
      "--image", "ubuntu-24-04-x64", "--ssh-keys", key, "--tag-names", `corten-bench,${runTag}`, "--wait", "--format", "ID,PublicIPv4", "--no-header"], { quiet: true, allowFail: true, timeout: 10 * minute });
    if (created.status !== 0) {
      // The droplet may exist even though the call failed or timed out: find it so finally deletes it.
      const ours = run("doctl", ["compute", "droplet", "list", "--tag-name", runTag, "--format", "ID", "--no-header"], { quiet: true, allowFail: true, timeout: minute });
      const ids = ours.status === 0 ? ours.stdout.split("\n").map((l) => l.trim()).filter(Boolean) : [];
      if (ids.length === 1) dropletId = ids[0];
      else if (ours.status !== 0) console.log(`check for a droplet left by this run: doctl compute droplet list --tag-name ${runTag}`);
      throw new Error(`could not create droplet ${name}: ${String(created.error?.message ?? created.stderr).trim()}`);
    }
    [dropletId, host] = created.stdout.trim().split(/\s+/);
    // --wait can return before the droplet has its public address (seen on 2026-10-31).
    for (let i = 0; i < 30 && !host; i += 1) {
      await sleep(10_000);
      host = run("doctl", ["compute", "droplet", "get", dropletId, "--format", "PublicIPv4", "--no-header"], { quiet: true, allowFail: true, timeout: minute }).stdout?.trim();
    }
    if (!host) throw new Error(`droplet ${dropletId} has no public address after five minutes`);
    console.log(`droplet ${dropletId} at ${host}`);
    await answers(30);
  }

  const attach = args.includes("--attach");
  if (attach && !flag("host")) throw new Error("--attach needs --host");
  if (!attach) {
    console.log("provisioning");
    await retry(2, () => remote([
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
    ].join(" && "), 30));

    console.log("uploading the working tree");
    const tarball = join(here, "results", ".upload.tgz");
    mkdirSync(dirname(tarball), { recursive: true });
    run("tar", ["-czf", tarball, "--exclude=target", "--exclude=node_modules", "--exclude=./bench/fullstack/results", "--exclude=./bench/live/results", "--exclude=./bench/browser/results", "--exclude=./bench/browser/out", "--exclude=./.git", "-C", repo, "."], { timeout: 5 * minute });
    const from = flag("from");
    await retry(3, () => remote(from ? "mkdir -p /bench" : "rm -rf /bench && mkdir -p /bench", 2));
    await retry(3, () => scp(tarball, `root@${host}:/bench/src.tgz`));
    remove(tarball);

    console.log("building the images");
    const fullstackImages = (names) => names.filter((app) => app !== "corten").map((app) => `docker build -q -t cb-${app} bench/fullstack/apps/${app}`);
    await retry(2, () => remote(suite === "live"
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
      ].join(" && "), 90));

    launched = true;
    console.log(`running round ${round} (${{ live: "about an hour", browser: "about an hour", fullstack: "about six hours for fourteen apps" }[suite]})`);
    launch({
      live: `sh -c 'node run.mjs ${round}; node summarize.mjs ${round}; echo all done'`,
      browser: `sh -c '. "$HOME/.cargo/env"; node run-all.mjs ${round} 10 browser; cd /bench/bench/browser && node run-all.mjs ${round}'`,
      fullstack: `node run-all.mjs ${round} 10 ${from ?? ""}`,
    }[suite], Boolean(from));
  }
  launched = true;
  let shown = 0;
  let lastLog = "";
  let changedAt = Date.now();
  let quietPolls = 0;
  let silentSince = null;
  let silentPolls = 0;
  let floor = 0;
  let bootId = null;
  let recoveries = 0;
  let gone = 0;
  for (;;) {
    await sleep(minute);
    const r = ssh(`cat /proc/sys/kernel/random/boot_id; cat ${log}; ${running} || echo __stopped__`, { quiet: true, allowFail: true });
    let why = null;
    if (r.status === 0) {
      silentSince = null;
      silentPolls = 0;
      bootId = r.stdout.split("\n")[0];
      const text = r.stdout.split("\n").slice(1).join("\n").replace("__stopped__\n", "");
      if (text !== lastLog) { changedAt = Date.now(); quietPolls = 0; } else quietPolls += 1;
      lastLog = text;
      // Two polls in a row with no round process and no last line: it died without finishing.
      gone = r.stdout.includes("__stopped__") && !doneMarker.test(text) ? gone + 1 : 0;
      if (gone >= 2) throw new Error(`the round's process on ${host} stopped before it finished; its log is ${log}`);
      const lines = text.split("\n").filter((l) => /^\d{4}-|step failed|step \S+ skipped|all done|browser suite|^\|/.test(l));
      for (const l of lines.slice(shown)) console.log(l);
      shown = lines.length;
      if (doneMarker.test(text)) break;
      // The droplet answers but the round has written nothing for `idle` (k6, perf or a browser
      // hung): treated like a droplet that stopped answering. Counted in polls too, so a PC that
      // slept through the round does not count its sleep.
      if (suite !== "fullstack" || Date.now() - changedAt < idle || quietPolls < idle / minute / 2) continue;
      why = `its log has not grown for ${Math.round((Date.now() - changedAt) / minute)} minutes`;
    } else {
      silentSince ??= Date.now();
      silentPolls += 1;
      if (Date.now() - silentSince < stall || silentPolls < 3) continue;
      // A network problem here looks the same as a hung droplet: only a droplet that DigitalOcean
      // can be reached past is restarted.
      if (run("doctl", ["account", "get"], { quiet: true, allowFail: true, timeout: minute }).status !== 0) {
        // Not forever: after four hours the run fails (its download and delete will likely fail too,
        // and say so).
        if (Date.now() - silentSince > 4 * 60 * minute) throw new Error(`neither the droplet at ${host} nor DigitalOcean has answered for four hours`);
        console.log("neither the droplet nor DigitalOcean answers from here: waiting for the network");
        continue;
      }
      why = `it has not answered for ${Math.round((Date.now() - silentSince) / minute)} minutes`;
    }
    // Restart the droplet and go on without the step it was in.
    const fromLog = lastLog.split("\n").map(stepOf).filter(Boolean).at(-1);
    const step = steps[Math.max(floor, steps.indexOf(fromLog))];
    if (suite !== "fullstack" || !fromLog || recoveries >= 2) {
      throw new Error(`the round on ${host} is stuck: ${why}${fromLog ? ` (in step ${step})` : ""}`);
    }
    recoveries += 1;
    const id = dropletId ?? dropletAt(host);
    if (!id) throw new Error(`the round on ${host} is stuck (${why}) and no corten-bench droplet has that address to power-cycle`);
    console.log(`the round is stuck in step ${step}, ${why}: power-cycling droplet ${id}`);
    const cycled = run("doctl", ["compute", "droplet-action", "power-cycle", id, "--wait"], { quiet: true, allowFail: true, timeout: 10 * minute });
    if (!(await answers(60))) throw new Error(`droplet ${id} did not answer after a power cycle`);
    // Only a droplet that restarted (a new boot id, so no process of the round is left) gets the
    // round again.
    const up = ssh(`cat /proc/sys/kernel/random/boot_id; ${running} && echo __running__; true`, { quiet: true, allowFail: true });
    const newBoot = up.stdout?.split("\n")[0];
    if (cycled.status !== 0 || up.status !== 0 || !bootId || newBoot === bootId || up.stdout.includes("__running__")) {
      console.log(`droplet ${id} did not restart (power cycle ${cycled.status === 0 ? "reported done" : "failed"}): watching the round again`);
      silentSince = null;
      silentPolls = 0;
      changedAt = Date.now();
      quietPolls = 0;
      continue;
    }
    const next = steps[steps.indexOf(step) + 1];
    const note = `step ${step} skipped: ${why}, so the droplet was power-cycled`;
    console.log(next ? `${note}; going on from ${next}` : note);
    // A reboot stops every container: clear the apps and start the item API again. skipped.txt
    // tells summarize.mjs (and so summary.json) which step's results are partly or wholly missing.
    const stamp = "$(date -u +%Y-%m-%dT%H:%M:%S.000Z)";
    // Under remote()'s lock and time limit, so a retry never runs beside an attempt still going,
    // and the next step starts only once all of it worked. The skip is written once.
    const skipped = `${suiteDir}/results/${round}/skipped.txt`;
    await retry(3, () => remote([
      `mkdir -p ${suiteDir}/results/${round}`,
      `(grep -qF ${quote(note)} ${skipped} 2>/dev/null || echo "${stamp} ${note}" | tee -a ${log} >> ${skipped})`,
      "(docker ps -aq --filter name=fsw- | xargs -r docker rm -f >/dev/null)",
      itemApi,
      `grep -qF ${quote(note)} ${skipped}`,
    ].join(" && "), 5, { quiet: true }));
    launch(next ? `node run-all.mjs ${round} 10 ${next}` : `sh -c 'node summarize.mjs ${round}; echo "${stamp} all done"'`, true);
    floor = next ? steps.indexOf(next) : steps.length - 1;
    silentSince = null;
    silentPolls = 0;
    changedAt = Date.now();
    quietPolls = 0;
    gone = 0;
  }

  console.log("downloading the results");
  await downloadAll("results");
} catch (e) {
  // A round that ran and then failed keeps what it measured: download it before the droplet goes.
  if (launched) {
    console.log(`the round failed (${String(e.message).split("\n")[0]}); downloading what it has`);
    try { await downloadAll("partial results (the round did not finish)"); } catch (d) {
      console.log(`could not download them: ${String(d.message).split("\n")[0]}`);
    }
  }
  throw e;
} finally {
  destroy();
}
