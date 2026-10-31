# watchlist

A stock watchlist: the stocks you follow with their prices live, the market's news, price alerts
and your account. Read it first: it is the example with the most of Corten in it, one piece per
route, styled utility-first from design tokens.

## Run it

From this directory:

```sh
corten dev     # http://localhost:3000, reloads on save
cargo run      # the same, without the reload
```

`corten` is the framework's command (`cargo install corten-cli`).

Or in Docker, from the repository root:

```sh
docker build -f examples/watchlist/Dockerfile -t corten-watchlist .
docker run --rm -p 3000:8080 corten-watchlist
```

Its image build runs the workspace's tests first, so it takes longer than the others.

It needs no environment: prices are simulated in the process and lists are kept in memory.
`HOST` and `PORT` work as they do for every Corten app.

## Where things are

Each route, the file it is in and the piece of Corten it shows are in the table at the top of
[`src/app.rs`](src/app.rs). Around them:

| Piece | File |
|---|---|
| The routes, the app and its error page | [`src/app.rs`](src/app.rs) |
| The document around every page, the viewer in the header | [`src/layout.rs`](src/layout.rs) |
| Design tokens (W3C format) and the utility classes made from them | [`tokens/`](tokens/base.tokens.json), [`src/theme.rs`](src/theme.rs), [`static/style.css`](static/style.css) |
| Live prices: the simulated market, the feed, and the component whose script follows it | [`src/market.rs`](src/market.rs), [`src/home.rs`](src/home.rs), [`src/live_prices.js`](src/live_prices.js) |
| Who the browser is (a cookie), and the lists kept for it | [`src/visitor.rs`](src/visitor.rs), [`src/lists.rs`](src/lists.rs) |
| Text, in one locale (`messages!`) | [`locales/`](locales) |
| The types the scripts are checked against | [`types/app.d.ts`](types/app.d.ts) |

The browser test of the runtime runs against this app: [`scripts/smoke.mjs`](../../scripts/smoke.mjs).
