# Cancellation

[Back to the guides](../../README.md#documentation)

Both sides use one idea, the web's `AbortSignal`: work belongs to something, and stops when
that thing ends.

| Where | The signal | Aborts when |
|---|---|---|
| a handler | the `signal` keyword: `on:connect={js::watch(signal)}` | the element with the `on:` leaves the page; pass it to `addEventListener`, `fetch`, `subscribe` and `mount`, which is the only cleanup |
| a `<form data-patch>` submit | the runtime's, per form | a newer submit of the same form, or a navigation, so a late reply never patches the wrong page |
| a client navigation | the Navigation API's `event.signal` | another navigation starts |
| a request on the server | `req.signal()`, or `signal: corten::Signal` as an extractor | the response was sent (`Abort::Responded`), the client left (`Disconnected`), or the deadline passed (`Deadline`) |
| a task of the app's ([`App::task`](app.md#tasks)) | its argument | shutdown began (`Abort::Shutdown`); `serve` then waits for it up to `App::shutdown_grace` |

On the server, work the handler awaits needs no signal to stop: Rust drops the handler's future
when the client leaves or the deadline passes. The signal is for what a drop cannot reach: spawned
tasks, blocking threads, queries running elsewhere. A drop also cannot run async cleanup, so a
handler that holds something across an await (a lease it claimed, a lock in another service, a
reservation) puts the claim, the work and the release in `signal.finish(..)` ([Work that must
finish](#work-that-must-finish)). `App::deadline` (30 seconds unless set, `None` for
none) bounds the time to a response's start and answers 503 with the error page; streams run as
long as they need. It includes reading the request body, so an app that takes large uploads
raises it. A client closing an HTTP/1.1 connection mid-request is seen at once (an
integration test closes the socket and waits for `Disconnected`). The deadline's timer starts
only when a handler waits, so a page rendered from memory registers none.

Routes that start their answer later than pages should (an upstream that answers in minutes, a
report, large uploads) get a deadline of their own where they are mounted, so the rest of the
app keeps its short one:

| Requests to | Deadline |
|---|---|
| an API's routes | `Api::deadline` if it set one, else `App::deadline` |
| an area's routes | the innermost area that set `Area::deadline`, else `App::deadline` |
| everything else | `App::deadline` |

```rust,ignore
App::new(url::router())
    .area::<Reports>(|reports| reports::area(reports).deadline(Duration::from_secs(120)))
    .api::<Proxy>(|proxy| proxy.route::<Compact>(post(compact)).deadline(Duration::from_secs(600)))
```

Each answers the same way past it: the 503 (the error page, or JSON under an API) and
`Abort::Deadline`. `None` removes it for those routes only. A single slow route goes in an area
or an API of its own.

## Work that must finish

A handler that claims something must give it back, and a dropped future stops between the claim
and the release. `Signal::finish` runs its work on a task that finishes even if the handler is
dropped, and the handler awaits it as it would the work itself. The work gets a signal of its own,
which aborts when the request's does (the client left, the deadline passed) or when shutdown
begins, so the slow step can stop early while the release after it still runs:

```rust,ignore
async fn check(signal: Signal, State(gate): State<Gate>, Params(at): Params<url::Check>) -> HtmlResult {
    let outcome = signal
        .finish(move |signal| async move {
            let lease = gate.claim(at.provider).await?;
            let outcome = tokio::select! {
                outcome = provider_request(&lease) => outcome,
                reason = signal.aborted() => Outcome::Cancelled(reason),
            };
            // Runs whether or not the client is still there, with the provider's Retry-After.
            gate.release(at.provider, &lease, &outcome).await?;
            Ok::<_, Error>(outcome)
        })
        .await?;
    ..
}
```

`serve` waits for this work at shutdown, after the app's tasks and within the same
`App::shutdown_grace`, so a deploy does not cut a release off either; past the grace it is left
to end with the process, and logged. The work takes what it uses by value (it is `Send +
'static`). Keep only what must finish in it: the rest of the handler is still dropped when the
client leaves, which is what stops it.
