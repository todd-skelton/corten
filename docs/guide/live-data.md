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
// the route: each item replaces the element with the same id
async fn trades(State(market): State<Market>) -> LiveResponse {
    let next = market.trades.subscribe();
    corten::live(futures_util::stream::unfold(next, |mut next| async move {
        next.changed().await.ok()?;
        let html = LastTrade { text: &next.borrow_and_update() }.to_html();
        Some((html, next))
    }))
}

// the page: rendered first by the server, then kept current
html! { <corten-live src={url::Trades}><LastTrade text={&last_trade} /></corten-live> }
```

### Resuming

A part whose data changes rarely should not be drawn again on every page view and reconnect. Give
each fragment the app's cursor after it, `Fragment::new(html).cursor(seq)` (a version, a
sequence number, a realtime stream's position; Corten only carries it), and the page the cursor
it was rendered at, `<corten-live src={..} since={seq}>`. The handler's `LiveSince` is that
cursor on the first connection, and on a reconnect the last fragment's (the browser sends it
back as `Last-Event-ID`); `None` when there is neither. Send what changed after it, or nothing
until the next change when it is current:

```rust,ignore
async fn trades(State(market): State<Market>, LiveSince(since): LiveSince) -> LiveResponse {
    let mut next = market.trades.subscribe();
    if since.as_deref() != Some(next.borrow().0.to_string().as_str()) {
        next.mark_changed(); // the browser is behind: send the current trade first
    }
    corten::live(futures_util::stream::unfold(next, |mut next| async move {
        next.changed().await.ok()?;
        let (n, text) = next.borrow_and_update().clone();
        Some((Fragment::new(LastTrade { text: &text }.to_html()).cursor(n.to_string()), next))
    }))
}
```

A cursor the app no longer knows (too old, another deploy's) is the handler's to handle; drawing
the part as it is now is always correct. A cursor with a line break or NUL would end its event,
so `cursor` refuses it (a panic in debug builds; in release the fragment goes without it).

[examples/hello](../../examples/hello) has this running.

### Connection

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
handler calls `subscribe(url, { snapshot, patch, status }, { signal })`.

```rust,ignore
let stocks = Feed::new(rows);
Router::new()
    .route(url::Stocks::PATH, get(|State(m): State<Market>| async move { m.stocks.snapshot() }))
    .route(url::StocksLive::PATH, get(|State(m): State<Market>, since: Since| async move { m.stocks.live(since) }));

// a write: one numbered patch with the final value of every row it touched
market.stocks.write(|tx| if let Some(s) = tx.get_mut("s1") { s.set_price(13_250) });
```

Feed guarantees: every `write` is one numbered patch carrying the final value of each row it
touched, encoded once and sent as the same bytes to every subscriber. A snapshot is serialized
once per change too: its body is kept (in memory, the size of the JSON) until the next write,
`apply` or `reset`, so pages that subscribe between two writes cost no serialization. A
subscriber gets the snapshot and then each later patch exactly once, in order; reconnects resume
from `Last-Event-ID`, and an unknown cursor or a gap ends in a fresh snapshot. `corten-cluster`
runs one feed across instances (Postgres outbox, a leased relay to NATS JetStream): in the
failover benchmark, 777,600 of 777,600 patches arrived with both instances killed and restarted
under load (measured, one run, `examples/cluster/results/`).

Native apps and agents follow the same feed with `corten-protocol`'s `Replica`; see
[Surfaces](../architecture.md).
