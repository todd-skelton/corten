import type { EventStream } from "h3";

const clients: Set<EventStream> = ((globalThis as { __livePrices?: Set<EventStream> }).__livePrices ??= new Set());

export default defineEventHandler(async (event) => {
  const body = (await readRawBody(event, "utf8")) ?? "";
  for (const stream of clients) void stream.push(body);
  setResponseStatus(event, 204);
  return null;
});
