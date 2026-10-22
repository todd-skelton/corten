# Requests and forms

[Back to the guides](../../README.md#documentation)

## How a request works

| Request | Sent by | Handler returns | Client does |
|---|---|---|---|
| `GET`, no `corten` header | first load, reload, no JavaScript | `req.page(meta, view)`: the shell around the page | renders HTML |
| `GET` with `corten: nav` | runtime, same-origin link or back/forward | the same call: `<title>`, head tags and the page only | swaps `<main>` |
| `POST` with `corten: action` | runtime, `<form data-patch>` | `req.patch(view)` | replaces each element by `id` |
| `POST`, no header | a form without JavaScript | `req.page(..)` or `req.redirect(&route)` | renders the page |
| any, `Accept: application/json` | native screen, agent, test | `req.screen(meta, &data, view)`: `{"title", "data"}` | renders natively, or reasons over it |
| SSE | `<corten-live src>` | `corten::live(stream_of_views)` | applies each like a patch |
| `GET {feed}`, `{feed}/live` | an island's `subscribe()`, or `Replica` | `feed.snapshot()`, `feed.live(since)` | snapshot, then ordered patches |

One handler serves every mode, and `req.mode` says which. The shell renders on full loads only;
every page carries a key hashed from the context and the deploy, and when a navigation answers
with a different key (sign-in, a new release) the runtime loads the whole document instead, so
the shell is never stale.

## Forms

`#[derive(Fields)]` gives a form struct its field names as `FieldName` constants, so markup and
validation cannot misspell them: `Errors` takes nothing else. Validation stays in the app; `Errors` carries the first message per field.

```rust,ignore
if let Err(errors) = check(&input) {
    // 422: the form as a patch for the runtime, `{"errors": ...}` for JSON clients, or the whole
    // page for a browser without JavaScript (only then is `page` called).
    return req.invalid(&errors, AlertForm { input: &input, errors: &errors }, page).await;
}
```

In the form, `<input name={f.email} {..errors.aria(f.email)}>` sets `aria-invalid` and
`aria-describedby`, and the runtime focuses the first invalid field.

## Redirects

After a successful write, `req.redirect(&route)` sends the browser on (post/redirect/get), with
and without JavaScript. It takes a typed route, or an `axum::http::Uri` for a URL outside the
app; a path written as text does not compile.

## Errors

A handler returns `corten::Result`; any `std::error::Error` converts with `?` and becomes a
500, and `Error::status(StatusCode::NOT_FOUND)` sets another status. The app's error page is
rendered in the request's mode (a page, a swap, or JSON for data clients) and the message is
logged, never shown, except under `corten dev`. An extractor's own rejection (a path segment
that does not parse, a form of the wrong type) gets the error page too when a browser asked.
