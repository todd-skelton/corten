# Live data

[Back to the guides](../../README.md#documentation)

Corten has two ways to keep a page current, both over server-sent events.

## Fragments: `<corten-live>`

For server-rendered parts that change (a status line, a notification count), stream views:
`corten::live(stream)` turns a stream of rendered HTML into server-sent events, and
`<corten-live src>` swaps each fragment into the page by `id`, like a form's patch. No script of
your own; the stream closes when the element leaves the page. When the stream ends the element
stays as it is instead of reconnecting, so a stream of one fragment is a slow part of a page sent
when it is ready. A server shutting down ends its streams first, and those browsers reconnect.

```rust,ignore
// the route: one fragment, when the slow read is done; it replaces the element with its id
async fn picks(State(shop): State<Shop>) -> LiveResponse {
    corten::live(futures_util::stream::once(async move {
        let picks = shop.picks_for_you().await;
        Picks { picks: &picks }.to_html()
    }))
}

// the page: drawn without them, then completed
html! { <corten-live src={url::Picks}><p id={PicksId}>"Finding picks for you..."</p></corten-live> }
```

A value that keeps changing is the next section's `corten::live_watch`.

### Resuming

A part whose data changes rarely should not be drawn again on every page view and reconnect. Give
each fragment the app's cursor after it, `Fragment::new(html).cursor(seq)` (a version, a
sequence number, a realtime stream's position; Corten only carries it), and the page the cursor
it was rendered at, `<corten-live src={..} since={seq}>`. The handler's `LiveSince` is that
cursor on the first connection, and on a reconnect the last fragment's (the browser sends it
back as `Last-Event-ID`); `None` when there is neither. Send what changed after it, or nothing
until the next change when it is current.

One value that changes, kept in a `tokio::sync::watch` channel (the last trade and the trade
count), is `corten::live_watch(values, since, cursor, view)`: it sends the current value only to a
page that is behind, then the latest after each change (a browser needs no value it missed in
between), and ends when the channel's sender is dropped:

```rust,ignore
async fn trades(State(market): State<Market>, since: LiveSince) -> LiveResponse {
    corten::live_watch(market.trades.subscribe(), since, |(n, _)| *n, |(_, text)| LastTrade { text }.to_html())
}
```

