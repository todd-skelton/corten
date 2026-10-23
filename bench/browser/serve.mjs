// Serves out/ on http://127.0.0.1:44200 (or PORT). No dependencies.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { join, extname, normalize, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "out");
const types = { ".html": "text/html; charset=utf-8", ".js": "text/javascript", ".css": "text/css", ".json": "application/json" };
export function serve(port = Number(process.env.PORT ?? 44200)) {
  const server = createServer(async (req, res) => {
    let path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname));
    if (path.endsWith("/") || path.endsWith("\\")) path += "index.html";
    const file = join(root, path);
    if (!file.startsWith(root)) return res.writeHead(403).end();
    try {
      const body = await readFile(file);
      res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream", "cache-control": "no-store" }).end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((ok) => server.listen(port, "127.0.0.1", () => ok(server)));
}
if (process.argv[1] === fileURLToPath(import.meta.url)) serve().then(() => console.log("http://127.0.0.1:44200/"));
