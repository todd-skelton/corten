import { readFileSync } from "node:fs";
import { env } from "node:process";
import { toItemView, type ItemView } from "../view-model";

const base = env.API_BASE ?? "http://localhost:8080";

// ITEM_SOURCE=fixture: API responses from FIXTURE_PATH, converted once, so the benchmark
// measures the web server alone.
const fixture =
  env.ITEM_SOURCE === "fixture"
    ? new Map(
        Object.entries(JSON.parse(readFileSync(env.FIXTURE_PATH ?? "fixtures/items.json", "utf8"))).map(
          ([id, value]) => [id, toItemView(value)] as const,
        ),
      )
    : null;

export async function fetchItem(id: string): Promise<ItemView | null> {
  if (fixture) return fixture.get(id) ?? null;
  const res = await fetch(`${base}/api/discovery/items/${encodeURIComponent(id)}`);
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`item API returned ${res.status}`);
  return toItemView(await res.json());
}
