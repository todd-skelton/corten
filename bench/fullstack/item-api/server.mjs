// A stand-in for the item API on a bench host: the fixture's API responses (about 55 KB of JSON
// each) over HTTP at the real API's path, serialized once at startup. The apps still fetch and
// parse every item; there is no database behind it, so it measures the same work as the
// production API with a warm cache.
// Usage: node server.mjs <items.json> [port=8080]
import { readFileSync } from "node:fs";
import { createServer } from "node:http";

const [file = "/fixtures/items.json", port = "8080"] = process.argv.slice(2);
const items = new Map(Object.entries(JSON.parse(readFileSync(file, "utf8"))).map(([id, item]) => [id, Buffer.from(JSON.stringify(item))]));
const prefix = "/api/discovery/items/";
createServer((req, res) => {
  const body = req.url?.startsWith(prefix) ? items.get(decodeURIComponent(req.url.slice(prefix.length))) : undefined;
  if (!body) {
    res.writeHead(404).end();
    return;
  }
  res.writeHead(200, { "content-type": "application/json", "content-length": body.length }).end(body);
}).listen(Number(port), "0.0.0.0", () => console.log(`item API: ${items.size} items on ${port}`));
