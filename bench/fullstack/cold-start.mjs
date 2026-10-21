// Cold start: from the moment Docker starts the server process to its first 200 for an item page
// (API mode, so no fixture parsing), measured inside the container's network namespace so Docker
// Desktop's port proxy is not part of the number. A probe container shares the namespace and
// polls 127.0.0.1:8080 before the app starts; both timestamps come from the Docker VM's clock.
// A `baseline` entry (busybox httpd serving a static file) measures Docker's own start overhead,
// which is in every number. Also records memory right after the first request and the image size.
// Usage: node cold-start.mjs <round> [iterations=10] [apps=all]
import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { apiBase, apps as catalog, network } from "./apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const [round = "r1", iterations = "10", appsArg = ["baseline", ...Object.keys(catalog)].join(",")] = process.argv.slice(2);
const baseline = {
  image: "busybox:musl",
  cmd: ["sh", "-c", "mkdir -p /w/items && echo ok > /w/items/item_1 && exec httpd -f -p 8080 -h /w"],
};
const docker = (...args) => spawnSync("docker", args, { encoding: "utf8" });
const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const holder = "cs-holder";
// bash's /dev/tcp and GNU date (nanoseconds) from debian:bookworm-slim, so no HTTP client is needed.
const probeImage = "debian:bookworm-slim";
const probe = [
  "until { exec 3<>/dev/tcp/127.0.0.1/8080 && printf 'GET /items/item_1 HTTP/1.0\\r\\nHost: localhost\\r\\n\\r\\n' >&3 && read -r line <&3 && [[ $line == *' 200 '* ]]; } 2>/dev/null; do exec 3>&- 2>/dev/null; done",
  "date +%s%N",
].join("; ");
const out = {};

for (const name of appsArg.split(",")) {
  const app = name === "baseline" ? baseline : catalog[name];
  const firstOk = [];
  const mem = [];
  for (let i = 0; i < Number(iterations); i += 1) {
    docker("rm", "-f", "cs-app", "cs-probe", holder);
    docker("run", "-d", "--name", holder, "--network", network, probeImage, "sleep", "600");
    const create = docker("create", "--name", "cs-app", "--network", `container:${holder}`, "--cpus", "1", "--memory", "1g", "-e", `API_BASE=${apiBase}`, "-e", "PORT=8080", app.image, ...(app.cmd ?? []));
    if (create.status !== 0) throw new Error(`${name}: ${create.stderr}`);
    docker("run", "-d", "--name", "cs-probe", "--network", `container:${holder}`, probeImage, "bash", "-c", probe);
    docker("start", "cs-app");
    const wait = docker("wait", "cs-probe");
    if (wait.stdout.trim() !== "0") throw new Error(`${name}: probe failed`);
    const okNs = BigInt(docker("logs", "cs-probe").stdout.trim());
    const started = docker("inspect", "cs-app", "--format", "{{.State.StartedAt}}").stdout.trim();
    // RFC 3339 with nanoseconds: keep the nanoseconds Date would drop.
    const [, base, frac = "0"] = /^(.*?:\d\d)(?:\.(\d+))?Z$/.exec(started);
    const startNs = BigInt(Date.parse(`${base}Z`)) * 1_000_000n + BigInt(frac.padEnd(9, "0").slice(0, 9));
    const ms = Number(okNs - startNs) / 1e6;
    firstOk.push(ms);
    const usage = docker("stats", "--no-stream", "--format", "{{.MemUsage}}", "cs-app").stdout.trim().split("/")[0].trim();
    mem.push(usage);
    console.log(name, i, `first 200 ${ms.toFixed(1)} ms after process start, ${usage}`);
  }
  docker("rm", "-f", "cs-app", "cs-probe", holder);
  const size = Number(docker("image", "inspect", app.image, "--format", "{{.Size}}").stdout.trim());
  out[name] = {
    firstOkMs: { median: +median(firstOk).toFixed(1), min: +Math.min(...firstOk).toFixed(1), max: +Math.max(...firstOk).toFixed(1) },
    memAfterFirstRequest: mem[Math.floor(mem.length / 2)],
    imageMB: +(size / 1e6).toFixed(0),
  };
}
mkdirSync(join(here, "results", round), { recursive: true });
writeFileSync(join(here, "results", round, "cold-start.json"), JSON.stringify(out, null, 1));
console.log(JSON.stringify(out, null, 1));
