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

On the server, work the handler awaits needs no signal: Rust drops the handler's future when the
client leaves or the deadline passes. The signal is for what a drop cannot reach: spawned tasks,
blocking threads, queries running elsewhere. `App::deadline` (30 seconds unless set, `None` for
none) bounds the time to a response's start and answers 503 with the error page; streams run as
long as they need. It includes reading the request body, so an app that takes large uploads
raises it. A client closing an HTTP/1.1 connection mid-request is seen at once (an
integration test closes the socket and waits for `Disconnected`). The deadline's timer starts
only when a handler waits, so a page rendered from memory registers none.
