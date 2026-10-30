# auction-room

A live card auction. People sign in and bid on the same lots at once; everyone watching a lot sees
its price, its latest bids and the time left change as they happen, and a bidder who is outbid is
told on every page they have open, and no one else is. Bids are kept in SQLite and checked in one
transaction, so a bid below the price is refused even when two arrive together, and a bid sent
twice (a double click, a retry) counts once. Each bidder may bid 20 times a minute. Without
JavaScript, bidding is a form and the page reads itself again while a lot is open.

Read it when several signed-in people act on the same thing in real time, or when your app keeps
its data in a database.

## Run it

From this directory:

```sh
HOUSE_KEY=change-me corten dev     # http://localhost:3000
```

`corten` is the framework's command (`cargo install corten-cli`).

Or in Docker, from the repository root:

```sh
docker build -f examples/auction-room/Dockerfile -t corten-auction-room .
docker run --rm -p 3000:8080 -e HOUSE_KEY=change-me -v auction-data:/app/data corten-auction-room
```

Open it in two browsers (or a private window), sign in under two names, and bid against yourself.
Sign in with the house key (`HOUSE_KEY`) to end a lot early from the house's page. Without a house
key no one can.

| Variable | What | Default |
|---|---|---|
| `DATABASE_URL` | the SQLite database | `sqlite:auction.db?mode=rwc` (a file here) |
| `HOUSE_KEY` | what signs the house in | none: the house cannot sign in |

When it starts with no lot open, it adds six, ending 4 to 30 minutes later.

## Where things are

The routes and what each does are in the table at the top of [`src/app.rs`](src/app.rs).

| Piece | File |
|---|---|
| Sessions, lots and bids in SQLite; a bid's checks and write in one `BEGIN IMMEDIATE` transaction | [`src/store.rs`](src/store.rs) |
| What is live: each lot's standing, the lobby, each bidder's own notices (`watch` channels) | [`src/room.rs`](src/room.rs) |
| The routes; who is signed in (`Identify`, with `Refused::sign_in`, `forbidden` and `unavailable`); the bid limit (an area's middleware, `Refused::too_many`); the handlers; the task that closes lots | [`src/app.rs`](src/app.rs) |
| The layout with the bidder's notices, and the error page | [`src/views.rs`](src/views.rs) |
| The lobby; a lot's page, its standing and the bid form; the house's desk | [`src/views/`](src/views) |

Its browser test, with two bidders in two browsers: [`smoke.mjs`](smoke.mjs).

## What a real auction adds

1. Accounts with a password or a provider: here anyone may sign in as any name.
2. More than one server process: the room is in memory, so each lot's changes would go through
   the database (Postgres `NOTIFY`) or a message bus to every process.
3. An end that moves when a bid comes in the last minute, so no one wins by bidding at the last
   second.