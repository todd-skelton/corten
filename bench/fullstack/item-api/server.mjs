// A stand-in for the item API on a bench host: the fixture's API responses (about 55 KB of JSON
// each) over HTTP at the real API's path, serialized once at startup. The apps still fetch and
// parse every item; there is no database behind it, so it measures the same work as the
// production API with a warm cache.
// It runs one server process per core it may use (two on a bench droplet, cores 2-3), each
// listening on the port with SO_REUSEPORT, so the kernel spreads connections across them and no
// process hands connections on: one Node process used 39-40% of a core for Corten's item-API
// runs in r6 and would have capped an app about 2.6x faster.
// Usage: node server.mjs <items.json> [port=8080] [processes=available cores]
import { fork } from "node:child_process";
import { readFileSync } from "node:fs";
import { createServer } from "node:http";
import { availableParallelism } from "node:os";
import { fileURLToPath } from "node:url";

const [file = "/fixtures/items.json", port = "8080", processes = String(availableParallelism())] = process.argv.slice(2);
const count = Math.max(1, Math.floor(Number(processes)) || 1);

if (count > 1 && !process.env.ITEM_API_SERVER) {
  // The first process only starts the servers, and stops when one of them stops.
  let ready = 0;
  for (let i = 0; i < count; i++) {
    const server = fork(fileURLToPath(import.meta.url), [file, port, String(count)], { env: { ...process.env, ITEM_API_SERVER: String(i) } });
    server.on("message", (size) => {
      if (++ready === count) console.log(`item API: ${size} items on ${port}, ${count} processes`);
    });
    server.on("exit", (code, signal) => {
      console.error(`item API: server process ${i} stopped (${signal ?? code})`);
      process.exit(1);
    });
  }
  process.on("SIGTERM", () => process.exit(0));
} else {
  const items = new Map(Object.entries(JSON.parse(readFileSync(file, "utf8"))).map(([id, item]) => [id, Buffer.from(JSON.stringify(item))]));
  const prefix = "/api/discovery/items/";
  createServer((req, res) => {
    const body = req.url?.startsWith(prefix) ? items.get(decodeURIComponent(req.url.slice(prefix.length))) : undefined;
    if (!body) {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, { "content-type": "application/json", "content-length": body.length }).end(body);
  }).listen({ port: Number(port), host: "0.0.0.0", reusePort: count > 1 }, () => {
    if (process.send) process.send(items.size);
    else console.log(`item API: ${items.size} items on ${port}`);
  });
}
