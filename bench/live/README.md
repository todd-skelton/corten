# Live updates: one price feed pushed to many clients, in fourteen frameworks

Each app holds many clients subscribed to listing-price updates, using the framework's own way
of pushing to browsers, and sends every price it is given to every client. This measures what a
live page (a price ticker, a seller grid, a notification badge) costs to hold open and to feed.

| App | Transport | How the app does it | Code |
|---|---|---|---|
| corten | Server-sent events, Corten's feed protocol | `corten::feed`: `GET /live/feed` is a snapshot, `/live/feed/live?since=` the numbered patches after it, encoded once for every client | [apps/corten](apps/corten) |
| leptos | Server-sent events | axum `Sse` and a tokio broadcast channel | [main.rs](../fullstack/apps/leptos/src/main.rs) |
| nextjs | Server-sent events | Route handler returning a `ReadableStream` per client | [lib/live.ts](../fullstack/apps/nextjs/lib/live.ts) |
| tanstack-start | Server-sent events | Server route, `ReadableStream` | [src/live.ts](../fullstack/apps/tanstack-start/src/live.ts) |
| sveltekit | Server-sent events | `+server.ts` endpoint, `ReadableStream` | [live.ts](../fullstack/apps/sveltekit/src/lib/server/live.ts) |
| react-router | Server-sent events | Resource route, `ReadableStream` | [live.server.ts](../fullstack/apps/react-router/app/live.server.ts) |
| astro | Server-sent events | Endpoint, `ReadableStream` | [live.ts](../fullstack/apps/astro/src/lib/live.ts) |
| nuxt | Server-sent events | h3 `createEventStream` | [prices.get.ts](../fullstack/apps/nuxt/server/routes/live/prices.get.ts) |
| aspnet | Server-sent events | `TypedResults.ServerSentEvents` over a `Channel<T>` per client | [LivePrices.cs](../fullstack/apps/aspnet/LivePrices.cs) |
| spring | Server-sent events | Spring MVC `SseEmitter` on Tomcat | [LiveController.java](../fullstack/apps/spring/src/main/java/bench/itempage/LiveController.java) |
| django | Server-sent events | Async view, `StreamingHttpResponse`, on uvicorn (ASGI) | [live.py](../fullstack/apps/django/itempage/live.py) |
| rails | WebSocket, Action Cable | A channel, `ActionCable.server.broadcast` | [prices_channel.rb](../fullstack/apps/rails/app/channels/prices_channel.rb) |
| phoenix | WebSocket, Phoenix Channels | A channel, `Endpoint.broadcast` over PubSub | [prices_channel.ex](../fullstack/apps/phoenix/lib/item_page_web/channels/prices_channel.ex) |
| laravel | WebSocket, Pusher protocol | Reverb, fed by a `ShouldBroadcastNow` event | [PriceTick.php](../fullstack/apps/laravel/app/Events/PriceTick.php) |

Apart from Corten's, these are the full-stack comparison's apps ([bench/fullstack](../fullstack))
with two more routes: the stream and `POST /live/publish`, which sends its JSON body on.

## What a run does

For each app and each number of clients (1,000 and 10,000):

1. Start the app alone: one CPU, 1 GiB of memory, a million open files allowed. Read its memory.
2. Connect the clients from a container on other cores ([client.mjs](client.mjs): Node 24, a worker
   thread per load core, six on the droplet, 1,000 new connections a second), each the way the framework's browser client
   does: one `EventSource`-style request for SSE, or a WebSocket that joins the channel. Corten's
   clients load the snapshot first, as `subscribe()` in corten.js does.
3. Ten seconds later, read the memory again (memory per client is the difference).
4. Publish 10 prices a second for 30 seconds, one request at a time, while sampling the app's CPU.
   Each price carries its sequence number and the time it was sent.
5. Every client records each price's delay (sent to received, same machine clock) and whether
   any were missing, repeated or out of order.

Reported: clients connected, prices delivered of those expected (connected clients x prices),
order, delay percentiles, CPU while publishing, memory, and whether the app kept up with 10
publishes a second (a publish request returns when the app has handed the price to every client,
so a slow fan-out stretches the publishing time).

## Fairness notes

- Every app runs with its framework's usual production settings in one CPU and 1 GiB. None is
  tuned to get through the test (a bigger heap, a higher connection limit): an app that cannot
  hold the clients that way is marked failed.
- One process per app, as in a single-instance deployment. Several processes would need a
  shared broker (Redis, Solid Cable, a database) for every one of these except Corten's
  `Feed::follow`, and that is not measured.
- **django** runs on uvicorn for this suite: gunicorn's sync workers (the full-stack setup) hold
  one stream each. There is no Channels or Redis: async views and an in-process queue per client.
