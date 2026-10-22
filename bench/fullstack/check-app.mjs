// Checks one app against the reference before it joins a round: starts it with the fixture,
// compares the item page's visible text (parity.mjs), the JSON route's body with the reference's,
// and the 404s. The reference (corten by default) must already be running on its catalog port.
// Usage: node check-app.mjs <app> [reference=corten] [keep]
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { apps, network, runArgs } from "./apps.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const [name, reference = "corten", keep] = process.argv.slice(2);
const app = apps[name];
if (!app) throw new Error(`unknown app ${name}`);
const docker = (...args) => spawnSync("docker", args, { encoding: "utf8" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
docker("network", "create", network);
docker("rm", "-f", `fsw-${name}`);
const run = docker(...runArgs(name, app, "fixture", join(here, "fixtures")));
if (run.status !== 0) throw new Error(run.stderr);
const base = `http://localhost:${app.port}`;
const started = Date.now();
let ready = false;
while (!ready && Date.now() - started < 120_000) {
  try { ready = (await fetch(`${base}/items/item_1`)).ok; } catch {}
  if (!ready) await sleep(250);
}
if (!ready) {
  console.log(docker("logs", "--tail", "40", `fsw-${name}`).stderr);
  throw new Error(`${name} did not answer in 120 s`);
}
console.log(`${name} ready after ${Date.now() - started} ms`);
const failures = [];
const check = (label, ok, detail = "") => { console.log(`${ok ? "ok  " : "FAIL"} ${label}${detail ? ` (${detail})` : ""}`); if (!ok) failures.push(label); };
// Key order is not part of the JSON's meaning (Elixir maps, for one, do not keep it).
const canonical = (v) => Array.isArray(v) ? v.map(canonical) : v && typeof v === "object" ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, canonical(v[k])])) : v;
const parity = spawnSync("node", [join(here, "parity.mjs"), `${reference},${name}`, reference], { encoding: "utf8" });
check("item pages have the reference's visible text", parity.status === 0, parity.stdout.trim().split("\n").slice(-3).join(" | ").slice(0, 600));
const refBase = `http://localhost:${apps[reference].port}`;
for (const id of ["item_5", "item_17", "item_12345"]) {
  const [a, b] = await Promise.all([fetch(`${refBase}/api/items/${id}`), fetch(`${base}/api/items/${id}`)]);
  const [ja, jb] = [await a.json(), await b.json()];
  const same = JSON.stringify(canonical(ja)) === JSON.stringify(canonical(jb));
  check(`JSON for ${id} matches`, b.ok && (b.headers.get("content-type") ?? "").includes("application/json") && same, `${b.status} ${b.headers.get("content-type")}, ${(await (await fetch(`${base}/api/items/${id}`)).arrayBuffer()).byteLength} bytes`);
}
check("unknown item page is 404", (await fetch(`${base}/items/does-not-exist`)).status === 404);
check("unknown item JSON is 404", (await fetch(`${base}/api/items/does-not-exist`)).status === 404);
check("home page links item_5", (await (await fetch(`${base}/`)).text()).includes('href="/items/item_5"'));
if (!keep) docker("rm", "-f", `fsw-${name}`);
console.log(failures.length ? `${failures.length} failed` : "all passed");
process.exit(failures.length ? 1 : 0);
