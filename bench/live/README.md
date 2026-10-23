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

All numbers are measured, from one run (`results/r3/`, 2026-10-23 UTC) on a DigitalOcean c-8
droplet (Intel Xeon Platinum 8280 at 2.7 GHz, 8 dedicated vCPUs, 16 GB, Ubuntu 24.04, kernel 6.8,
Docker 29.9), created for this suite and destroyed after it: the app pinned to core 1, the clients
on cores 2-7 (six worker threads), the system on core 0. Ten prices a second for 30 seconds;
delay is from the publisher's send to the client's receipt. Kept up: every price delivered to
every client, in order, at the rate asked. Fell behind: every price delivered, in order, but the
app took longer than 30 seconds to send them (the publisher stops after 120 seconds). Failed:
clients refused, prices lost or out of order, publishes unanswered, or the app killed for memory.
r2 (`results/r2/`) measured Corten before handlers replaced islands and before the later
request-path work (`corten::Json`, the middleware layer, float formatting); r1 (`results/r1/`)
ran Spring and Laravel with tuned settings. Both are kept for the record; numbers are compared
within a round.

### 1,000 clients (10,000 messages a second)

| App | Transport | Result | Connected | Delivered per second | Delay p50 / p99 | CPU | Memory connected | Per client |
|---|---|---|---|---|---|---|---|---|
| **corten** | feed (SSE) | kept up | 1,000 | 10,033 | 9.3 / 20 ms | 17% | 25.7 MiB | 25.6 KiB |
| leptos | SSE | kept up | 1,000 | 10,033 | 9.3 / 19 ms | 17% | 23.9 MiB | 23.7 KiB |
| aspnet | SSE | kept up | 1,000 | 10,033 | 13 / 29 ms | 22% | 45.4 MiB | 18.7 KiB |
| phoenix | Channels | kept up | 1,000 | 10,033 | 14 / 33 ms | 27% | 180.3 MiB | 45.5 KiB |
| sveltekit | SSE | kept up | 1,000 | 10,033 | 15 / 37 ms | 25% | 81.5 MiB | 63.6 KiB |
| spring | SSE | kept up | 1,000 | 10,033 | 15 / 55 ms | 28% | 239.3 MiB | 123.1 KiB |
| tanstack-start | SSE | kept up | 1,000 | 10,033 | 16 / 38 ms | 26% | 80.3 MiB | 57.5 KiB |
| astro | SSE | kept up | 1,000 | 10,033 | 16 / 39 ms | 26% | 103.8 MiB | 71.3 KiB |
| react-router | SSE | kept up | 1,000 | 10,033 | 18 / 89 ms | 33% | 98.8 MiB | 70.8 KiB |
| nextjs | SSE | kept up | 1,000 | 10,033 | 20 / 58 ms | 31% | 153.7 MiB | 112 KiB |
| laravel | Reverb | kept up | 1,000 | 10,033 | 22 / 36 ms | 34% | 152.6 MiB | 24.3 KiB |
| nuxt | SSE | kept up | 1,000 | 10,033 | 22 / 52 ms | 33% | 85 MiB | 57.3 KiB |
| django | SSE | kept up | 1,000 | 10,033 | 22 / 48 ms | 41% | 137.8 MiB | 96.3 KiB |
| rails | Action Cable | **failed: messages lost or out of order** | 1,000 | 10,000 | 38 / 89 ms | 67% | 97.3 MiB | 35.4 KiB |

### 10,000 clients (100,000 messages a second)

