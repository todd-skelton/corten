# Requests and forms

[Back to the guides](../../README.md#documentation)

## How a request works

| Request | Sent by | Handler returns | Client does |
|---|---|---|---|
| `GET`, no `corten` header | first load, reload, no JavaScript | `req.page(meta, view)`: the shell around the page | renders HTML |
| `GET` with `corten: nav` | runtime, same-origin link or back/forward | the same call: `<title>`, head tags and the page only | swaps `<main>` |
| `POST` with `corten: action` | runtime, `<form data-patch>` | `req.patch(view)` | replaces each element by `id`; `view::Remove(id)` removes one (ids are `#[derive(Id)]` types) |
| `POST`, no header | a form without JavaScript | `req.page(..)` or `req.redirect(&route)` | renders the page |
| any, `Accept: application/json` | native screen, agent, test | `req.screen(meta, &data, view)`: `{"title", "data"}` | renders natively, or reasons over it |
| any, under an `App::api` prefix | a webhook, a service, a partner's site | JSON (`corten::Json`), errors as `{"error": ...}` | reads the JSON |
| SSE | `<corten-live src>` | `corten::live(stream_of_views)` | applies each like a patch |
| `GET {feed}`, `{feed}/live` | a handler's `subscribe()`, or `Replica` | `feed.snapshot()`, `feed.live(since)` | snapshot, then ordered patches |

One handler serves every mode, and `req.mode` says which. A route with no screen goes under
[`App::api`](app.md#json-only-routes) instead, where every answer is JSON. The shell renders on full loads only;
every page carries a key hashed from the context and the deploy, and when a navigation answers
with a different key (sign-in, a new release) the runtime loads the whole document instead, so
the shell is never stale.

## Forms

`#[derive(Fields)]` gives a form struct its field names as `FieldName` constants, so markup and
validation cannot misspell them: `Errors` takes nothing else. Validation stays in the app; `Errors` carries the first message per field.

```rust,ignore
if let Err(errors) = check(&input) {
    // 422: the form as a patch for the runtime, the JSON error with `fields` for JSON clients,
    // or the whole page for a browser without JavaScript (only then is `page` called).
    return req.invalid(&errors, AlertForm { input: &input, errors: &errors }, page).await;
}
```

`Form` and `Query` in the prelude are Corten's: they read a body or a query string the way a
browser sends it, a key once per value.

| Field type | Reads | A browser sends |
|---|---|---|
| `String`, a number, a unit-variant enum | the key's one value; sent twice is a 422 that names the field | `<input>`, `<select>`, a radio group |
| `bool`, with `#[serde(default)]` | `true` for `on` or `true`, `false` when missing | a checkbox: `on` when checked, nothing when not |
| `Option<T>` | `None` when missing or blank | an empty `<input type="number">` sends `max=` |
| `Vec<T>`, with `#[serde(default)]` | every value, in order; empty when missing | checkboxes sharing a name, `<select multiple>` |

`#[derive(Fields)]` refuses a `bool` or `Vec` field without `#[serde(default)]`, since the browser
sends nothing when no box is checked and the form would fail exactly then. A required list is a
validation rule (`errors.add(f.ids, ..)` when it is empty), so the user gets a message, not an
error page. A box outside the `<form>`, in a table row, joins it with `form={ArchiveFormId}`
(an `Id`); `examples/seller-desk` posts its checked rows that way.

A typed route's list field writes the parameter once per item, so a filter link and the handler
that reads it agree:

```rust,ignore
#[derive(Route, serde::Deserialize)]
#[route("/search")]
struct Search { q: Option<String>, #[serde(default)] condition: Vec<String> }

// <a href={Search { q: None, condition: vec!["nm".into(), "lp".into()] }}> is /search?condition=nm&condition=lp
async fn search(req: Req, Query(search): Query<Search>) -> corten::Result { .. }
```

Measured in a release build (Docker on a Windows workstation, best of five runs): the item page's two-field
alert form reads in 673 ns with `corten::Form` and 871 ns with axum's `Form`, request included.
200 ids read into a `Vec<String>` in 22.3 µs; one comma-joined field split in the handler took
6.5 µs, most of the difference being 200 strings instead of one.

In the form, `<input name={f.email} {..errors.aria(f.email)}>` sets `aria-invalid` and
`aria-describedby`, and the runtime focuses the first invalid field (unless the user has moved
on to another part of the page while waiting).

A patch replaces markup but not the user's work: a field typed into after the form was sent (or,
in a `<corten-live>` patch, any field the user changed) keeps its value, and focus and the caret
stay on the matching element. Everything else is the server's.

## Redirects

After a successful write, `req.redirect(&route)` sends the browser on (post/redirect/get), with
and without JavaScript. It takes a typed route, or an `axum::http::Uri` for a URL outside the
app; a path written as text does not compile.

## Errors

A handler returns `corten::Result`; any `std::error::Error` converts with `?` and becomes a
500, and `Error::status(StatusCode::NOT_FOUND)` (or `.with_status(..)` on an error) sets
another status. The app's error page is rendered in the request's mode (a page or a swap) and
the message is logged, never shown, except under `corten dev`. An extractor's own rejection (a
path segment that does not parse, a form of the wrong type) gets the error page too when a
browser asked.

A client that asked for JSON, and every request under an [`App::api`](app.md#json-only-routes)
prefix, gets every failure in one shape: `{"error":{"status":404,"message":"Not Found"}}`. A
4xx carries the error's message or the rejection's text; a 5xx carries only the status's
reason. Validation errors add the fields:
`{"error":{"status":422,"message":"email: ...","fields":{"email":"..."}}}`.
