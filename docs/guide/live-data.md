# Live data

[Back to the guides](../../README.md#documentation)

Corten has two ways to keep a page current, both over server-sent events.

## Fragments: `<corten-live>`

For server-rendered parts that change (a status line, a notification count), stream views:
`corten::live(stream)` turns a stream of rendered HTML into server-sent events, and
`<corten-live src>` swaps each fragment into the page by `id`, like a form's patch. No script of
your own; the stream closes when the element leaves the page.

```rust,ignore
// the route: each item replaces the element with the same id
async fn trades(State(market): State<Market>) -> impl IntoResponse {
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

[examples/hello](../../examples/hello) has this running.

## Rows: feeds

For islands that keep state (a table of live prices, a grid), a `corten::feed::Feed<Row>`
(feature `feed`) holds keyed rows and serves a snapshot plus ordered patches; an island calls
`subscribe(url, { snapshot, patch, status }, { signal })`.

```rust,ignore
let stocks = Feed::new(rows);
Router::new()
    .route(url::Stocks::PATH, get(|State(m): State<Market>| async move { m.stocks.snapshot() }))
    .route(url::StocksLive::PATH, get(|State(m): State<Market>, since: Since| async move { m.stocks.live(since) }));

// a write: one numbered patch with the final value of every row it touched
market.stocks.write(|tx| if let Some(s) = tx.get_mut("s1") { s.set_price(13_250) });
```

Feed guarantees: every `write` is one numbered patch carrying the final value of each row it
touched, encoded once and sent as the same bytes to every subscriber. A subscriber gets the
snapshot and then each later patch exactly once, in order; reconnects resume from
`Last-Event-ID`, and an unknown cursor or a gap ends in a fresh snapshot. `corten-cluster`
runs one feed across instances (Postgres outbox, a leased relay to NATS JetStream): in the
failover benchmark, 777,600 of 777,600 patches arrived with both instances killed and restarted
under load (measured, one run, `examples/cluster/results/`).

Native apps and agents follow the same feed with `corten-protocol`'s `Replica`; see
[Surfaces](../architecture.md).