| App | Transport | Result | Connected | Delivered per second | Delay p50 / p99 | CPU | Memory connected | Per client |
|---|---|---|---|---|---|---|---|---|
| leptos | SSE | kept up | 10,000 | 100,334 | 96 / 235 ms | 102% | 232.2 MiB | 23.7 KiB |
| **corten** | feed (SSE) | kept up | 10,000 | 100,334 | 111 / 243 ms | 103% | 249.5 MiB | 25.5 KiB |
| aspnet | SSE | fell behind: 300 of 300 in 53.8 s | 10,000 | 55,762 | 220 / 475 ms | 102% | 200.9 MiB | 17.8 KiB |
| phoenix | Channels | fell behind: 300 of 300 in 83.6 s | 10,000 | 35,885 | 142 / 335 ms | 102% | 584.9 MiB | 46 KiB |
| sveltekit | SSE | fell behind: 300 of 300 in 98.5 s | 10,000 | 30,457 | 436 / 768 ms | 101% | 262.8 MiB | 24.9 KiB |
| tanstack-start | SSE | fell behind: 300 of 300 in 101.8 s | 10,000 | 29,470 | 458 / 786 ms | 101% | 241.2 MiB | 22.2 KiB |
| astro | SSE | fell behind: 300 of 300 in 109.7 s | 10,000 | 27,347 | 468 / 889 ms | 101% | 299.4 MiB | 27.2 KiB |
| nuxt | SSE | fell behind: 274 of 300 in 120.1 s | 10,000 | 22,814 | 510 / 1,008 ms | 101% | 307.3 MiB | 28.5 KiB |
| rails | Action Cable | fell behind: 160 of 300 in 120.1 s | 10,000 | 13,322 | 697 / 1,288 ms | 101% | 327.3 MiB | 27.1 KiB |
| django | SSE | **failed: 62 of 10000 clients not connected, killed out of memory** | 9,938 | 19,480 | 274 / 1,201 ms | 101% | 1011 MiB | 99.7 KiB |
| nextjs | SSE | **failed: 77 of 10000 clients not connected** | 9,923 | 16,827 | 501 / 2,569 ms | 101% | 604.5 MiB | 57.8 KiB |
| react-router | SSE | **failed: 214 of 300 publishes unanswered, messages lost or out of order** | 10,000 | 12,592 | 534 / 3,482 ms | 101% | 323.7 MiB | 30.1 KiB |
| spring | SSE | **failed: 8017 of 10000 clients not connected, 12 of 12 publishes unanswered, messages lost or out of order** | 1,983 | - | - | 100% | 373.9 MiB | 127.4 KiB |
| laravel | Reverb | **failed: 4995 of 10000 clients not connected, 300 of 300 publishes unanswered, messages lost or out of order** | 5,005 | - | - | 6% | 94 MiB | - |

At 1,000 clients every app but Rails delivered every message in order; they differ in cost.
Corten used 17% of its core and 25.7 MiB, Leptos 17% and 23.9 MiB, ASP.NET Core 22% and 45.4 MiB;
the JavaScript frameworks 25-33% and 80-154 MiB, Django 41%, Rails 67%. Rails delivered all
300,000 messages but 2 arrived out of order, which this suite counts as failed (r2's Rails kept
up).

At 10,000 clients only Corten and Leptos delivered all 100,000 messages a second, so their row is
close to what one core of this machine sends; the other apps used their whole core and fell
behind or failed. ASP.NET Core came next at 55,762 a second, then Phoenix at 35,885. Failures:

1. **django** was killed for memory at 1,011 MiB (about 100 KiB a client) with 9,938 connected;
   every price it sent before then reached every connected client. In r2 it stayed at 1,017 MiB
   and was not killed.
2. **nextjs** connected 9,923 of 10,000; every price it sent reached every client that was
   connected. Not investigated.
3. **react-router**: 214 of 300 publish requests got no answer within 10 seconds; the 86
   answered reached every client.
4. **spring** (Tomcat and JVM defaults) accepted 1,983 connections and answered none of its 12
   publishes. At about 130 KiB a client that is the default heap (a quarter of 1 GiB). r1, with
   75% of memory as heap and Tomcat's limit raised, held 6,666.
5. **laravel** (PHP's default `memory_limit`, 128 MB) accepted 5,005 connections, then Reverb
   closed all of them and every publish failed. r1, with 512 MB, held 10,000, so the limit is the
   likely cause; not confirmed.

Corten against Leptos: the same at 1,000 clients (9.3 ms median delay each); at 10,000 Leptos's
median delay was lower (96 against 111 ms) and the p99s level (235 against 243 ms). r2 had the
medians the other way round (78 against 88 ms), so the difference is within the noise of one
run. Each Corten message is 167 bytes on the wire against 61 for a plain SSE event, because it
carries the feed's cursor (`id: epoch-seq`) and a keyed patch
(`{"op":"upsert","id":...,"value":...}`): a client that reconnects resumes from its cursor, and a
patch updates one row of a grid, neither of which the plain stream does. The reconnect and resume
path was not measured here.