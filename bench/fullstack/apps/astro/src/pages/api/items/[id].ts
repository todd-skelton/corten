import type { APIRoute } from "astro";
import { fetchItem } from "../../../lib/api";

// The item as JSON for other services: the same data the page renders.
export const GET: APIRoute = async ({ params }) => {
  const item = await fetchItem(params.id ?? "");
  return item ? Response.json(item) : new Response("Not found", { status: 404 });
};