Another source (a database's change stream, a queue) does the same with `corten::live`: compare
`since` with the version the part is at, send the current part first only when they differ, and
give each fragment its version with `Fragment::cursor`.

A cursor the app no longer knows (too old, another deploy's) is the handler's to handle; drawing
the part as it is now is always correct. A cursor with a line break or NUL would end its event,
so `cursor` refuses it (a panic in debug builds; in release the fragment goes without it).

[examples/watchlist](../../examples/watchlist) has this running for everyone.
[examples/auction-room](../../examples/auction-room) also streams each bidder's own notices (you
were outbid, you won): the route takes the `Caller`, so each stream is that caller's own `watch`
channel.

### Connection

Each open stream holds one of the browser's connections to the host. Over HTTP/1.1 a browser opens
about six, so a page with a few live parts open in a few tabs uses them up and its other requests
wait; serve browsers HTTP/2 or HTTP/3 from the proxy or CDN in front of the app, which carries every
stream on one connection (Corten itself speaks HTTP/1.1; design.md, Limits 14).

A browser without JavaScript gets no stream: a page whose part is relayed while a job runs asks
it alone to reload, with `ReadAgain { after }.without_javascript()` in its head
([read again](requests-and-forms.md)).

`<corten-live>` shows its connection on itself, so a page can say a part is stale with CSS alone:
`status` is `connecting`, `live` once the stream is open, `reconnecting` while the browser
retries after a drop, `closed` when it gave up (the server answered with no stream: an error, a
204) and `ended` when the server finished the stream (`end`). After a live stream drops,
`stale-since` holds when (an ISO time), until it is live again.

```css
corten-live[status="reconnecting"]::before { content: "Reconnecting"; }
corten-live[status="closed"] .reload { display: inline; }
```

Each change of `status` also dispatches a bubbling `corten-status` event on the element, for a
script that writes text CSS cannot ("Stale since 10:42", in the user's time zone). A handler
takes it like any other and reads the attributes, which hold the state:

```rust,ignore
let note = refs();
html! {
    <corten-live src={url::Trades} on:corten-status={js::stale(target, note, t.js().stale_since())}>..</corten-live>
    <output ref={note}></output>
}
```

```js
/** @param {HTMLElement} live @param {HTMLOutputElement} note @param {Message<"time">} since */
export function stale(live, note, since) {
  const at = live.getAttribute("stale-since");
  note.textContent = at ? since({ time: new Date(at).toLocaleTimeString() }) : "";
}
```

An event before the page's scripts have loaded runs nothing, as for any event; read the
attributes in `on:connect` too when the first state matters.

### Redrawing is cheap

With feature `compress`, a live stream is compressed as one stream per connection and flushed
after every event, so each event arrives at once and the compressor remembers the ones before it.
A fragment drawn again with one value changed costs about what changed: on a 6 KB list of 33
rows, the first event is 425 bytes with zstd and each redraw 16 to 23. So drawing a whole region
again is usually cheaper on the wire than it looks, and needs no script. The compression context is kept for
events up to about 64 KB apart. Each open stream holds about 0.5 MB (zstd) or 1 MB (brotli) while it
is open; past 256 MB for all of them, new streams go out uncompressed. Browsers that only accept
gzip get streams as they are, since deflate repeats a fragment poorly.

## Rows: feeds

For scripts that keep state (a table of live prices, a grid), a `corten::feed::Feed<Row>`
(feature `feed`) holds keyed rows and serves a snapshot plus ordered patches; an `on:connect`
handler calls `subscribe(feed, { snapshot, patch, status }, { signal })`.

```rust,ignore
let stocks = Feed::new(rows);
// in the routes module: each feed route names its rows
//   #[route("/feed/stocks", get = stocks, rows = Stock)] pub struct Stocks;
//   #[route("/feed/stocks/live", get = stocks_live, rows = Stock)] pub struct StocksLive;
async fn stocks(State(m): State<Market>) -> FeedResponse<Stock> { m.stocks.snapshot() }
async fn stocks_live(State(m): State<Market>, since: Since) -> FeedResponse<Stock> { m.stocks.live(since) }

// a write: one numbered patch with the final value of every row it touched
market.stocks.write(|tx| if let Some(s) = tx.get_mut("s1") { s.set_price(13_250) });
```

The row type goes from the feed to the script, checked at each step. `Feed<Stock>` answers
`FeedResponse<Stock>`; the route says `rows = Stock`, and a handler that answers with other rows
(or a feed's route that names none) does not compile. The markup passes the route,
`js::watch(.., url::Stocks, signal)`, to a parameter the script declares as `FeedUrl<Stock>`
(from `corten`). `Stock` there is the Rust type of that name in the component's module (bring it
in with `use`; `FeedUrl<import("../types/app").Stock>` reads the same), so the parameter takes
only a route whose rows are that type. A `FeedUrl` Corten cannot read (`FeedUrl<Stock[]>`) is an
error, not an unchecked string. `subscribe()` takes the row type from it, so `snapshot` and
`patch` are typed without a `@param` of their own, and a `Stock` handler given a `Level` feed
fails `tsc`:

```js
/** @import { FeedUrl } from "corten" */
/** @typedef {import("../types/app").Stock} Stock */

/** @param {FeedUrl<Stock>} feed @param {AbortSignal} signal */
export function watch(feed, signal) {
  subscribe(feed, { snapshot: (rows) => rows.forEach(paint), patch: () => {} }, { signal });
}
```

A plain `string` is not a `FeedUrl`, so a script cannot follow a feed whose rows it guessed.

Feed guarantees: every `write` is one numbered patch carrying the final value of each row it
touched, encoded once and sent as the same bytes to every subscriber. A snapshot is serialized
once per change too: its body is kept (in memory, the size of the JSON) until the next write,
`apply` or `reset`, so pages that subscribe between two writes cost no serialization. A
subscriber gets the snapshot and then each later patch exactly once, in order; reconnects resume
from `Last-Event-ID`, and an unknown cursor or a gap ends in a fresh snapshot. `corten-cluster`
runs one feed across instances (Postgres outbox, a leased relay to NATS JetStream): in the
failover benchmark (200 clients, 50 writes a second, each instance killed and restarted under
load), every patch arrived in both committed runs: 777,600 of 777,600, and 800,000 of 800,000
with clients back on the other instance after 1.1 s at the median (measured,
`examples/listings-feed/results/failover-r1.json` and `failover-r2.json`).

Native apps and agents follow the same feed with `corten-protocol`'s `Replica`; see
[Surfaces](../architecture.md).