- **rails** uses Action Cable's async (in-process) adapter, the fan-out Solid Cable or Redis would
  feed in a multi-process deployment.
- **laravel** runs Reverb, a second process, in the same container on the same CPU as Octane;
  each publish goes from Laravel to Reverb over HTTP. ext-uv is installed, as Reverb's docs say
  to for more than about 1,000 connections.
- **spring** runs with Tomcat's and the JVM's defaults: at most 8,192 connections, and a heap of
  a quarter of the container's memory.
- **nuxt** and **aspnet** send one event when a stream opens: their streams send no headers until
  the first write, so a browser would not see the connection open until the first price.
- Slow clients: Corten tells a client that falls more than its buffer behind to reload the
  snapshot (counted as resyncs; prices missed meanwhile count as missing, though the snapshot has
  the latest price). Leptos's broadcast channel skips what a slow client missed. The JavaScript,
  .NET and Python apps buffer without a limit.
- The client's own work is in the delay at 10,000 clients: four threads parse 100,000 messages a
  second.

## Running it

```sh
# images: the full-stack apps' (see bench/fullstack) and Corten's live app
docker build -f bench/live/apps/corten/Dockerfile -t cb-live-corten .
node bench/live/run.mjs <round> [clients=1000,10000] [apps=all] [rate=10] [seconds=30]
node bench/live/summarize.mjs <round>
# or on a fresh DigitalOcean droplet, created and destroyed for the run
node bench/fullstack/droplet.mjs <round> --ssh-key <id> --suite live
```

## Results

All numbers are measured, from one run (`results/r2/`, 2026-10-22) on a DigitalOcean c-8
droplet (Intel Xeon Platinum 8280 at 2.7 GHz, 8 dedicated vCPUs, 16 GB, Ubuntu 24.04, kernel 6.8,
Docker 29.9), the same droplet as the full-stack round r2: the app pinned to core 1, the clients
on cores 2-7 (six worker threads), the system on core 0. Ten prices a second for 30 seconds;
delay is from the publisher's send to the client's receipt. Kept up: every price delivered to
every client, in order, at the rate asked. Fell behind: every price delivered, in order, but the
app took longer than 30 seconds to send them (the publisher stops after 120 seconds). Failed:
clients refused, prices lost, or publishes unanswered. r1 (`results/r1/`) ran Spring and Laravel
with tuned settings and is kept for the record.

### 1,000 clients (10,000 messages a second)

| App | Transport | Result | Connected | Delivered per second | Delay p50 / p99 | CPU | Memory connected | Per client |
|---|---|---|---|---|---|---|---|---|
| **corten** | feed (SSE) | kept up | 1,000 | 10,033 | 7.8 / 18 ms | 13% | 25.7 MiB | 25.6 KiB |
| leptos | SSE | kept up | 1,000 | 10,033 | 9.0 / 18 ms | 16% | 23.9 MiB | 23.7 KiB |
| aspnet | SSE | kept up | 1,000 | 10,033 | 12 / 27 ms | 19% | 45.3 MiB | 18.6 KiB |
| phoenix | Channels | kept up | 1,000 | 10,033 | 12 / 30 ms | 24% | 179.3 MiB | 45 KiB |
| spring | SSE | kept up | 1,000 | 10,033 | 13 / 90 ms | 24% | 244.8 MiB | 121.8 KiB |
| astro | SSE | kept up | 1,000 | 10,033 | 14 / 30 ms | 22% | 100.4 MiB | 67.6 KiB |
| sveltekit | SSE | kept up | 1,000 | 10,033 | 14 / 32 ms | 24% | 87.3 MiB | 69.4 KiB |
| tanstack-start | SSE | kept up | 1,000 | 10,033 | 14 / 39 ms | 23% | 82.3 MiB | 59.5 KiB |
| react-router | SSE | kept up | 1,000 | 10,033 | 17 / 81 ms | 30% | 95.2 MiB | 67.4 KiB |
| nextjs | SSE | kept up | 1,000 | 10,033 | 20 / 60 ms | 29% | 155.5 MiB | 114.1 KiB |
| nuxt | SSE | kept up | 1,000 | 10,033 | 20 / 41 ms | 29% | 86.5 MiB | 59.2 KiB |
| laravel | Reverb | kept up | 1,000 | 10,033 | 21 / 34 ms | 33% | 152 MiB | 24.3 KiB |
| django | SSE | kept up | 1,000 | 10,033 | 22 / 47 ms | 42% | 137.7 MiB | 96.4 KiB |
| rails | Action Cable | kept up | 1,000 | 10,000 | 42 / 91 ms | 78% | 97.9 MiB | 36.4 KiB |

