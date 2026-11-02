# Live data

[Back to the guides](../../README.md#documentation)

Corten has two ways to keep a page current, both over server-sent events, and a third for a
script of the app's own: named events of JSON.

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

A job another process runs (a worker writing to a shared database, a third party's job API) has
no channel to watch, only a read to repeat. When many pages can follow the same job, one task
of the app's ([`App::task`](app.md#tasks)) reads it on an interval into a `watch` channel, and
each page's stream is `live_watch` over it: one read per interval however many pages are open.
A job only one page follows reads in its own stream, with `corten::live` over a loop that reads,
sends the part only when its version moved, ends after the state that settles it, and after a
few failed reads in a row draws that the job could not be read before it ends:

```rust,ignore
async fn batch_live(Path(id): Path<BatchId>, State(jobs): State<Jobs>, since: LiveSince) -> LiveResponse {
    // The version the page shows, failed reads in a row, and the wait before the next read
    // (None once the batch has settled and its last state is sent).
    let start = (since.0, 0, Some(Duration::ZERO));
    corten::live(futures_util::stream::unfold(start, move |(shown, mut failed, wait)| {
        let jobs = jobs.clone();
        async move {
            let mut wait = wait?;
            loop {
                tokio::time::sleep(wait).await;
                wait = Duration::from_secs(1);
                match jobs.batch(id).await {
                    Ok(batch) => {
                        failed = 0;
                        let version = batch.version.to_string();
                        let next = batch.running().then_some(wait);
                        if shown.as_deref() != Some(version.as_str()) {
                            let part = Fragment::new(BatchPanel { batch: &batch }.to_html()).cursor(version.clone());
                            return Some((part, (Some(version), 0, next)));
                        }
                        next?;
                    }
                    Err(error) if failed < 10 => {
                        failed += 1;
                        tracing::warn!(%error, "reading batch {id}");
                    }
                    Err(_) => {
                        // Ending without a word would look settled: the element keeps what it drew.
                        let part = Fragment::new(BatchUnreadable { id }.to_html());
                        return Some((part, (shown, 0, None)));
                    }
                }
            }
        }
    }))
}
```

The version is one the job writes (a counter, an update time); without one, a hash of the
rendered part does (a cursor another deploy does not know only draws the part again). A
settled batch is sent once more if the page has not drawn it, and the stream ends; a stream that
ends sends `end`, and the element keeps the last state it drew. How often to read, what to read
less often (a costly part every few reads), when the job is done and what giving up draws are
the app's to choose, so this loop is written in the app; what many pages share is `live_watch`.

`App::task` starts its tasks with the app, so jobs that start while it runs share one task: it
reads each running job and sends to that job's `watch` channel (a map from the job to its
sender), and drops the sender once the job settles, which ends every `live_watch` on it.

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
`examples/deck-builder` follows its saved deck this way, so a save made in another tab reaches a
page that is open.

`snapshot` also gets the write's `seq` and the feed's generation, `epoch` (`Feed::generation`).
A script that uses the feed's `seq` as a version (the deck builder sends it with a save, so a save
built on an older deck is refused) compares the pair: `seq` only counts writes within one
generation, and after a restart (a `Feed::new` feed) the new `seq` may be lower, equal or higher
than one the page holds without being the same deck. Render the generation next to the version,
and treat a snapshot from another generation as news whatever its `seq`. `reset` keeps the
generation, so a feed that starts following another log is a new `Feed::follow` with that log's
generation, not a `reset`.

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

## Named events: `corten::events`

For a script that reads a stream itself with `EventSource` (a job's progress drawn in a bar, a
status a native client follows), `corten::events(stream)` (feature `json`) sends each
`Message` as `event: <name>` with `data: <JSON>`. The route is a JSON route: a failure before
the stream starts is answered as JSON. As with `corten::live`, the stream is kept alive with a
comment every 15 seconds and ends when the app starts shutting down, so a deploy is not held open
by its clients.

```rust,ignore
// GET /jobs/{job}/progress: the job's state each time it moves, then a last word
async fn progress(State(jobs): State<Jobs>, job: JobId, since: LiveSince) -> EventsResponse {
    corten::events(jobs.changes(job, since).map(|state| match state {
        Some(state) => Message::new("progress", &state).cursor(state.version.to_string()),
        None => Message::new("gone", &()),
    }))
}
```

```js
/** @param {string} url @param {AbortSignal} signal */
export function follow(url, signal) {
  const progress = new EventSource(url);
  signal.addEventListener("abort", () => progress.close());
  progress.addEventListener("progress", (event) => draw(JSON.parse(event.data)));
  progress.addEventListener("gone", () => progress.close());
}
```

A message's cursor is the event's `id`, which a reconnecting `EventSource` sends back as
`Last-Event-ID`; `LiveSince` reads it, so the handler resumes after it. When the stream ends,
an `EventSource` connects again, so a stream that has a last word sends a message the script
closes on. A name is one line without a colon (another panics); a cursor with a line break or NUL
panics in debug builds and is dropped in release.

Prefer `<corten-live>` when the server can draw the part, and a feed when the script keeps rows:
both already resume, reconnect and stop with the element. Named events are for the data a script
of the app's own draws.
