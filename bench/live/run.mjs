// Live updates to many clients, one app at a time on one CPU and 1 GiB (as bench/fullstack).
// For each app and each number of clients: start the app, read its memory, connect the clients
// (client.mjs, in a container on the load cores), read the memory again, then publish RATE
// messages a second for SECONDS while sampling the app's CPU. Writes
// results/<round>/<app>-<clients>.json and prints one line per run.
// Usage: node run.mjs <round> [clients=1000,10000] [apps=all] [rate=10] [seconds=30]
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { appCpus, loadCpus, network } from "../fullstack/apps.mjs";
import { live } from "./apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const [round = "r1", levelsArg = "1000,10000", appsArg = Object.keys(live).join(","), rate = "10", seconds = "30"] = process.argv.slice(2);
const dir = join(here, "results", round);
mkdirSync(dir, { recursive: true });
const docker = (...args) => spawnSync("docker", args, { encoding: "utf8" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (msg) => console.log(new Date().toISOString(), msg);
const memMiB = (name) => {
  const text = docker("stats", "--no-stream", "--format", "{{.MemUsage}}", name).stdout.split("/")[0].trim();
  const m = /([\d.]+)\s*([KMG]i?B)/.exec(text);
  return m ? +(Number(m[1]) * { KiB: 1 / 1024, KB: 1 / 1024, MiB: 1, MB: 1, GiB: 1024, GB: 1024 }[m[2]]).toFixed(1) : null;
};

docker("network", "create", network);
for (const name of appsArg.split(",")) {
  const app = live[name];
  for (const clients of levelsArg.split(",").map(Number)) {
    const container = `lv-${name}`;
    docker("rm", "-f", container);
    await sleep(2000);
    const args = ["run", "-d", "--name", container, "--network", network, ...appCpus, "--memory", "1g", "--ulimit", "nofile=1048576:1048576", "-p", `${app.port}:8080`, "-e", "PORT=8080", "-e", "ITEM_SOURCE=none", "-e", "API_BASE=http://localhost:1"];
    for (const [k, v] of Object.entries(app.env ?? {})) args.push("-e", `${k}=${v}`);
    if (app.cmd) args.push("--entrypoint", app.cmd[0], app.image, ...app.cmd.slice(1));
    else args.push(app.image);
    const run = docker(...args);
    if (run.status !== 0) throw new Error(`${name}: ${run.stderr}`);
    let up = false;
    for (let i = 0; i < 1200 && !up; i += 1) {
      // Any answer means the server is listening (a GET to the publish route is a 404 or 405).
      try { await fetch(`http://localhost:${app.port}/live/publish`); up = true; } catch {}
      if (!up) await sleep(100);
    }
    if (!up) {
      // Apps run with their usual settings; one that cannot start in one CPU and 1 GiB fails the run.
      const logs = docker("logs", "--tail", "20", container);
      writeFileSync(join(dir, `${name}-${clients}.json`), JSON.stringify({ app: name, transport: app.transport, note: app.note, clients, startFailed: true, logs: (logs.stdout + logs.stderr).trim().split("\n") }, null, 1));
      log(`${name} did not start: ${logs.stderr}`);
      docker("rm", "-f", container);
      continue;
    }
    await sleep(5000);
    const idleMiB = memMiB(container);
    const out = `results/${round}/${name}-${clients}.client.json`;
    const stats = join(dir, `${name}-${clients}-stats.json`);
    const clientName = `lv-client-${name}`;
    docker("rm", "-f", clientName);
    const client = spawn("docker", ["run", "--rm", "--name", clientName, "--network", network, ...loadCpus, "--ulimit", "nofile=1048576:1048576", "-v", `${here}:/live`,
      "-e", `BASE=http://${container}:8080`, "-e", `STREAM=${app.path}`, "-e", `TRANSPORT=${app.transport}`, "-e", `WS_PORT=${app.wsPort ?? 8080}`,
      "-e", `PUBLISH=http://${container}:8080/live/publish`, "-e", `CLIENTS=${clients}`, "-e", `WORKERS=${process.env.BENCH_LIVE_WORKERS ?? 4}`, "-e", `RATE=${rate}`, "-e", `SECONDS=${seconds}`,
      "-e", `OUT=/live/${out}`, "node:24-bookworm-slim", "node", "/live/client.mjs"], { stdio: ["ignore", "pipe", "inherit"] });
    let connectedMiB = null;
    let sampler = null;
    let buffer = "";
    client.stdout.on("data", async (chunk) => {
      buffer += chunk;
      let end;
      while ((end = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, end);
        buffer = buffer.slice(end + 1);
        if (line.startsWith("CONNECTED")) { log(`${name} ${clients}: ${line}`); setTimeout(() => { connectedMiB = memMiB(container); }, 5000); }
        if (line.startsWith("PUBLISHING")) sampler = spawn("node", [join(here, "../fullstack/sample-stats.mjs"), stats, String(Number(seconds)), container], { stdio: "ignore" });
      }
    });
    // Connecting (3 min at most), settling, publishing (4x the time at most), delivery: then give up.
    const limit = setTimeout(() => { log(`${name} ${clients}: client over time, stopped`); docker("rm", "-f", clientName); }, (240 + Number(seconds) * 5 + 60) * 1000);
    const code = await new Promise((r) => client.on("exit", r));
    clearTimeout(limit);
    if (sampler && sampler.exitCode === null) await new Promise((r) => sampler.on("exit", r));
    const result = { app: name, transport: app.transport, note: app.note, clients, idleMiB, connectedMiB, clientExit: code };
    try { Object.assign(result, JSON.parse(readFileSync(join(here, out), "utf8"))); } catch {}
    try { const s = JSON.parse(readFileSync(stats, "utf8"))[container]; result.cpuPct = s?.cpuAvgPct ?? null; result.memMaxMiB = s?.memMaxMiB ?? null; } catch {}
    result.kibPerClient = idleMiB !== null && connectedMiB !== null && result.connected ? +(((connectedMiB - idleMiB) * 1024) / result.connected).toFixed(1) : null;
    result.oomKilled = docker("inspect", "-f", "{{.State.OOMKilled}}", container).stdout.trim() === "true";
    writeFileSync(join(dir, `${name}-${clients}.json`), JSON.stringify(result, null, 1));
    log(`${name} ${clients}: ${JSON.stringify(result)}`);
    docker("rm", "-f", container);
  }
}
log("done");
