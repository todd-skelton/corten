// The live benchmark (bench/live): every open `GET /live/prices` stream, and `publish`, which
// sends a `POST /live/publish` body to each of them as one server-sent event. Resource routes return the streams.
const encoder = new TextEncoder();
type Client = ReadableStreamDefaultController<Uint8Array>;
// On globalThis, so every route bundle that imports this module shares one set.
const clients: Set<Client> = ((globalThis as { __livePrices?: Set<Client> }).__livePrices ??= new Set());

export function pricesStream(): Response {
  let self: Client;
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      self = controller;
      clients.add(controller);
      // A comment first, so the headers go out now rather than with the first message.
      controller.enqueue(encoder.encode(": open\n\n"));
    },
    cancel() {
      clients.delete(self);
    },
  });
  return new Response(body, { headers: { "content-type": "text/event-stream", "cache-control": "no-cache" } });
}

export function publish(body: string): Response {
  const event = encoder.encode(`data: ${body}\n\n`);
  for (const client of clients) {
    try {
      client.enqueue(event);
    } catch {
      clients.delete(client);
    }
  }
  return new Response(null, { status: 204 });
}
