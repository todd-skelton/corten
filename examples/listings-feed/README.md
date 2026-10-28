# listings-feed

One seller's listings as a live feed served by several instances through
[`corten-cluster`](../../crates/corten-cluster): writes commit to Postgres with an outbox row, the
instance holding the lease publishes them to NATS JetStream, and every instance applies them to its
feed in memory. A page follows the feed, and a client resumes on any instance with the same
cursor. Read it when one instance is not enough.

## Run it

It needs Postgres and NATS with JetStream. From the repository root:

```sh
docker network create feeds
docker run -d --name feeds-pg --network feeds -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=feeds postgres:16-alpine
docker run -d --name feeds-nats --network feeds nats:2-alpine -js
docker build -f examples/listings-feed/Dockerfile -t corten-listings-feed .
docker run --rm --network feeds -p 3000:8080 -e DATABASE_URL=postgres://postgres:postgres@feeds-pg/feeds -e NATS_URL=nats://feeds-nats:4222 corten-listings-feed
```

Then open http://localhost:3000/sellers/s1. Start a second instance (`-e INSTANCE=b`, another
port) to see the feed served by both.

| Variable | What it does | Default |
|---|---|---|
| `DATABASE_URL` | Postgres | `postgres://postgres:postgres@localhost/feeds` |
| `NATS_URL` | NATS with JetStream | `nats://localhost:4222` |
| `INSTANCE` | this instance's name, sent as `X-Instance` | `a` |
| `SELLER`, `ROWS` | the seller seeded once per database, and its listings | `s1`, `5000` |
| `LEASE_MS` | how long the relay's lease lasts | `2000` |

## Where things are

The routes and what each shows are in the table at the top of [`src/app.rs`](src/app.rs).

| Piece | File |
|---|---|
| Startup: connect, migrate, seed, start the cluster, serve | [`src/main.rs`](src/main.rs) |
| The routes, the `X-Instance` middleware, readiness on Postgres | [`src/app.rs`](src/app.rs) |
| The listings and how a write changes them | [`src/listing.rs`](src/listing.rs) |
| The page that follows the feed | [`src/feed_monitor.rs`](src/feed_monitor.rs), [`src/feed_monitor.js`](src/feed_monitor.js) |
| The failover benchmark (kills and restarts instances while clients follow) and its results | [`bench/`](bench), [`results/`](results) |
