// Cold start: from the moment the server process starts to its first 200 for an item page (items
// from the item API, so no fixture parsing). Docker's own container setup is not in the number:
// the container runs a shell that writes the time, then execs the server; a probe in another
// container on the same network namespace polls 127.0.0.1:8080 and writes the time of the first
// 200. Both clocks are the host kernel's. Also records memory right after the first request and
// the image size.
// Usage: node cold-start.mjs <round> [iterations=10] [apps=all]
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { apiBase, appCpus, apps as catalog, loadCpus, network } from "./apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const [round = "r1", iterations = "10", appsArg = Object.keys(catalog).join(",")] = process.argv.slice(2);
const docker = (...args) => spawnSync("docker", args, { encoding: "utf8" });
const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const holder = "cs-holder";
const clock = "cs-clock";
// bash's /dev/tcp and GNU date (nanoseconds) from debian:bookworm-slim, so no HTTP client is needed.
const probeImage = "debian:bookworm-slim";
const probe = [
  "until { exec 3<>/dev/tcp/127.0.0.1/8080 && printf 'GET /items/item_1 HTTP/1.0\\r\\nHost: localhost\\r\\n\\r\\n' >&3 && read -r line <&3 && [[ $line == *' 200 '* ]]; } 2>/dev/null; do exec 3>&- 2>/dev/null; done",
  "date +%s%N",
].join("; ");
// Merged into the round's earlier results, so one app can be measured again on its own.
const outFile = join(here, "results", round, "cold-start.json");
const out = existsSync(outFile) ? JSON.parse(readFileSync(outFile, "utf8")) : {};

/** The image's command, without an entrypoint wrapper script (node images' docker-entrypoint.sh only execs it). */
function command(image) {
  const config = JSON.parse(docker("image", "inspect", image, "--format", "{{json .Config}}").stdout);
  const entry = (config.Entrypoint ?? []).filter((part) => part !== "docker-entrypoint.sh");
  return [...entry, ...(config.Cmd ?? [])];
}

for (const name of appsArg.split(",")) {
  const app = catalog[name];
  const firstOk = [];
  const mem = [];
  for (let i = 0; i < Number(iterations); i += 1) {
    docker("rm", "-f", "cs-app", "cs-probe", holder);
    docker("volume", "rm", "-f", clock);
    // Writable by an image that runs as another user (Rails runs as uid 1000).
    docker("run", "--rm", "-v", `${clock}:/clock`, probeImage, "chmod", "777", "/clock");
    docker("run", "-d", "--name", holder, "--network", network, ...loadCpus, probeImage, "sleep", "600");
    const create = docker(
      "create", "--name", "cs-app", "--network", `container:${holder}`, ...appCpus, "--memory", "1g",
      "-v", `${clock}:/clock`, "-e", `API_BASE=${apiBase}`, "-e", "PORT=8080", "--entrypoint", "sh",
      app.image, "-c", 'date +%s%N > /clock/start; exec "$@"', "sh", ...command(app.image),
    );
    if (create.status !== 0) throw new Error(`${name}: ${create.stderr}`);
    docker("run", "-d", "--name", "cs-probe", "--network", `container:${holder}`, ...loadCpus, probeImage, "bash", "-c", probe);
    docker("start", "cs-app");
    const wait = docker("wait", "cs-probe");
    if (wait.stdout.trim() !== "0") throw new Error(`${name}: probe failed`);
    const okNs = BigInt(docker("logs", "cs-probe").stdout.trim());
    const startNs = BigInt(docker("run", "--rm", "-v", `${clock}:/clock`, probeImage, "cat", "/clock/start").stdout.trim());
    const ms = Number(okNs - startNs) / 1e6;
    firstOk.push(ms);
    const usage = docker("stats", "--no-stream", "--format", "{{.MemUsage}}", "cs-app").stdout.trim().split("/")[0].trim();
    mem.push(usage);
    console.log(name, i, `first 200 ${ms.toFixed(1)} ms after the server process started, ${usage}`);
  }
  docker("rm", "-f", "cs-app", "cs-probe", holder);
  docker("volume", "rm", "-f", clock);
  const size = Number(docker("image", "inspect", app.image, "--format", "{{.Size}}").stdout.trim());
  out[name] = {
    firstOkMs: { median: +median(firstOk).toFixed(1), min: +Math.min(...firstOk).toFixed(1), max: +Math.max(...firstOk).toFixed(1) },
    memAfterFirstRequest: mem[Math.floor(mem.length / 2)],
    imageMB: +(size / 1e6).toFixed(0),
  };
}
mkdirSync(join(here, "results", round), { recursive: true });
writeFileSync(outFile, JSON.stringify(out, null, 1));
console.log(JSON.stringify(out, null, 1));
