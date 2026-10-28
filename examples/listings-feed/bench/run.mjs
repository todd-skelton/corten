// Cross-instance feed failover benchmark. Starts Postgres, NATS (JetStream), instances a and b
// (1 CPU / 512 MiB each, fixed IPs) and an nginx load balancer on the feedbench network; then runs
// failover.mjs in a node container (it kills and restarts instances on schedule) while a real
// browser follows the same feed with corten.js through the load balancer.
// Usage (from the repository root): node examples/listings-feed/bench/run.mjs [out=failover-<date>.json]
//   [clients=200] [wps=50] [n=5] [seconds=80] [keep=1]
// Needs the image: docker build -f examples/listings-feed/Dockerfile -t corten-listings-feed .
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const here = dirname(fileURLToPath(import.meta.url));
const args = Object.fromEntries(process.argv.slice(2).map((a) => a.split("=")));
const out = join(here, "..", "results", args.out ?? `failover-${new Date().toISOString().slice(0, 10)}.json`);
const env = { CLIENTS: args.clients ?? "200", WPS: args.wps ?? "50", N: args.n ?? "5", SECONDS: args.seconds ?? "80", WORKERS: args.workers ?? "4" };
if (args.schedule) env.SCHEDULE = args.schedule;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const docker = (...a) => {
  const r = spawnSync("docker", a, { encoding: "utf8" });
  const optional = a[0] === "rm" || (a[0] === "network" && a[1] === "rm");
  if (r.status !== 0 && !optional) throw new Error(`docker ${a.join(" ")}: ${r.stderr}`);
  return r.stdout.trim();
};
const containers = ["fb-lb", "fb-a", "fb-b", "fb-nats", "fb-pg"];
const net = ["--network", "feedbench"];
const db = "postgres://postgres:postgres@10.77.77.10/feeds";
const logs = (name) => {
  const r = spawnSync("docker", ["logs", name], { encoding: "utf8" });
  return `${r.stdout}${r.stderr}`;
};

async function until(what, test, ms = 60_000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (await test()) return;
    await sleep(250);
  }
  throw new Error(`timed out waiting for ${what}`);
}

function instance(name, ip, letter) {
  docker("run", "-d", "--name", name, ...net, "--ip", ip, "--cpus", "1", "--memory", "512m",
    "-e", `INSTANCE=${letter}`, "-e", `DATABASE_URL=${db}`, "-e", "NATS_URL=nats://10.77.77.11:4222", "corten-listings-feed");
}

for (const c of containers) docker("rm", "-f", c);
docker("network", "rm", "feedbench");
docker("network", "create", "--subnet", "10.77.77.0/24", "feedbench");
docker("run", "-d", "--name", "fb-pg", ...net, "--ip", "10.77.77.10", "-e", "POSTGRES_PASSWORD=postgres", "-e", "POSTGRES_DB=feeds", "postgres:16-alpine");
docker("run", "-d", "--name", "fb-nats", ...net, "--ip", "10.77.77.11", "nats:2-alpine", "-js");
await until("postgres", () => spawnSync("docker", ["exec", "fb-pg", "pg_isready", "-U", "postgres", "-d", "feeds"]).status === 0);
// a starts first, so it is the active relay when it is killed.
instance("fb-a", "10.77.77.21", "a");
await until("a", () => /relay a: active/.test(logs("fb-a")));
instance("fb-b", "10.77.77.22", "b");
await until("b", () => /listening/.test(logs("fb-b")));
docker("run", "-d", "--name", "fb-lb", ...net, "--ip", "10.77.77.30", "-p", "18420:8080", "-v", `${join(here, "nginx.conf")}:/etc/nginx/nginx.conf:ro`, "nginx:1.27-alpine");
await until("load balancer", async () => (await fetch("http://localhost:18420/health/ready").catch(() => null))?.ok);

// A real browser on the seller's page (FeedMonitor, corten.js subscribe()), through the load
// balancer, for the whole run.
const browser = await chromium.launch();
const page = await browser.newPage();
const pageErrors = [];
page.on("pageerror", (e) => pageErrors.push(String(e)));
await page.goto("http://localhost:18420/sellers/s1");
/** The monitor's outputs by name: status, listings, through, changes, snapshots. */
const shown = () =>
  page.evaluate(() => Object.fromEntries([...document.querySelectorAll("output[name]")].map((o) => [o.getAttribute("name"), o.textContent ?? ""])));
await until("the page's feed", async () => (await shown()).status === "live");

const envArgs = Object.entries(env).flatMap(([k, v]) => ["-e", `${k}=${v}`]);
const client = spawn("docker", ["run", "--rm", ...net, "--ulimit", "nofile=65536:65536", "-v", "/var/run/docker.sock:/var/run/docker.sock",
  "-v", `${here}:/bench`, ...envArgs, "node:24-bookworm-slim", "node", "/bench/failover.mjs"], { stdio: ["ignore", "pipe", "inherit"] });
