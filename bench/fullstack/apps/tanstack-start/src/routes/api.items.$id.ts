import { createFileRoute } from "@tanstack/react-router";
import { fetchItem } from "../item-api";

// A server route: the item as JSON for other services, the same data the page renders.
export const Route = createFileRoute("/api/items/$id")({
  server: {
    handlers: {
      GET: async ({ params }) => {
        const item = await fetchItem(params.id);
        return item ? Response.json(item) : Response.json({ error: "not found" }, { status: 404 });
      },
    },
  },
});
