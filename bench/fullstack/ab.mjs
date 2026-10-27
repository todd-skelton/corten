// Before/after for one app on one CPU, items from memory: node ab.mjs <out-dir> <image-a> <image-b>
// For each image and Accept-Encoding: a fixed 300 req/s (CPU use at the same work) and 64 users
// (throughput). Images run one at a time, alternating, on the same port.
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { k6User, network } from "./apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const [out = "results/ab", ...images] = process.argv.slice(2);
const dir = join(here, out);
mkdirSync(dir, { recursive: true });
const docker = (...args) => spawnSync("docker", args, { encoding: "utf8" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const encodings = { identity: "identity", browser: "gzip, deflate, br, zstd" };
const rows = [];

for (const round of [1, 2]) {
  for (const image of images) {
    docker("rm", "-f", "ab-app");
    const run = docker("run", "-d", "--name", "ab-app", "--network", network, "--cpus", "1", "--memory", "1g", "-v", `${join(here, "fixtures")}:/fixtures:ro`, "-e", "ITEM_SOURCE=fixture", "-e", "FIXTURE_PATH=/fixtures/items.json", "-e", "PORT=8080", "-e", "HOSTS=ab-app", image);
    if (run.status !== 0) throw new Error(run.stderr);
    await sleep(4000);
    for (const [encName, enc] of Object.entries(encodings)) {
      for (const [phase, env] of [["warmup", { MODE: "closed", VUS: "32", DURATION: "10s" }], ["fixed300", { MODE: "open", RATE: "300", DURATION: "40s" }], ["vus64", { MODE: "closed", VUS: "64", DURATION: "40s" }]]) {
        const name = `${image}-${encName}-${phase}-r${round}`;
        const stats = join(dir, `${name}-stats.json`);
        const sampler = phase === "warmup" ? null : spawn("node", [join(here, "sample-stats.mjs"), stats, "42", "ab-app"], { stdio: "ignore", windowsHide: true });
        const args = ["run", "--rm", "--network", network, "--cpus", "4", ...k6User, "-v", `${here}:/bench`, "-e", "BASE=http://ab-app:8080", "-e", `ENC=${enc}`];
        for (const [k, v] of Object.entries(env)) args.push("-e", `${k}=${v}`);
        args.push("grafana/k6:latest", "run", "--quiet", "--summary-export", `/bench/${out}/${name}.json`, "/bench/ab-load.js");
        docker(...args);
        if (sampler) await new Promise((r) => (sampler.exitCode !== null ? r() : sampler.on("exit", r)));
        if (phase === "warmup") continue;
        const k6 = JSON.parse(readFileSync(join(dir, `${name}.json`), "utf8"));
        const d = k6.metrics.http_req_duration;
        const cpu = JSON.parse(readFileSync(stats, "utf8"))["ab-app"]?.cpuAvgPct ?? null;
        const bytes = k6.metrics.data_received.count / k6.metrics.http_reqs.count;
        const row = { round, image, enc: encName, phase, rps: Math.round(k6.metrics.http_reqs.rate), p50: +d["p(50)"].toFixed(2), p99: +d["p(99)"].toFixed(2), failed: k6.metrics.checks?.fails ?? 0, cpu, bytesPerResponse: Math.round(bytes) };
        rows.push(row);
        console.log(JSON.stringify(row));
      }
    }
    docker("rm", "-f", "ab-app");
  }
}
writeFileSync(join(dir, "summary.json"), JSON.stringify(rows, null, 1));