let stdout = "";
client.stdout.on("data", (d) => (stdout += d));
const code = await new Promise((r) => client.on("exit", r));
if (code !== 0) throw new Error(`failover.mjs exited ${code}`);
const result = JSON.parse(stdout.trim().split("\n").at(-1));

// subscribe() never skips a sequence: a gap loads a new snapshot. So the page has lost nothing once
// it reaches the committed sequence, and each snapshot after the first is a resync.
const committed = result.committed;
await until("the page to reach the committed sequence", async () => Number((await shown()).through) >= committed, 20_000).catch(() => {});
const seen = await shown();
const probe = { status: seen.status, patches: Number(seen.changes), snapshots: Number(seen.snapshots), last: Number(seen.through) };
await browser.close();
result.browser = { ...probe, committed, resyncs: Math.max(0, probe.snapshots - 1), lost: Math.max(0, committed - probe.last), pageErrors };
result.instanceLogs = Object.fromEntries(
  ["fb-a", "fb-b"].map((c) => {
    const r = spawnSync("docker", ["logs", "-t", c], { encoding: "utf8" });
    return [c, (r.stdout + r.stderr).split("\n").filter((l) => /relay|hub|listening/.test(l))];
  }),
);
// Relay takeover: from killing the active relay to the other instance logging "relay: active".
const activations = Object.entries(result.instanceLogs).flatMap(([c, lines]) =>
  lines.filter((l) => /relay \w+: active/.test(l)).map((l) => ({ c, at: Date.parse(l.slice(0, l.indexOf(" "))) - result.t0 })),
);
result.relayTakeoverMs = result.events
  .filter((e) => e.action === "kill")
  .map((e) => {
    const next = activations.filter((a) => a.c !== e.name && a.at > e.at).sort((x, y) => x.at - y.at)[0];
    const wasLeader = activations.filter((a) => a.at < e.at).sort((x, y) => y.at - x.at)[0]?.c === e.name;
    return { killed: e.name, wasLeader, takeoverMs: wasLeader && next ? next.at - e.at : null };
  });
result.stats = Object.fromEntries(["fb-a", "fb-b", "fb-pg", "fb-nats", "fb-lb"].map((c) => [c, docker("stats", "--no-stream", "--format", "{{.CPUPerc}} {{.MemUsage}}", c)]));
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, JSON.stringify(result, null, 1));

const d = result.delivery;
const l = result.latencyMs;
const line = (s) => `p50 ${s.p50} p95 ${s.p95} p99 ${s.p99} max ${s.max} ms (n ${s.n})`;
console.log(`events: ${result.events.map((e) => `${(e.at / 1000).toFixed(1)}s ${e.action} ${e.name}`).join(", ")}`);
console.log(`load generator: ${result.workers} threads, event loop delay p99 ${result.loadGenerator.loopDelayP99Ms.toFixed(1)} max ${result.loadGenerator.loopDelayMaxMs.toFixed(1)} ms`);
console.log(`writes: ${result.writes.ok} ok (${result.writes.replayed} answered from their idempotency key), ${result.writes.retries} retries, ${result.writes.failed} failed after 3 attempts, extra commits ${result.writes.extraCommits}, ${result.writes.skipped} skipped at 64 in flight; commit ${line(result.writes.commitMs)}; committed seq ${committed}`);
console.log(`delivery: ${d.patchesReceived} patches + ${d.coveredBySnapshot} covered by snapshots of ${d.patchesExpected} expected; lost ${d.lost} (${d.clientsWithLoss} clients), out of order ${d.outOfOrder}, redelivered ${d.redelivered}, duplicates dropped ${d.duplicatesDropped}, extra snapshots ${d.extraSnapshots}, syncs ${d.syncs.n}, reconnects ${d.reconnects}, http errors ${d.httpErrors}, network errors ${d.networkErrors}`);
console.log(`commit -> client, steady: ${line(l.commitToClient.steady)}`);
console.log(`commit -> client, within 5 s of a kill: ${line(l.commitToClient.within5sOfKill)}`);
console.log(`write -> client, steady: ${line(l.writeToClient.steady)}`);
console.log(`write -> client, within 5 s of a kill: ${line(l.writeToClient.within5sOfKill)}`);
console.log(`outages: ${result.outages.n} (${result.outages.resynced} resynced); reconnect ${line(result.outages.reconnectMs)}; silent ${line(result.outages.silentMs)}`);
console.log(`relay takeover: ${result.relayTakeoverMs.map((r) => `${r.killed} ${r.wasLeader ? `${r.takeoverMs} ms` : "(not the relay)"}`).join(", ")}`);
console.log(`browser: ${probe.patches} patches, ${probe.snapshots} snapshot(s) (${result.browser.resyncs} resyncs), last ${probe.last}/${committed}, lost ${result.browser.lost}, status ${probe.status}, errors ${pageErrors.length}`);
console.log(`wrote ${out}`);
if (!args.keep) for (const c of containers) docker("rm", "-f", c);
if (!args.keep) docker("network", "rm", "feedbench");
