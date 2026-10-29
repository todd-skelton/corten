# listings-feed

A seller's listings page whose table stays current from one feed served by several instances
through [`corten-cluster`](../../crates/corten-cluster): writes commit to Postgres with an outbox
row, the instance holding the lease publishes them to NATS JetStream, and every instance applies
them to its feed in memory. The page renders the first 500 listings from the instance's feed, its
script follows the feed (one subscription for the rows and the counts), and it resumes on any
instance with the same cursor. Its button moves five market lows; the rows change when the feed
brings the change, on every instance's pages. Read it when one instance is not enough.

## Run it

It needs Postgres and NATS with JetStream. From the repository root:

```sh
docker network create feeds
docker run -d --name feeds-pg --network feeds -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=feeds postgres:16-alpine
docker run -d --name feeds-nats --network feeds nats:2-alpine -js
docker build -f examples/listings-feed/Dockerfile -t corten-listings-feed .
docker run --rm --network feeds -p 3000:8080 -e DATABASE_URL=postgres://postgres:postgres@feeds-pg/feeds -e NATS_URL=nats://feeds-nats:4222 corten-listings-feed
```

Then open http://localhost:3000/sellers/s1 and press "Move five market lows". A second instance on the same network, with the same
`DATABASE_URL` and `NATS_URL`, `-e INSTANCE=b` and another port, serves the same feed.

To run it with `cargo run` (or `corten dev`) from this directory instead, publish the two
services' ports (`-p 5432:5432` on Postgres, `-p 4222:4222` on NATS): the defaults below reach
them on `localhost`. `corten` is the framework's command (`cargo install corten-cli`).

Remove them afterwards: `docker rm -f feeds-pg feeds-nats` and `docker network rm feeds`.

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
| The page and its write form | [`src/page.rs`](src/page.rs), [`static/style.css`](static/style.css) |
| The table the feed keeps current, and its counts | [`src/listing_table.rs`](src/listing_table.rs), [`.js`](src/listing_table.js), [`.css`](src/listing_table.css), [`src/listing_row.rs`](src/listing_row.rs) (a view) |
| Money and the count, in the page's locale | [`src/text.rs`](src/text.rs), [`locales/en.json`](locales/en.json) |
| The failover benchmark (kills and restarts instances while clients follow) and its results | [`bench/`](bench), [`results/`](results) |