### 10,000 clients (100,000 messages a second)

| App | Transport | Result | Connected | Delivered per second | Delay p50 / p99 | CPU | Memory connected | Per client |
|---|---|---|---|---|---|---|---|---|
| **corten** | feed (SSE) | kept up | 10,000 | 100,334 | 78 / 216 ms | 102% | 250.5 MiB | 25.6 KiB |
| leptos | SSE | kept up | 10,000 | 100,334 | 88 / 188 ms | 102% | 232.2 MiB | 23.7 KiB |
| aspnet | SSE | fell behind: 300 of 300 in 56.4 s | 10,000 | 53,191 | 235 / 487 ms | 101% | 201.8 MiB | 17.9 KiB |
| phoenix | Channels | fell behind: 300 of 300 in 80.5 s | 10,000 | 37,267 | 135 / 345 ms | 101% | 583 MiB | 45.9 KiB |
| sveltekit | SSE | fell behind: 300 of 300 in 92.4 s | 10,000 | 32,468 | 402 / 752 ms | 101% | 268.6 MiB | 25.5 KiB |
| tanstack-start | SSE | fell behind: 300 of 300 in 96.5 s | 10,000 | 31,088 | 429 / 761 ms | 101% | 239.7 MiB | 22.1 KiB |
| astro | SSE | fell behind: 300 of 300 in 103.5 s | 10,000 | 28,986 | 439 / 851 ms | 102% | 301.4 MiB | 27.4 KiB |
| nuxt | SSE | fell behind: 292 of 300 in 120.2 s | 10,000 | 24,293 | 480 / 907 ms | 101% | 311.1 MiB | 28.9 KiB |
| rails | Action Cable | fell behind: 151 of 300 in 120.4 s | 10,000 | 12,542 | 744 / 1,461 ms | 101% | 324.9 MiB | 26.9 KiB |
| nextjs | SSE | **failed: 64 of 10000 clients not connected** | 9,936 | 24,385 | 446 / 1,647 ms | 101% | 605.1 MiB | 57.8 KiB |
| django | SSE | **failed: 4 of 10000 clients not connected** | 9,996 | 21,005 | 256 / 1,181 ms | 101% | 1017 MiB | 99.7 KiB |
| react-router | SSE | **failed: 214 of 300 publishes unanswered, messages lost or out of order** | 10,000 | 12,719 | 501 / 3,573 ms | 101% | 326.6 MiB | 30.4 KiB |
| spring | SSE | **failed: 8026 of 10000 clients not connected, 12 of 12 publishes unanswered, messages lost or out of order** | 1,974 | - | - | 100% | 382.4 MiB | 131.6 KiB |
| laravel | Reverb | **failed: 4997 of 10000 clients not connected, 300 of 300 publishes unanswered, messages lost or out of order** | 5,003 | - | - | 6% | 93.9 MiB | - |

At 1,000 clients every app kept up and delivered every message in order; they differ in cost.
Corten used 13% of its core and 25.7 MiB, Leptos 16% and 23.9 MiB, ASP.NET Core 19% and 45.3 MiB;
the JavaScript frameworks 22-30% and 82-156 MiB, Django 42%, Rails 78%.

At 10,000 clients only Corten and Leptos delivered all 100,000 messages a second, so their row is
close to what one core of this machine sends; the other apps used their whole core and fell
behind or failed. ASP.NET Core came next at 53,191 a second. Failures:

1. **spring** (Tomcat and JVM defaults) accepted 1,974 connections and answered none of its 12
   publishes. At about 130 KiB a client that is the default heap (a quarter of 1 GiB); the app's
   log was not kept. r1, with 75% of memory as heap and Tomcat's limit raised, held 6,666.
2. **laravel** (PHP's default `memory_limit`, 128 MB) accepted 5,003 connections, then Reverb
   closed all of them and every publish failed. r1, with 512 MB, held 10,000, so the limit is the
   likely cause; not confirmed.
3. **react-router**: 214 of 300 publish requests got no answer within 10 seconds; 99.1% of the 86
   answered reached every client.
4. **nextjs** connected 9,936 of 10,000 and **django** 9,996; neither failure was investigated.
   Every price they sent reached every client that was connected.

Corten against Leptos: the same at 1,000 clients; at 10,000 Corten's median delay was lower
(78 against 88 ms) and its p99 higher (216 against 188 ms), within the noise of one run. Each
Corten message is 167 bytes on the wire against 61 for a plain SSE event, because it carries the
feed's cursor (`id: epoch-seq`) and a keyed patch (`{"op":"upsert","id":...,"value":...}`): a
client that reconnects resumes from its cursor, and a patch updates one row of a grid, neither of
which the plain stream does. The reconnect and resume path was not measured here.
