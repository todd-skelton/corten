import { createFileRoute } from "@tanstack/react-router";
import { pricesStream } from "../live";

export const Route = createFileRoute("/live/prices")({
  server: { handlers: { GET: () => pricesStream() } },
});
