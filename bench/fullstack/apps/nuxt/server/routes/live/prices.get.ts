// The live benchmark (bench/live): an h3 event stream per client, kept in a set that
// POST /live/publish sends each body to.
import type { EventStream } from "h3";

const clients: Set<EventStream> = ((globalThis as { __livePrices?: Set<EventStream> }).__livePrices ??= new Set());

export default defineEventHandler((event) => {
  const stream = createEventStream(event);
  clients.add(stream);
  stream.onClosed(async () => {
    clients.delete(stream);
    await stream.close();
  });
  const sending = stream.send();
  // An event first, so the headers go out now rather than with the first price.
  void stream.push({ event: "open", data: "" });
  return sending;
});
