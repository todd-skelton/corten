import { createFileRoute } from "@tanstack/react-router";
import { publish } from "../live";

export const Route = createFileRoute("/live/publish")({
  server: { handlers: { POST: async ({ request }) => publish(await request.text()) } },
});
