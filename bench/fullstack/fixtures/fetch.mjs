// Saves the item API's responses for items 1..500 (and the ids the browser and parity runs use)
// to fixtures/items.json, for the no-database runs. Needs the rust-item-detail API on :18400.
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const api = process.argv[2] ?? "http://localhost:18400";
const ids = [...Array.from({ length: 500 }, (_, i) => i + 1), 777, 1500, 1999, 2000, 2500, 3000, 12345, 49999];
const out = {};
for (const n of ids) {
  const res = await fetch(`${api}/api/discovery/items/item_${n}`);
  if (!res.ok) throw new Error(`item_${n}: ${res.status}`);
  out[`item_${n}`] = await res.json();
}
const file = join(dirname(fileURLToPath(import.meta.url)), "items.json");
writeFileSync(file, JSON.stringify(out));
console.log(`${Object.keys(out).length} items -> ${file}`);
