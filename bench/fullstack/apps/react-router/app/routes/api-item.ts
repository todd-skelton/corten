import type { LoaderFunctionArgs } from "react-router";
import { fetchItem } from "../api.server";

// A resource route: the item as JSON for other services, the same data the page renders.
export async function loader({ params }: LoaderFunctionArgs) {
  const item = await fetchItem(params.id ?? "");
  return item ? Response.json(item) : Response.json({ error: "not found" }, { status: 404 });
}
