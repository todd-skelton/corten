# Requests and forms

[Back to the guides](../../README.md#documentation)

## How a request works

| Request | Sent by | Handler returns | Client does |
|---|---|---|---|
| `GET`, no `corten` header | first load, reload, no JavaScript | `req.page(doc)`: the app's layout with the page in it | renders HTML |
| `GET` with `corten: nav` | runtime, same-origin link or back/forward | the same call: `<title>`, head tags and the page only | swaps `<main>` |
| `POST` with `corten: action` | runtime, `<form data-patch>` | `req.patch_or(view, page)`, or `req.patch(view)` | replaces each element by `id`; `view::Remove(id)` removes one, `view::Append(id, rows)` adds rows at its end (ids are `#[derive(Id)]` types) |
| `POST`, no header | a form without JavaScript | `req.page(..)` or `req.redirect(&route)` | renders the page |
| any, `Accept: application/json` | native screen, agent, test | `req.screen(&data, doc)` (`data` a `ToJson`): `{"title", "data"}`, the title from the document's `<title>` | renders natively, or reasons over it |
| any, under an `App::api` prefix | a webhook, a service, a partner's site | JSON (`corten::Json`), errors as `{"error": ...}` | reads the JSON |
| SSE | `<corten-live src>` | `corten::live(stream_of_views)` | applies each like a patch |
| `GET {feed}`, `{feed}/live` | a handler's `subscribe()`, or `Replica` | `feed.snapshot()`, `feed.live(since)` | snapshot, then ordered patches |

One handler serves every mode, and `req.mode` says which. A `data-patch` form's handler answers
both of its rows with `req.patch_or(view, || async { req.page(..) }).await`: the patch with
JavaScript, and otherwise the page (or a redirect), whose closure runs only then, so the patch
never loads what only the page shows. A patch that needs data of its own, which the page does not,
or a status or headers of its own, checks `req.mode == Mode::Action` itself. A route with no screen goes under
[`App::api`](app.md#json-only-routes) instead, where every answer is JSON. The layout around `<main>` renders on full
loads only ([the document](views.md#the-document)); every page carries a key hashed from the
caller its handler read (if any), the deploy and its layout, and when a navigation answers with a different key
(sign-in, a new release, a page in another layout) the runtime loads the whole document instead, so the layout is
never stale.

## Forms

A form is for what the server has to answer: a write, or data the page does not have. Sorting,
filtering or totalling what the page already shows is faster in a handler, with no request
([Server or browser](scripts.md#server-or-browser)).

`#[derive(Fields)]` gives a form struct its field names as `FieldName` constants, so markup and
validation cannot misspell them: `Errors` takes nothing else. Validation stays in the app; `Errors` carries the first message per field.
Any name works, including ones the DOM gives a form's own properties (`action`, `method`, `id`): the runtime
posts a `data-patch` form to its own `action` and `method` whatever its controls are named. An app's own
script that reads `form.action` gets the control instead, as in any page; read `getAttribute("action")`.
`FieldName`, `Errors` and `Form` are all in the prelude, and so is `Result`, whose error type
defaults to `corten::Error`: a handler returns `Result`, and a helper that checks one field
returns `Result<u32, Errors>`, both under `use corten::prelude::*`.

```rust,ignore
if let Err(errors) = check(&input) {
    // 422: the form as a patch for the runtime, the JSON error with `fields` for JSON clients,
    // or the whole page for a browser without JavaScript (only then is `page` called).
    return req.invalid(&errors, AlertForm { input: &input, errors: &errors }, page).await;
}
```

A JSON handler has no form to show again: it `?`s the same check, next to the `?` of a load
that failed. `JsonResult<T>`'s error is a `JsonError`, which takes either: field errors are
the 422 with `fields`, anything else is answered as any failure of a JSON route
(`examples/card-market`'s `POST /api/alerts`):

```rust,ignore
async fn alert_json(State(source): State<Source>, Json(input): Json<CreateAlert>) -> JsonResult<AlertSet> {
    let price = check(&input)?; // 422, {"error":{..,"fields":{"email":".."}}}
    let item = source.item(&input.id).await?.ok_or_else(Error::not_found)?; // 500 or 404
    Ok(Json(AlertSet::new(&item, price)))
}
```

On a page, `?` on `Errors` does not compile: `HtmlResult`'s error is an `Error`, and a
page's form answers with `req.invalid`, which shows the form again. Under an API with its own
error shape (`Api::errors`), the 422 keeps Corten's shape, since a status and a message have
no place for the fields; every other failure is written in the API's.

`Form` (a form's body, or a `GET` form's query string) and `Params` (a route's own parameters,
below) read what a browser sends the way it sends it, a key once per value.

| Field type | Reads | A browser sends |
|---|---|---|
| `String`, a number, a unit-variant enum | the key's one value; sent twice is a 422 that names the field | `<input>`, `<select>`, a radio group |
| `bool`, with `#[serde(default)]` | `true` for `on` or `true`, `false` when missing | a checkbox: `on` when checked, nothing when not |
| `Option<T>` | `None` when missing or blank | an empty `<input type="number">` sends `max=` |
| `Vec<T>`, with `#[serde(default)]` | every value, in order; empty when missing | checkboxes sharing a name, `<select multiple>` |

`#[derive(Fields)]` refuses a `bool` or `Vec` field without `#[serde(default)]`, since the browser
sends nothing when no box is checked and the form would fail exactly then. A required list is a
validation rule (`errors.add(f.ids, ..)` when it is empty), so the user gets a message, not an
error page; `corten_ui::CheckboxGroup` around the boxes has the browser say so before the form is
sent, and a field required only for one choice of another (a refund amount for a partial refund)
is `corten_ui::RequiredWhen`. The handler's check stays: it is what a browser without scripts
gets. A box outside the `<form>`, in a table row, joins it with `form={ArchiveFormId}`
(an `Id`); `examples/seller-desk` posts its checked rows that way.

A route's handler reads its parameters back as the route's `Params`: the struct's fields, owned
(a `&'a str` is a `String`, a `&'a [T]` a `Vec<T>`), read from the path and the query the way
the struct writes them. So a link and the handler that reads it cannot disagree about a
parameter's name or type, and a request whose parameters do not fit is a 400. A typed route's
list field writes the parameter once per item, and reads every value back:

```rust,ignore
#[corten::routes]
mod url {
    #[route("/search", get = search)]
    pub struct Search<'a> { pub q: Option<&'a str>, pub condition: &'a [&'a str] }
}

// <a href={url::Search { q: None, condition: &["nm", "lp"] }}> is /search?condition=nm&condition=lp
async fn search(req: Req, Params(search): Params<url::Search<'_>>) -> HtmlResult {
    let (q, conditions): (&Option<String>, &Vec<String>) = (&search.q, &search.condition);
    ..
}
```

A missing parameter is `None` for an `Option`, empty for a list, and a field's
`#[serde(default)]` for anything else, as a link leaves those out. In a `#[corten::routes]`
module a route with a parameter in its path holds each of its handlers to taking its own
`Params`: one that reads the path another way (axum's `Path`), or reads another route's, does
not compile.

Links write a route's parameters by name with `Display`, so `Params` refuses what would read them
back differently: a `#[serde(..)]` key on the route struct that links do not follow (`alias`,
`flatten`, `skip`, `skip_deserializing`, `with`, `deserialize_with`, `borrow` on a
field; `default`, `deny_unknown_fields`, `from`, `try_from`, `transparent` on the struct), on the
route or on a `#[route(query)]` set it includes, and a set that borrows. A struct's `default`
is refused because a link of the default writes nothing for what it fills (`None`, an empty
list), which the default would read back as its own value; give the field its own
`#[serde(default)]` or `#[serde(default = "path")]` instead. Such a route still writes
links; a handler that takes its `Params` does not compile, and the error names the key or the
borrowed type.

When several routes carry the same parameters (a list's filters, which its `<corten-live>` rows,
its bulk preview and its confirm need too), declare them once as a query set and include it:

```rust,ignore
#[derive(Clone, Default, serde::Deserialize, Route, Fields)]
#[route(query)]
struct ItemQuery { search: Option<String>, page: Option<u32>, #[serde(default)] status: Vec<String> }

#[derive(Route)] #[route("/items")]      struct Items     { #[route(query)] filters: ItemQuery }
#[derive(Route)] #[route("/items/live")] struct ItemsLive { #[route(query)] filters: ItemQuery }

// ItemsLive { filters: items.filters.clone() } is /items/live?search=..&page=..
async fn items(req: Req, Params(items): Params<Items>) -> HtmlResult { .. } // items.filters
```

Each of `ItemQuery`'s fields is written as the route's own parameter, so a route that must carry
the filters cannot leave one out: a filter added to `ItemQuery` is in every route at once. The
handler reads it as its route's `Params` field, each of the set's fields by its key as the route
reads its own (a list, a prefix field and a set inside the set included; the set needs no
`Deserialize` for it), and the filter form names its inputs `ItemQuery::FIELDS`. A
`#[route(query)]` field whose type is not a query set does not compile, and neither does a route
whose own parameters and its sets' could be read back as each other's: one name written twice, or
a prefix another name starts with.

A parameter is named as serde reads its field, so `#[serde(rename_all = "camelCase")]` or a
field's `#[serde(rename = "priceMin")]` renames it in links as well: addresses that are already
shared or indexed keep working, and the link and the handler cannot disagree. A path parameter
keeps the name the path gives it, so renaming one does not compile.

When the parameters' names are data (an option per product dimension, a filter per attribute),
mark a map field with a prefix, and each entry is written as `prefix` + key = value, in order:

```rust,ignore
#[derive(Route)]
#[route("/items/{slug}")]
struct Item<'a> { slug: &'a str, #[route(prefix = "dimension.")] dimensions: &'a BTreeMap<String, String> }

// Item { slug: "pika", dimensions: &picked } is /items/pika?dimension.form=graded&dimension.grade=PSA%208
async fn item(req: Req, Params(item): Params<Item<'_>>) -> HtmlResult { .. } // item.dimensions: the entries, prefix taken off
```

The field is a `BTreeMap`, or a `Vec` or slice of pairs (kept in its order, repeats and all);
a `HashMap` does not compile, because its order would make the same link differ between
renders. The key and the value are percent-encoded. The prefix is letters, digits and `-._~`;
no other parameter's name may start with it, and no two prefixes may start one another, because
the handler could not tell them apart: both are compile errors.

Measured in a release build (Docker on a Windows workstation, best of five runs): the item page's two-field
alert form reads in 673 ns with `corten::Form` and 871 ns with axum's `Form`, request included.
200 ids read into a `Vec<String>` in 22.3 µs; one comma-joined field split in the handler took
6.5 µs, most of the difference being 200 strings instead of one.

In the form, the errors are read within the form's id, the `#[derive(Id)]` of the element a
refusal replaces: `let e = errors.within(PriceId(id));`. Then `<input name={f.price}
{..e.aria(f.price)}>` sets `aria-invalid` and `aria-describedby`, `{e.message(f.price)}` prints
the message it points at, with the id `{form id}-{field}-error` (`price-s1-price-error`), so two
refused forms with a field of the same name (a price in every row) never share a message id.
A field with a hint of its own (a format, a size limit) names it to the spread,
`{..e.aria(f.price).hint(&PriceHintId(id))}`, which writes one `aria-describedby` naming the hint
and then the message. A browser keeps only an attribute's first value, so a start tag that writes
`aria-describedby` or `aria-invalid` beside `e.aria(..)` does not compile.
The runtime focuses the first invalid field (unless the user has moved on to another part of the
page while waiting).

A refusal no single field explains (a rule across fields, or a service the handler wrote to
saying no: "The policy was retired meanwhile.") is the form's own message:
`errors.add_form(text)`, shown with `{errors.form_message()}` at the top of the form, a
`role="alert"` paragraph that is announced when the patch arrives. JSON clients get it first in
`message` and as `form`; an agent tool gets it first in its error.

A write that must not apply twice (an order, a payment, a message sent) carries a `SubmitKey`.
A 502 or a dropped connection can hide a write that committed, and the user, the browser or the
runtime then sends it again. Add a `SubmitKey` field to the form struct and draw it with
`SubmitKey::field`, a hidden input with a fresh random UUID v4. A UUID is what most APIs take as
an idempotency key, so the app passes the key on as it is:

```rust,ignore
#[derive(Deserialize, Fields)]
struct OrderInput { quantity: u32, submit: SubmitKey }

html! { <form method="post" action={url::Order { id }} data-patch>{SubmitKey::field(f.submit)} .. </form> }

async fn order(req: Req, State(db): State<Db>, Form(input): Form<OrderInput>) -> HtmlResult {
    // In the same transaction as the write: a key seen before answers with the first outcome.
    let placed = db.place_once(input.submit.as_str(), input.quantity).await?;
    req.redirect(&url::Order { id: placed.id })
}
```

The same key is sent for as long as the submission has no answer: a page reloaded after a post
posts it again, and the runtime sends a `data-patch` form again with it after a network error, a
5xx, an abort (a newer submit of the same form, a navigation) or an answer whose body was cut off
on the way. Once a whole answer arrives, any status below 500, the runtime gives the form a new
key, so the next submit is a new one; a page or patch the server draws has a new key anyway. A form changed and sent again before the first had an
answer is the same submission: the app answers it with what it did the first time, and the user
sees that and can send the change. The key is the app's to store, next to the write (Corten keeps
no store); `corten_cluster::begin` takes it as its idempotency key. A field that is not a UUID v4
(in either case; it is kept lowercase) is a 422 that names it. Make the field
`Option<SubmitKey>` when data clients post the same form without one.

The key is a form field and not the `Idempotency-Key` header that some HTTP APIs read. A browser
without JavaScript cannot set a header, so a field is the one way that works for every client,
and a data client posts the same field. The header is still an IETF draft
(`draft-ietf-httpapi-idempotency-key-header`); a second place for the key would wait for it to
be a standard, with a rule for a request that sends both.

When a `data-patch` submit gets no answer the runtime can draw, it dispatches `corten:error` on
the form (it bubbles), with `detail.status`: the answer's status (a proxy's 502 page), or 0 when
there was none or its body was cut off (`detail.error` says why). The form keeps what the user
typed and, below 500, its key, so the page can say so and the user can send it again. A script
that needs what the server decided on a submit that did draw (saved at version 4, refused,
stale) gets it from the answer itself; see [Typed events](scripts.md#typed-events).

A patch replaces markup but not the user's work: a field typed into after the form was sent (or,
in a `<corten-live>` patch, any field the user changed) keeps its value, and focus and the caret
stay on the matching element. When the patch takes the focused element away (a row removed with
its Remove button, the last row of a list drawn again without it) or its new markup cannot take
focus (a button now disabled), focus goes to the nearest element around it that is still there,
made focusable for the move (`tabindex="-1"`, removed once focus moves to another element), so a
keyboard or screen reader user stays in the part of the page they were in rather than at the top
of the document. A list is announced as a list; give a `<section>` or `<div>` that may take focus
this way an accessible name (`aria-labelledby` its heading), and say what happened in a status
message. A row with an id is found by it, so focus lands on its list, not on another row; Tab
then goes on from the start of the list. A file input keeps the files the user chose, which markup cannot
give back, so a refusal can redraw the input with `aria-invalid` and its message and the files
are still there to send again; only a successful answer to the form that sent them takes them
(found by first file name, so files chosen again under the same name while it was on its way go
too). What the user opened stays open: a `<dialog>` (modal again if it
was modal), a `<details>` they unfolded and a shown popover, matched by id (a `<details>` also by
its place). One the user left as the server sent it takes the new markup's state, so a dialog
the server sent open and now sends closed closes. A dialog the server draws `open` is open but not
modal; give it `on:connect={corten_ui::dialog::modal(target)}` ([headless components](ui.md)). Everything else is the server's.

Each top-level element of a patch replaces the element with its `id`, and the browser drops one
without an id (and any text outside an element), so a refusal drawn inside a wrapper `<div>`
would never show. `req.patch`, `req.patch_with_status` and `req.invalid` therefore take a
`Patch`, which the compiler checks: `html!` whose top-level elements each have `id={..}` with a
`#[derive(Id)]` type (in every branch of an `if` or `match` among them, whether or not it runs),
`Remove`, or a component declared `-> impl Patch`, whose markup must be such an `html!`:

```rust,ignore
#[component]
pub fn Status(text: &str) -> impl Patch {
    html! { <p id={StatusId} role="status">{text}</p> }   // the root has the id
}
```

A component that says `-> impl Render` is not a patch, so one that is drawn by the page and sent
by an action says `-> impl Patch`, as a layout says `-> impl Document`. A `corten::live`
fragment is already text, so debug builds check it when it is sent instead (it panics, naming the
element).

## Files

A form with a file input posts `multipart/form-data` (`enctype="multipart/form-data"`), which
`Form` does not read (it answers 415). The handler takes `Upload<T>` instead. The input is named
by a `Files` field of the form struct, so the markup (`name={f.photos}`), the files and `Errors`
all take the same `FieldName`:

```rust,ignore
#[derive(serde::Deserialize, Fields)]
struct NewListing { title: String, price: u32, photos: Files }

async fn create(req: Req, mut upload: Upload<NewListing>) -> HtmlResult {
    let f = NewListing::FIELDS;
    upload.limit(60 << 20); // the whole body; 2 MiB unless set
    let (mut photos, mut errors) = (Vec::new(), Errors::new());
    while let Some(mut file) = upload.next_file().await? {
        if !file.is(f.photos) { continue }
        if !matches!(file.content_type(), "image/jpeg" | "image/png" | "image/webp") {
            errors.add(f.photos, "Photos can be JPEG, PNG or WebP.");
        } else if photos.len() == 24 {
            errors.add(f.photos, "A listing can have 24 photos.");
        } else {
            match file.bytes_up_to(10 << 20).await? {
                Some(bytes) => photos.push((file.filename().to_string(), bytes)),
                None => errors.add(f.photos, "Each photo can be up to 10 MB."),
            }
        }
    }
    let input = upload.fields().await?;
    if !errors.is_empty() {
        return req.invalid(&errors, ListingForm { input: &input, errors: &errors }, page).await;
    }
    ..
}
```

The body is read as the handler asks for it. `next_file` reads the text fields before the next
file and keeps them. `fields` reads the rest and reads the fields into `T` by `Form`'s rules (a
422 when they do not fit). A file the handler does not read is skipped, not stored. A file input
left empty sends no file.

To hand a file on without holding it (to an object store or a storage API), read it piece by
piece with `file.chunk()`. Each piece is passed on as it arrives, and the request keeps one piece
at a time, not the file.

`upload.limit(bytes)` bounds the whole body, files and fields together. Set it before reading.
Past it every read returns `UploadError::TooLarge`, which `?` makes a 413 and the error page.
To draw the form again with a message instead, read in one block and match its outcome once,
since any read can find the body over the limit:

```rust,ignore
let read = async {
    let mut photos = Vec::new();
    while let Some(mut file) = upload.next_file().await? {
        photos.push(file.bytes_up_to(10 << 20).await?);
    }
    Ok::<_, UploadError>((upload.fields().await?, photos))
};
match read.await {
    Ok((input, photos)) => { .. }
    Err(UploadError::TooLarge { .. }) => { /* the form, "The photos are over 60 MB together." */ }
    Err(other) => Err(other.into()),
}
```

The fields are not read then (they may come after the files), so the form is drawn from what
the page had. The rest of the body is read past, up to 64 MiB, and not kept: a browser that is
still sending when the answer comes reports a network error and never shows the answer. Past
that the connection is closed. The limit for one file, the types a field accepts and how many files
it takes are the handler's rules, so it checks them and adds a message to `Errors`. The form
is then drawn again with its other fields kept and the message next to the input.

The `content_type` a file arrives with is what the client said, as its name is. It decides which
message the user sees, never how the bytes are sent back: a file named `me.png` may hold HTML.
To serve an upload, read its type from its first bytes and answer it as a `Media`, whose type is
one the app names:
`Media::new("image/png", bytes)`. Kept at a URL that names its bytes (a keyed hash of them,
`/pictures/{id}`), `Public::immutable(..)` lets every cache keep it for a year, cookies or not,
and its handler cannot read who is asking; anywhere else it is `private, no-cache` like the rest
of the app. The watchlist example's
profile picture (`examples/watchlist/src/account.rs`) does both.

A file the user downloads names itself on its `Media`: `.attachment("shipments-2026-10-29.csv")`
to save it, `.inline("packing-slips.pdf")` to show it where the browser can and save it under
that name. It stays private unless the handler says `Public`, so a user's export is not cached
for anyone else. Corten writes a name that is not plain ASCII as `filename*=UTF-8''..` (RFC 6266),
which browsers read, and a quote or line break in a name taken from user data cannot change the
header. The browser still cleans the name it saves (a `/`, a `:`, a control character), and an
empty name lets it name the file from the URL. Link to a download with `download`
(`<a href={export} download>`), or `data-reload` for a file the browser shows: without one, the
runtime fetches the link on hover and again on click, running the handler twice.

```rust,ignore
async fn export(caller: Caller<Viewer>) -> corten::Result<Media> {
    let csv = shipments_csv(&caller).await?;
    Ok(Media::new("text/csv", csv).attachment("shipments.csv"))
}
```

A browser cannot be given its files back by markup. A patch keeps the files chosen in the input
it replaces until a successful answer, so the user can send them again. Without JavaScript the
page asks for them again. A body that is not multipart is a 415, and one that breaks the format
is a 400. A slow upload may need its area's own deadline (`Area::deadline`, see
[Cancellation](cancellation.md)).

## Filters and lists that grow

A list's filters are a GET form: every choice is in the address, so the results can be shared,
bookmarked, reloaded and reached with Back, and the form works without a script. To apply a
filter as soon as it changes, a handler submits the form (`form.requestSubmit()` on `change`).
Mark the form `data-keep-place` and its navigation keeps the user where they were: the scroll
position stays, and focus returns to the control they changed (by id, else by its name, and its
value for a checkbox or radio), with its caret. The page keeps what the user sees around it too: an element that
scrolls on its own (a filter rail) keeps its scroll, and a `<details>` open on screen stays open
while one they closed stays closed, so the control is in view and can take focus even when the
server draws a group open only while one of its options is chosen. Without the mark a navigation goes to the top of
the new page, as a link should. A link can carry the mark too. A submit button's `formmethod`
decides how the form goes, as without the runtime: a POST button in a GET form (Save next to
Apply) is the browser's own post, and a GET button in a POST form is swapped in like any GET.
A `data-patch` form reads its button the same way: an unknown `formmethod` (`put`) is a GET, an
empty `formaction` is the page's own address, a `formenctype` of `multipart/form-data` sends
multipart, and a `dialog` method closes its dialog without sending anything. Its body is the one
the browser would send: a file field is the file's name unless the form is multipart, line
breaks are CRLF, and `text/plain` is `name=value` lines ending in CRLF, nothing escaped, with
`content-type: text/plain`. An `on:submit` handler that calls `event.preventDefault()` while the
event is dispatched (a `confirm()`, a check of its own) stops it, as it stops any form; one that
waits for something first (a `<dialog>` that asks) cannot, so it cancels first and sends the form
again itself (`form.requestSubmit()`). One that stops the submit's propagation keeps it from the
page's outer handlers, and the runtime still sends it. Its answer always
patches the page it was sent from: a `target` or `formtarget` on it is not followed.

```rust,ignore
<form method="get" action={url::Search} data-keep-place on:change={js::apply(target)}>
    <label><input type="checkbox" name={f.in_stock} checked={q.in_stock}>"In stock"</label>
    <button>"Apply"</button>
</form>
```

A list that grows (Load more, the next page of results) answers with only the new rows:
`view::Append(ResultsId, rows)` adds them at the end of the element with that id, and the rows
the page has stay as they are. Load more is a GET form with `data-patch`, so without JavaScript
it loads the page with one page more. `req.patch_or` answers both: the patch with JavaScript, and
the page otherwise (its closure runs only then):

```rust,ignore
let more = html! { {Append(ResultsId, html! { for r in &page { <Result r={r} /> } })} <LoadMore next={next} /> };
req.patch_or(more, || async { req.page(results_page(&req, &q, next)) }).await
```

The address does not change with an appended page, so a reload shows the first page again;
a list whose length must survive a reload puts it in the address with a link instead.
`examples/watchlist`'s news page has both.

## Redirects

After a successful write, `req.redirect(&route)` sends the browser on (post/redirect/get), with
and without JavaScript. It takes a typed route, a return address from outside the code that
`corten::Local::parse` checked is on this site (see the app guide's sign-in item), or an
`axum::http::Uri` for a URL outside the app; a path written as text does not compile.

To bring the browser back to what the user acted on (a review on page 2 of a long list, a row
after an edit), redirect to a place on the page: `route.at(ReviewId(&id))` is the route with
`#review-7`, taken from the element's `#[derive(Id)]` type, so the markup and the URL cannot
spell it differently. `href={route.at(..)}` links to one the same way, and a link to an element
on the page it is in (a validation summary's link to the field it names, a skip link) is
`href={FeedbackId.fragment()}`, a bare `#feedback` written the same way: it is a jump within the
document, never a request, so it is an attribute value and not an address `req.redirect` takes.
The browser scrolls to a route's place after a full load and after the runtime's navigation
alike; the runtime fetches the page again even when only the `#id` differs from the address,
since the write may have changed it.
A client navigation the server sends to a place gets `corten-location` instead of a 303,
because `fetch` follows a redirect but drops its fragment. So does one sent to another site
(an `axum::http::Uri` such as `https://pay.example/..`), which `fetch` may not read (the CSP's
`connect-src`, else CORS): the browser goes there itself, in one request to the app. The same
holds for `req.moved`.

A place on a page outside the app (another site, another app of the same product) has an id
that page's markup owns, so it is text: `Elsewhere::at(&uri, "fee-calculator")` is
`https://example.com/compare?x=1#fee-calculator`, and `req.moved` and `req.redirect` take it.
An `axum::http::Uri` alone cannot carry it: it drops a `#fragment` it parses.

```rust,ignore
async fn report(req: Req, Params(at): Params<url::Report<'_>>) -> HtmlResult {
    // ... record the report of at.review on at.slug ...
    req.redirect(&url::Account { slug: &at.slug, reported: Some(&at.review) }.at(ReviewId(&at.review)))
}
```

A page handler may redirect too (an old URL; a `Caller` sending someone to sign in does the
same). A client
navigation follows it: the page it was sent to is swapped in and replaces the requested URL
in the history, so Back skips the URL that redirected.

When a page has moved for good (an item asked for by an old slug or its id, a renamed
category), answer `req.moved(&route)`: a `301 Moved Permanently`, so search engines give the
old URL's place to the new one, and the browser keeps the redirect for a day (its own copy
only; a shared cache does not) and goes straight there next time. Any parameter the new URL
needs is a field of its route; the old URL's other parameters are not carried over. A
`data-patch` form that gets it is sent on, as by `redirect`. Use `redirect` after a write.

## A read that has not caught up yet

A write is often read back from a projection that is a moment behind: the page after the
redirect says "saved, updating" and should read itself again shortly. `ReadAgain`, among the
page's head tags, asks for that:

```rust,ignore
let again = cart.behind(&receipt).then_some(ReadAgain { after: Duration::from_secs(2) });
req.page(html! { <Layout req={&req} title="Cart" head={again}>{cart_view(&cart)}</Layout> })
```

However the page was reached (a full load, a link, a `data-patch` form's redirect), the runtime
fetches the same URL after the delay and swaps it in: the history entry is replaced, the scroll
position stays, the focused element (found again by its id) keeps focus, an answer the same as
the last read's is not swapped, and a hidden tab waits until it is shown. Each answer asks again or not, so the reads stop once the write is in; give up after a
while (the receipt's age) so a page never polls without end. A navigation away stops them.
Without JavaScript the browser reloads the page (`<meta http-equiv="refresh">` in `<noscript>`).

A `data-patch` form's answer can be in the same state: the write is in, but the read that
draws the part it replaces is behind. Rather than wait for the read, or redirect to a page only
to show "saving", the patch says where to read it again: `ReadAgain { after }.from(&route)`, a
patch instruction like `Remove` and `Append`. After the delay the runtime fetches the route as a
GET `data-patch` form would (`corten: action`) and applies the patch it answers, which asks again
while it is still behind. The route's URL carries what the read waits for (the write's receipt),
since the page's address does not; its handler answers the part with `req.patch_or`, and so the
page without JavaScript, at the same URL:

```rust,ignore
async fn pause(req: Req, Params(at): Params<url::Pause<'_>>) -> HtmlResult {
    let receipt = alerts::pause(&at.id).await?;
    let from = url::Alerts { after: Some(&receipt) };
    if req.mode != Mode::Action {
        return req.redirect(&from); // without JavaScript, nothing to read here: the page reads it
    }
    let alerts = alerts::read(&receipt).await?;
    let again = alerts.behind.then(|| ReadAgain { after: Duration::from_secs(2) }.from(&from));
    req.patch(html! { <AlertList alerts={&alerts} /> {again} })
}

async fn alerts(req: Req, Params(at): Params<url::Alerts<'_>>) -> HtmlResult {
    // With patch_or: the list, with `ReadAgain::from` while behind, as the patch; the page otherwise.
}
```

The reads wait while the tab is hidden or a form's answer is on its way, keep what the user
typed, opened and focused, as any patch does, and stop once an element the patch put on the
page is replaced (a later patch of it, a navigation away) or a read fails. A read that fails without a
patch (the error page: the server has logged it) or offline stops them quietly and leaves the
part as it was; any answer that is a patch is applied whatever its status, so a handler whose
read failed after the write can say so in place (`req.patch_with_status(StatusCode::BAD_GATEWAY,
..)` in `Mode::Action`: "Saved; reload to see it"). Only a success that is not a patch is logged
in the console: the route answered its page, not the part.

A page can ask the same of its parts, with the same `ReadAgain::from` among its markup: when the
whole page would be the wrong thing to read again (a form the user is filling in, a payment
provider's frame that would lose what was typed), only the parts are. However the page was
reached, the runtime starts the read once the page is on the screen, as for a patch; it stops
when the page is left, or an element with an id beside the tag is replaced (a form's answer,
which brings its own read). Without JavaScript the tag does nothing, so a page that should
still update then puts `ReadAgain` in its head as well: with JavaScript a page that reads parts
again is not read whole.

```rust,ignore
let again = alerts.behind.then_some(ReadAgain { after: Duration::from_secs(2) });
let parts = again.map(|again| again.from(&from)); // the route answers the list as a patch
req.page(html! {
    <Layout req={&req} title="Alerts" head={again}>
        <AlertForm />
        <AlertList alerts={&alerts} />
        {parts}
    </Layout>
})
```

`examples/watchlist`'s alerts page has all three.

A page whose changing part a `<corten-live>` relays ([live data](live-data.md)) already keeps
itself fresh with JavaScript; reading it whole would replace the `<corten-live>` and open its
stream again. While the job the stream follows runs, it asks a browser without JavaScript only:

```rust,ignore
let again = job.running.then_some(ReadAgain { after: Duration::from_secs(2) }.without_javascript());
```

With JavaScript the `<corten-live>` alone keeps the page fresh, so its stream should send the
job's last state before it ends: when a stream ends, the element stays as its last fragment drew
it, and nothing reads the page again. Only a stream that follows the job counts: a page whose
`<corten-live>` relays something else (a price ticker) while it waits for a write keeps plain
`ReadAgain`.

Do not send a `Refresh` header: Chromium, Firefox and WebKit keep a full load's `Refresh` after
the runtime has navigated away, and then load the first URL again. Debug builds answer 500 to a
page that sends one.


## Errors

A page handler returns `HtmlResult`; any `std::error::Error` converts with `?` and becomes a
500. A record that is not there is `Error::not_found()` (`load(id).await?.ok_or_else(Error::not_found)?`),
and `Error::status(StatusCode::FORBIDDEN)` (or `.with_status(..)` on an error) sets any other
status. The app's error page is rendered in the request's mode (a page or a swap) and
the message is logged, never shown, except under `corten dev`. An extractor's own rejection (a
path segment that does not parse, a form of the wrong type) gets the error page too when a
browser asked. The page is given a `Failure` that says which case it is
([error pages](app.md#error-pages)).

A client that asked for JSON, every route whose handler returns `JsonResult<T>`, and every
request under an [`App::api`](app.md#json-only-routes) prefix, gets every failure in one shape: `{"error":{"status":404,"message":"Not Found"}}`. A
4xx carries the error's message or the rejection's text; a 5xx carries only the status's
reason. Validation errors add the fields:
`{"error":{"status":422,"message":"email: ...","fields":{"email":"..."}}}`, and a message about
the whole form (`Errors::add_form`) is `form` too.
