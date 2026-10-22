import { error, json } from "@sveltejs/kit";
import { fetchItem } from "#lib/server/api";
import type { RequestHandler } from "./$types";

// The item as JSON for other services: the same data the page renders.
export const GET: RequestHandler = async ({ params }) => {
  const item = await fetchItem(params.id);
  if (!item) error(404, "Not found");
  return json(item);
};
