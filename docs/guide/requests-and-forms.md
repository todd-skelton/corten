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
| any, under an `App::api` prefix | a webhook, a service, a partner's site | JSON (`corten::Json`), errors as Problem Details (`application/problem+json`) | reads the JSON |
| SSE | `<corten-live src>` | `corten::live(stream_of_views)` | applies each like a patch |
| `GET {feed}`, `{feed}/live` | a handler's `subscribe()`, or `Replica` | `feed.snapshot()`, `feed.live(since)` | snapshot, then ordered patches |

One handler serves every mode, and `req.mode` says which. A `data-patch` form's handler answers
both of its rows with `req.patch_or(view, || async { req.page(..) }).await`: the patch with
JavaScript, and otherwise the page (or a redirect), whose closure runs only then, so the patch
never loads what only the page shows. A patch that needs data of its own, which the page does not,
or a status or headers of its own, checks `req.mode == Mode::Action` itself; a refusal whose
form needs a read is `req.invalid_with` (below). A route with no screen goes under
[`App::api`](app.md#json-only-routes) instead, where every answer is JSON. The layout around `<main>` renders on full
loads only ([the document](views.md#the-document)); every page carries a key hashed from the
caller its handler read (if any), the deploy and its layout, and when a navigation answers with a different key
(sign-in, a new release, a page in another layout) the runtime loads the whole document instead, so the layout is
never stale.

While a request is on its way, the runtime marks what waits for it with `aria-busy="true"`: a
`data-patch` form until its answer comes, and for a navigation `<main>` and the link or GET form
that started it (through a redirect too), until the new page is swapped in. When another
navigation replaces one, the first one's marks go at once, and the newer one's stay. Showing which control is working is then CSS
alone, in the component's stylesheet:

```css
.reports a[aria-busy]::after {
  content: " …";
}
```

Back and forward mark only `<main>`, since nothing on the page started them, and a page reading
itself again marks nothing.

## Forms

A form is for what the server has to answer: a write, or data the page does not have. Sorting,
filtering or totalling what the page already shows is faster in a handler, with no request
([Server or browser](scripts.md#server-or-browser)).

`#[derive(Fields)]` gives a form struct its field names as `FieldName` constants, so markup and
validation cannot misspell them: `Errors` takes nothing else. A name carries the type its control
sends (`condition: Option<Condition>` is a `FieldName<Condition>`), so a control that takes a typed
name (`SubmitKey::field`, corten-ui's `Picker`, `Number` and `RequiredWhen`) cannot be given another
field's; `name={..}` and `Errors` take a name of any type, and `.untyped()` drops the type for a
list of names of several fields (`[f.email.untyped(), f.price.untyped()]`). Validation stays in the app; `Errors` carries the first message per field.
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

A refusal whose form needs a read of its own (the batch as it is now, for "someone moved it
on") is `req.invalid_with`: the form side runs only for the patch, the page side only for the
page, and a data client gets the Problem Details document (feature `json`) with neither read. The form side gets
a `FormPatch`, which draws a patch and nothing else, so it cannot answer a page by mistake:

```rust,ignore
return req
    .invalid_with(
        &errors,
        |form| async {
            let batch = store.batch(id).await?;
            form.draw(BatchPanel { batch: &batch, errors: &errors })
        },
        || page(&req, &store, id, &errors),
    )
    .await;
```

A JSON handler has no form to show again: it `?`s the same check, next to the `?` of a load
that failed. `JsonResult<T>`'s error is a `JsonError`, which takes either: field errors are
the 422 with `fields`, anything else is answered as any failure of a JSON route
(`examples/card-market`'s `POST /api/alerts`):

```rust,ignore
async fn alert_json(State(source): State<Source>, Json(input): Json<CreateAlert>) -> JsonResult<AlertSet> {
    let price = check(&input)?; // 422, {"title","status","detail","fields":{"email":".."}}
    let item = source.item(&input.id).await?.ok_or_else(Error::not_found)?; // 500 or 404
    Ok(Json(AlertSet::new(&item, price)))
}
```

On a page, `?` on `Errors` does not compile: `HtmlResult`'s error is an `Error`, and a
page's form answers with `req.invalid`, which shows the form again. Under an API with its own
error shape (`Api::errors`), the 422 stays a Problem Details document with its `fields` and
`form` members, since a status and a message have no place for them; every other failure is
written in the API's.

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
`#[serde(default)]` for anything else, as a link leaves those out. A word an enum parameter
does not have (`?state=due_now` after the app renamed it, or an address edited by hand) reads as
missing too, so a filter, a sort or a tab falls back to its default and the page still shows: a
bookmark outlives the words it was written with; a `#[route(prefix = ..)]` entry whose key or
value is such a word is left out of the map. A parameter with neither `Option` nor a default
still refuses it (400), and so does a value that is not a word (`?page=two`), a list holding one,
or a form's body, which the page that sent it just wrote. In a `#[corten::routes]`
module a route with a parameter in its path holds each of its handlers to taking its own
`Params`: one that reads the path another way (axum's `Path`), or reads another route's, does
not compile.

A parameter may be followed by plain text in its segment, as a file's extension is:
`#[route("/sitemap/{kind}/{page}.xml")] pub struct SitemapPage<'a> { pub kind: &'a str, pub page: u32 }`
links to `/sitemap/items/3.xml`, and `Params` reads `page` as `3`. A segment that does not end in
the text (`/sitemap/items/3.json`), or is only the text, is a 404 before the handler runs,
whatever its method; one whose value is not the type (`x.xml`) is a 400, as any parameter is,
and the value reads as any path parameter's does (a `bool` is `true` or `false`).
The router matches the segment whole, so the route takes every path of its shape, as
`/sitemap/{kind}/{page}` would: a `/{*page}` catch-all does not answer `/sitemap/items/3.json`
either (`req.serves` agrees), and the app's middleware sees that 404 with the route's
`MatchedPath`. Two routes of a module that differ only in that text (`{name}.rss` and
`{name}.atom`) do not compile, and two in different modules, areas or APIs stop the app at
startup: give each a segment of its own. A route with a parameter where another has its catch-all,
after the same segments (`/{name}.txt` and `/{*page}`), which the router cannot hold, does not
compile either when both are in one module, and stops the app at startup otherwise;
`/docs/{name}.md` beside `/{*page}` is fine. Text before a parameter (`item_{id}`) does not
compile.

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

A set's plain field refuses the page when its key is sent twice (a link edited by hand, an old
one), and a list field reads every value. When the value is a rule over what was sent (a listing
that counts only when every `listing` sent names the same one, `filtered` when any `dimension.*`
has a value), make it a plain struct that implements `corten::route::FromQuery` (its error is a
`corten::form::FormError`) with two derived sets of the same names: `Link`, which writes its
links as narrow as the value, and `Read`, which reads every value sent for `from_query` to judge:

```rust,ignore
#[derive(Route)] #[route(query)]
pub struct ChoiceLink<'a> { pub listing: Option<&'a str>, #[route(prefix = "dimension.")] pub options: &'a [(String, String)] }

#[derive(Route)] #[route(query)]
pub struct ChoiceRead { pub listing: Vec<String>, #[route(prefix = "dimension.")] pub options: Vec<(String, String)> }

pub struct Choice { pub listing: Option<String>, pub options: Vec<(String, String)> }

impl FromQuery for Choice {
    type Link<'a> = ChoiceLink<'a>;
    type Read = ChoiceRead;
    fn link(&self) -> ChoiceLink<'_> { ChoiceLink { listing: self.listing.as_deref(), options: &self.options } }
    fn from_query(read: ChoiceRead) -> Result<Self, FormError> { /* the rule */ }
}

#[derive(Route)] #[route("/items/{slug}")]
pub struct Item<'a> { pub slug: &'a str, #[route(query)] pub choice: Choice }
```

The route reads it back as its `Params` field (`Read`, then `from_query`), so every handler gets the
rule already applied, and no parameter name is written as a string. A `Read` and a `Link` with
different names do not compile, and the route checks the set's names against its own. `from_query`
must give back the value `link` wrote, or a link and its handler disagree: test the round trip
through the app's router (a link's `href()` requested, and the page shows the value). Its error
refuses the page with a 400, as a parameter that does not read does (`serde::de::Error::custom`
makes one; start its message with the parameter's name). A derived set cannot implement `FromQuery` too. A set of plain fields stays derived.

A parameter is named as serde reads its field, so `#[serde(rename_all = "camelCase")]` or a
field's `#[serde(rename = "priceMin")]` renames it in links as well: addresses that are already
shared or indexed keep working, and the link and the handler cannot disagree. A path parameter
keeps the name the path gives it, so renaming one does not compile.

A parameter renamed after its addresses were shared keeps reading its older name in a `FromQuery`
set: a field of its `Read` set marked `#[route(read_only)]`, which `Params` reads and nothing
writes, and `from_query` takes the new name, or the older one when the new one is missing. Only a
`Read` set has one: a set with one is not a set a link or a route writes, and a route's own field
cannot be one, so no link can be given a value for it that goes nowhere. A route that includes the
set (one without type parameters) checks its own names against the older one too, so no other
parameter can take it:

```rust,ignore
#[derive(Route)]
#[route(query)]
pub struct TermsRead { pub q: Vec<String>, #[route(read_only)] pub search: Vec<String> }
// from_query: read.q.into_iter().chain(read.search).find(|t| !t.is_empty())
```

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

A group of fields several forms share (an address) is a `Fields` struct of its own, put in a form
with `#[fields(group)]`. Its fields post under the field's name, `ship_to.street`, so a form can
hold two of one type (where it ships from and to), each read into its own value as its own type (a
number stays a number). A component draws the group once for every form that has one, given its
names: `<AddressInputs f={f.ship_to} />`, where `f: AddressGroupFields` (the
names of an `Address` put in a form as a group; in a row it is `AddressRowGroupFields<'k>`, so a
group drawn both at a form's top and in its rows takes a component for each). A group whose
fields a form can all leave out (checkboxes, `Option`s, file inputs) needs `#[serde(default)]` on the group's
field as well, or it does not compile: a form that sends none of them would otherwise be refused.
A group is never an `Option` (that does not compile either): a form sends a group's fields one by
one, so a group the form may leave out is a group of `Option`s with `#[serde(default)]`.
Keys under a field's name (`go.x`, `go.y`) are its group's, so a plain field sent only that way (an
image button `name={f.go}` sends `go.x` and `go.y`, never `go`) is refused with a message that
says so, unless it is an `Option`, which reads as `None`.

Rows of the same fields (one per API domain, a line of an intake, a shipment) are a map from each
row's key to a `Fields` struct, `#[fields(rows)] #[serde(default)] domains: BTreeMap<Domain,
DomainLimits>`. A row's field is named by its key, `f.domains.at(&row.domain).max_concurrent`,
posted as `domains[seller-portal].max_concurrent`, so it is the key, not the row's place in the
form, that puts each value in its row: a row the browser leaves out (its inputs disabled) leaves
the others where they are, and an unchecked checkbox is `false` in its own row. A row is in the
map when the form sends any of its fields, so give each row one the browser always sends (a
value, or a hidden input); a row of checkboxes alone, all unchecked, is not read at all. `at`
takes only a reference to the map's key type. An error on one row's field marks that input only, with its own message id
(`limits-domains[seller-portal].max_concurrent-error`):

```rust,ignore
#[derive(Deserialize, Fields)]
struct DomainLimits { max_concurrent: u32, #[serde(default)] adaptive: bool }

#[derive(Deserialize, Fields)]
struct Limits { #[fields(rows)] #[serde(default)] domains: BTreeMap<Domain, DomainLimits> }

let f = Limits::FIELDS;
html! {
    for row in rows {
        let max = f.domains.at(&row.domain).max_concurrent;
        <input name={max} value={row.max_concurrent} {..e.aria(max)}> {e.message(max)}
    }
}
errors.add(f.domains.at(&Domain::SellerPortal).max_concurrent, "At least 1.");
```

A key is a `RowKey`: text, an integer, or a `#[derive(UrlValue)]` enum, whose key is the word
serde reads (`seller-portal`), so it reads back as the same variant. Its characters other than letters, digits, `-` and `_` are %-encoded in the name, so any text is
a key, and a key sent any other way (`%61` for `a`) is refused, so one key cannot reach a row two
ways. An integer key is read only as it writes itself: `07`, `+7` and `-0` are refused, for the
same reason. An enum with a variant serde reads by more than its word (`#[serde(alias)]`,
`#[serde(other)]`) is not a `RowKey`, so naming its row does not compile: two keys the form tells
apart would be one row on the server.

One plain value per key (a count per SKU) is a map without a row struct:
`#[fields(map)] #[serde(default)] counts: BTreeMap<Sku, u32>`. Each key's value is a field of its
own, `f.counts.at(&sku)`, posted as `counts[sku]`, with its own error, and its keys follow the
rules above.

A row holds plain fields and groups. A group of groups, rows in a group and rows in a row
do not compile, and neither does `#[serde(flatten)]`: a group is named under its field so two of
one type cannot clash.

A refusal no single field explains (a rule across fields, or a service the handler wrote to
saying no: "The policy was retired meanwhile.") is the form's own message:
`errors.add_form(text)`, shown with `{errors.form_message()}` at the top of the form, a
`role="alert"` paragraph that is announced when the patch arrives. JSON clients get it first in
`message` and as `form`; an agent tool gets it first in its error.

A submit refused for the state it was made against rather than its input (a stale revision, a step
already taken or out of order, a record deleted meanwhile), or while another request acts on the
same thing ([below](#work-that-must-not-run-twice-at-once)), is
`errors.refuse(Refusal::Conflict, text)` (`Refusal::Gone` for a deleted one). It is the form's
message, and `req.invalid` answers it with 409 (410) in every mode instead of 422: the form drawn
again with what the user typed, the page without JavaScript, the Problem Details document for a data
client. A service the handler calls for the submit that failed or did not answer (a carrier, a
payment provider, another app) is `errors.refuse(Refusal::Upstream, text)`: 502, the same three
answers, with the form drawn again so the user can send it once more. The service may have applied
it, so the retry must be recognized as the same work: by its `SubmitKey`, or the handler checks
what is done already under a lock and sends the service a key made from what it acts on
([below](#work-that-must-not-run-twice-at-once)). The runtime keeps the
form's `SubmitKey` (below) over whatever key the patch drew, and the form drawn for the page without
JavaScript writes the posted key again, `input.submit.again(f.submit)` where it would write
`SubmitKey::field(f.submit)`. A proxy in front of the app that swaps 5xx bodies for a page of its
own (nginx's `proxy_intercept_errors`, a CDN's error pages) hides the redrawn form; let the app's
502 through. A failure of the app itself (its database) is no refusal: it is `?`'d, and the error
page answers it. A post that can be refused is a `data-patch` form: a plain post's refusal draws the
whole page and leaves the address at the action's URL, whose GET may not exist. The one exception is
a download, which must be a plain post ([below](#files)): its refusal redirects to the page that
says why.

A 502 or a dropped connection can hide a write that committed, and the user, the browser or the
runtime then sends it again. Add a `SubmitKey` field to the form struct and draw it with
`SubmitKey::field`, a hidden input with a fresh random UUID v4. A UUID is what most APIs take as
an idempotency key, so the app passes the key on as it is for a write that belongs to this
submission:

```rust,ignore
#[derive(Deserialize, Fields)]
struct OrderInput { quantity: u32, submit: SubmitKey }

html! { <form method="post" action={url::Orders} data-patch>{SubmitKey::field(f.submit)} .. </form> }

async fn order(req: Req, State(db): State<Db>, Form(input): Form<OrderInput>) -> HtmlResult {
    // In the same transaction as the write: a key seen before answers with the first outcome.
    let placed = db.place_once(input.submit.as_str(), input.quantity).await?;
    req.redirect(&url::Order { id: placed.id })
}
```

A write that only the submission identifies (placing a new order, sending a message) carries a
`SubmitKey`. Work on something the app's data already names (one label per order, one charge
per invoice) takes a lock and leaves out what is done already instead, with no `SubmitKey`, and
gives the provider a key made from what it names
([below](#work-that-must-not-run-twice-at-once)).

The same key is sent for as long as the submission has no answer: a page reloaded after a post
posts it again, and the runtime sends a `data-patch` form again with it after a network error, a
5xx, an abort (a navigation) or an answer whose body was cut off on the way. Once a whole answer
arrives, any status below 500, the runtime gives the form a new key, so the next submit is a new
one. A patch never sets the key: the runtime keeps the form's own over the one the patch drew, so
a 5xx patch (`Refusal::Upstream`) cannot swap in a fresh key before the retry. A form sent again before the first had an answer (a
browser without JavaScript, a data client) is the same submission: the app answers it with what
it did the first time, and the user sees that and can send the change. A `data-patch` form's
change made while its write is out waits for the answer instead, and goes under the new key
(below). The key is the app's to store, next to the write (Corten keeps
no store); `corten_cluster::begin` takes it as its idempotency key. A field that is not a UUID v4
(in either case; it is kept lowercase) is a 422 that names it. Make the field
`Option<SubmitKey>` when data clients post the same form without one.

A key makes one submission apply once. It does not keep two submissions apart: two tabs that
each press "Buy postage" send two keys, and both run. That takes a lock and a check of what is
done already ([below](#work-that-must-not-run-twice-at-once)).

The key is a form field and not the `Idempotency-Key` header that some HTTP APIs read. A browser
without JavaScript cannot set a header, so a field is the one way that works for every client,
and a data client posts the same field. The header is still an IETF draft
(`draft-ietf-httpapi-idempotency-key-header`); a second place for the key would wait for it to
be a standard, with a rule for a request that sends both.

A `data-patch` form has one submit in flight. Pressing the same button again with the same values
while it is pending sends nothing (a double click). A newer submit of a read (a GET: a search, a
filter) aborts the older one, whose answer is worth nothing now. While a write is pending, any
newer submit of the form waits instead: the server may already have applied the write, and its
answer is the page's next state. Once the runtime has drawn the write's answer (a patch or a
page) and it took the write (2xx) or refused only its input (422), it sends the latest of the submits that waited, from the form as it is then,
so the version or count the answer wrote goes out with it, under the form's new key. Its
`on:submit` handlers ran at the press and do not run again, and the browser does not check the
form's constraints again (the server checks what it gets). Several presses while the write is
out are one submit of where the form ended. A form the answer replaced is found by its `id`, so
give a form that a patch redraws, or that sits in an element a patch redraws, one: a form that
left the page without an id is not sent, since its values are the ones from before the answer.
The waiting submit is dropped, and the user sends it again, when the write gets no answer (the
network failed, or the browser is leaving the page) or one the runtime does not draw (a proxy's
502), which `corten:error` reports; when the write is refused for the state it was made against
(`Errors::refuse`, a 409 or 410), which the user reads before anything else goes; or a 5xx,
after which the server may or may not have applied the write; when the answer sends the browser elsewhere (a redirect,
a reload), or removed the form or the pressed button; and a navigation aborts everything, so a
late answer never patches a page it was not meant for. A screen whose every press is a step of
its own (a count's + and -, each a delta the server adds) keeps its own ordered queue in its
script: the form sends where the user ended up, not each step.

When a `data-patch` submit gets no answer the runtime can draw, it dispatches `corten:error` on
the form (it bubbles), with `detail.status`: the answer's status (a proxy's 502 page), or 0 when
there was none or its body was cut off (`detail.error` says why). A submit abandoned for a newer
one or a navigation dispatches nothing. The form keeps what the user typed, so the page can say
what happened and the user can send it again: with the same key after a 5xx or no whole answer,
and with a new one after a whole answer below 500, as above. A script
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

Rows known only from data (the fields of the items a write touched) are a `for` among the
patch's parts, with its `let`s and branches, as in a page:

```rust,ignore
req.patch(html! {
    <QueueTotals queue={&queue} />
    for sku in &taken {
        let label = label_of(sku);
        if sku.counted {
            <QuantityField id={FieldId(sku.id)} label={&label} quantity={0} />
        } else {
            {Remove(FieldId(sku.id))}
        }
    }
})
```

A component that says `-> impl Render` is not a patch, so one that is drawn by the page and sent
by an action says `-> impl Patch`, as a layout says `-> impl Document`. When a view is refused,
the error names the kind of part that is not a patch (`ComponentOrExpressionNotAPatch`,
`IdNotADeriveIdType`, `TextOrElementWithoutId`), though not where it is: look for that kind among
the view's top-level parts. A `corten::live`
fragment is already text, so debug builds check it when it is sent instead (it panics, naming the
element).

## Work that must not run twice at once

Some work on something the app's data already names must not run while the same work is running, or
run again once it is done: a paid call for it (postage for an order, a charge for an invoice), a
batch over the same rows. A `SubmitKey` does not cover it, since two submissions (two tabs, two
people) have two keys. A write only the submission identifies (placing a new order, sending a
message) has nothing to leave out, and takes a `SubmitKey` instead ([above](#forms)). The handler
takes a lock, and under it reads what the work changes and leaves out what is done already. The lock
alone keeps two runs apart only while they overlap: a second submit made after the first finished
finds it free.

The lock lives where every instance of the app sees it. A `Mutex` or an `AtomicBool` in a static
holds within one process, and an app is often more than one: a blue/green or rolling release runs
the old and the new side by side while traffic moves, and an app scaled out runs several. A click
each of them answers takes each process's lock, and both buy. In Postgres the lock is an advisory
lock taken in a transaction kept open for the work. It ends with the transaction (a commit, a
rollback, a statement that failed, a connection that closed), and the check and the record of what
was bought run on that transaction, so they commit with the lock's release:

```rust,ignore
/// A kind of work that must not overlap: the first half of Postgres's two-part advisory lock
/// key. One enum numbers them all, so two kinds cannot share a number (the compiler refuses it).
#[derive(Clone, Copy)]
#[repr(i32)]
pub enum OneAtATime {
    BuyingPostage = 1,
}

/// Held while the work runs. Dropping it rolls the transaction back on a task sqlx spawns, so the
/// lock ends a moment later: end it with `release` or `abandon` before it is taken again.
pub struct Held(Transaction<'static, Postgres>);

impl OneAtATime {
    /// `None` while a request in this process or another holds it for `key` (a seller's id).
    /// All of it, the wait for a connection included, takes at most `CALL` (the budget below
    /// counts on it); the pool's `acquire_timeout` is every page's (sqlx's default is 30 s).
    pub async fn take(self, pool: &PgPool, key: i32) -> sqlx::Result<Option<Held>> {
        // Past it, an I/O timeout: the lock or the database was slow, not only the pool.
        let late = |_| sqlx::Error::Io(std::io::ErrorKind::TimedOut.into());
        timeout(CALL, self.take_now(pool, key)).await.map_err(late)?
    }

    async fn take_now(self, pool: &PgPool, key: i32) -> sqlx::Result<Option<Held>> {
        // Each statement sees what committed before it, so the check sees the last holder's
        // record whatever the database's default is.
        let mut tx = pool.begin_with("BEGIN ISOLATION LEVEL READ COMMITTED").await?;
        // Postgres cancels a statement after 5 s. A statement that fails aborts the transaction,
        // which releases the lock at once (and undoes these `SET LOCAL`s), so the work stops at
        // any database error (`?`), never catching one and carrying on. A transaction left
        // waiting 10 s between statements (above one carrier call) ends with its session, and
        // the lock with it: a host cut off.
        sqlx::query("SET LOCAL statement_timeout = '5s'").execute(&mut *tx).await?;
        sqlx::query("SET LOCAL idle_in_transaction_session_timeout = '10s'").execute(&mut *tx).await?;
        let taken: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1, $2)")
            .bind(self as i32)
            .bind(key)
            .fetch_one(&mut *tx)
            .await?;
        Ok(taken.then_some(Held(tx)))
    }
}

impl Held {
    /// The transaction the lock is in: the work's reads and writes go here.
    pub fn conn(&mut self) -> &mut PgConnection {
        &mut self.0
    }

    /// Ends the hold, keeping what it wrote.
    pub async fn release(self) -> sqlx::Result<()> {
        self.0.commit().await
    }

    /// Ends the hold now, keeping nothing it wrote.
    pub async fn abandon(self) -> sqlx::Result<()> {
        self.0.rollback().await
    }
}

/// How a buy went. Each label the carrier returned is recorded, unless the database fails (case 1
/// below).
enum Bought {
    /// Every order has a label: bought now, or already.
    All,
    /// Stopped with orders left: the budget was spent, or the signal aborted (the client left,
    /// shutdown began).
    Unfinished,
    /// The carrier failed or did not answer for an order.
    Failed,
    /// Another request holds the lock (before the first order, or between two).
    Busy,
}

/// How long a buy starts work on orders for, and how long one carrier call may take. No order and
/// no call starts with less than `CALL` left, so the calls end within `BUDGET`. On a live
/// connection the last call's record and commit take at most a statement timeout each (5 s); an
/// order started near the end takes at most all of `take` (`CALL`), then the check and the end of
/// the hold at their statement timeout, before the next stops. Either way 25 s in all, which leaves
/// 5 s of the route's 30 s deadline for the rest of the request (the body, the caller, the answer's
/// read).
const BUDGET: Duration = Duration::from_secs(15);
const CALL: Duration = Duration::from_secs(5);

async fn buy(
    req: Req,
    signal: Signal,
    Caller(seller): Caller<Seller>,
    State(db): State<Db>,
    Form(input): Form<BuyInput>,
) -> HtmlResult {
    let (shared, orders, seller) = (db.clone(), input.orders.clone(), seller.id);
    // On a task that finishes if the client leaves: a label bought but not recorded is paid for
    // and lost. Its signal stops it starting calls, never a call under way, and it commits what
    // it bought.
    let bought = signal
        .finish(move |signal| async move {
            let end = Instant::now() + BUDGET;
            let stop = || signal.is_aborted() || end.saturating_duration_since(Instant::now()) < CALL;
            for order in &orders {
                if stop() {
                    return Ok(Bought::Unfinished);
                }
                // A lock for each order, committed with its record: a run that ends anywhere (a
                // lost connection, a killed process) loses at most the record of the call under
                // way. Keyed by what must not overlap (the seller whose orders these are), taken
                // from what the server checked (the caller), never from an unchecked form value.
                let Some(mut held) = OneAtATime::BuyingPostage.take(&shared.pool, seller).await? else {
                    return Ok(Bought::Busy);
                };
                // Under the lock: whether this order is the seller's and has no label yet (one
                // statement, as `record` is). Every hold is ended before the run goes on or
                // answers: a dropped one ends a moment later, and the next `take` (this run's, or
                // the user's resend) could find it still held and refuse.
                if !shared.unlabeled(held.conn(), seller, order).await? {
                    held.release().await?;
                    continue;
                }
                if stop() {
                    held.abandon().await?;
                    return Ok(Bought::Unfinished);
                }
                // The carrier is sent the order's id as its idempotency key (below).
                match timeout(CALL, shared.carrier.label(order)).await {
                    Ok(Ok(label)) => shared.record(held.conn(), order, &label).await?,
                    // Here every carrier error stops the batch as a failure to answer; what it
                    // bought before is committed. A carrier that answers no (a bad address) is
                    // better the app's own 422, `errors.add_form` naming the order, since a resend
                    // would fail the same way.
                    Ok(Err(_)) | Err(_) => {
                        held.abandon().await?;
                        return Ok(Bought::Failed);
                    }
                }
                held.release().await?;
            }
            Ok::<_, Error>(Bought::All)
        })
        .await?;
    let (why, text) = match bought {
        Bought::All => return req.redirect(&url::Labels),
        // Not a failure: what is left goes out with the next submit.
        Bought::Unfinished => {
            let left = "Some orders are left to buy. Send them again.";
            return req.redirect_with_notice(&url::Buy, left);
        }
        Bought::Busy => (
            Refusal::Conflict,
            "Postage for this account is being bought. \
             Send this again once it is done to buy what it left.",
        ),
        Bought::Failed => (
            Refusal::Upstream,
            "The carrier did not answer for every order. \
             The orders below show what has a label; send this again to buy the rest.",
        ),
    };
    let mut errors = Errors::new();
    errors.refuse(why, text);
    req.invalid_with(
        &errors,
        |form| async {
            let orders = db.orders(seller, &input.orders).await?; // as committed now
            form.draw(BuyPanel { orders: &orders, errors: &errors })
        },
        || page(&req, &db, &errors),
    )
    .await
}
```

What it gives:

1. Two buys for one seller never buy at once, on any instance, while the lock is held (its session
   outlives a host cut off from the database by at most the idle timeout after its last whole
   statement, 10 seconds, kept above `CALL` so a call still running there ends first; one cut off in
   the middle of a statement holds it until the network gives up, which the server's TCP keepalives
   bound, or on Postgres 17 a `SET LOCAL transaction_timeout` set above the work's own bound; case 1
   below), and one after another the second leaves out what the first recorded. Each order is bought
   under its own hold of the lock and committed with its label, so a run that ends anywhere loses at
   most the record of the call under way. A repeat (a second tab after the first finished, a retry
   whose answer was lost) finds nothing to buy and goes to the labels once the first is done (while
   it runs, whichever of the two meets the other's lock stops with the 409 below, and neither buys
   an order twice), so the form needs no `SubmitKey`.
2. A busy lock is `Refusal::Conflict`: a 409 whose form shows the orders as committed, which
   leaves out what the other buy has not committed yet, so the message asks for a resend once it
   is done; the resend buys only what is left. A buy that meets the other between two orders stops
   there, with the labels it bought committed.
3. A carrier that fails or does not answer for an order ends the batch there: the labels bought
   before it commit, and the answer is `Refusal::Upstream` ([above](#forms)), a 502 whose form shows
   what has a label, so the seller sends the rest again. A batch that stops for time (its budget
   spent, the client gone, shutdown) commits what it bought too, and is no failure: a redirect back
   to the form with a notice, where the form lists the orders still without a label. A batch that
   does not fit one request in the normal case goes to [a queue](app.md#a-queue-a-request-fills)
   instead. A failure of the app itself (its database) is `?`'d to the error page, and rolls back
   the order under way (case 1 below).

What it does not give, which is the app's to design for its provider:

1. **A call whose record is lost.** A call that failed or timed out may still have been applied by
   the provider, and its order stays without a label; a database failure after a paid call rolls
   back that order's record; and the lock ends with its connection (a network fault, the
   transaction's timeouts) or its process (killed at shutdown) while a call can still be under way,
   and that call's record rolls back. The next buy then calls again for that order. What answers it
   is the provider's idempotency key, made from what is bought (the order), not from the submission,
   so the call made again is the same purchase. How long a provider keeps a key, and what it does
   with one sent again after a failure, with changed details or while the first call still runs, is
   the provider's; the app makes its keys to fit. A provider with no keys (most email and SMS
   services) cannot be made to send exactly once: the app chooses at most once (a row claimed and
   committed before each call, the call only where the claim was new) or at least once.
2. **Timeouts beyond its own.** The work's worst case on a live connection (the budget, then the
   last record and the commit at their statement timeout: 25 seconds here) is under the route's
   deadline (`App::deadline`, 30 seconds unless set; an [area](app.md#areas) of its own sets
   another), past which the client gets the 503 page while the work goes on, so what the request
   does outside the work (reading the body, the caller, the read the refusal draws) fits in what is
   left, bounded as the app's other reads are. The wait for a connection counts against the budget,
   so `take` bounds all of it itself: the pool's `acquire_timeout` (sqlx's default is 30 seconds) is
   every page's, and an order started late would wait past the deadline. `statement_timeout` bounds
   the database's work, not the app's wait for an answer: a connection lost mid-statement waits
   until the network gives up, and is case 1.
3. **Shutdown.** `serve` waits for a handler whose client is still there up to its deadline, and for
   work whose handler is gone until `App::shutdown_grace` has passed since shutdown began (5 seconds
   unless set, shared with the app's tasks). Shutdown aborts the work's signal, so it starts no more
   calls and, on a live connection, ends within one call, a record and the commit (15 seconds here)
   once a call has started; before a call, nothing paid is under way, and an order's lock and check
   end within their own timeouts. An app with this work sets the grace above that
   (`.shutdown_grace(Duration::from_secs(20))`) and the platform's wait before it kills the process
   above the longer of the deadline and the grace (`docker stop` waits 10 seconds, set by Compose's
   `stop_grace_period`; Kubernetes 30, `terminationGracePeriodSeconds`). A process killed sooner is
   case 1.

Notes:

1. The second part of the key names what must not overlap: a seller's id lets two sellers buy at
   once, and 0 is one lock for all. A key that is not an `int4` (a `bigint` id, a UUID, a name)
   is `hashtext($2::text)`, an internal Postgres function whose 32 bits can collide, which now and
   then refuses a submit that would not have overlapped.
2. The lock's transaction is one pooled connection for as long as an order's work runs, and the
   work uses it rather than taking more. Work that takes a second connection while it holds the lock
   (a claim committed apart from it) holds two, and when concurrent buys' first connections take
   the whole pool, each waits for a second that none can get; a semaphore with fewer permits than
   the pool has connections, taken before the first, leaves room for the seconds. The same
   semaphore, well under the pool's size, keeps a few buys at once from leaving the app's pages
   waiting for a connection (or the work takes a small pool of its own). Work that runs
   for minutes takes a leased row instead, the claim [the queue](app.md#a-queue-a-request-fills)
   shows: taken while its lease is past, renewed while the work runs, and taken over by another
   process once a dead one's lease runs out.
3. Another store's lock is a lease (Redis: `SET name token NX PX 30000`, released only by the
   token's holder), which expires under longer work unless it is renewed, as the queue renews its
   claim. The lock is the app's, over its own store and driver, as its other reads and writes
   are: about thirty lines that only apps with such work need, so Corten gives no lock.
   A schedule that must run once across instances claims each run instead
   ([On a schedule](app.md#on-a-schedule)).
4. The lock holds only between instances that all take it. The release that brings it in (and one
   that changes a lock's kind or key) overlaps instances that do not, so it goes out without the
   old and the new running side by side: the old drained first.

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

A download the handler may refuse (posted from a page, at the revision the page showed, when the
run has moved on) answers `PageOr<Media>` (or `PageOr<Relay>` for another service's file): the
file, or the page saying why, or a redirect to it, each `.into()` it. The user stays on the page
with the reason instead of an error page, and the page is private like any other. The form is a
plain `method="post"` form, not `data-patch`: the runtime draws a patch or a page, and a file
answered to a `data-patch` submit is not saved (it dispatches `corten:error`). Inside `Public`,
name the variant (`.map(PageOr::Page)`), since `.into()` cannot infer the type there. This is the
one refusal that redirects: a plain post's page drawn at the action's URL would leave the user
there, so a stale download answers a redirect to its page with the reason, where a `data-patch`
form's refusal is a 409 drawn in place.

```rust,ignore
async fn download(req: Req, at: Params<url::Download<'_>>, Form(form): Form<At>) -> corten::Result<PageOr<Media>> {
    match claim(&at.id, form.revision).await {
        Ok(csv) => Ok(Media::new("text/csv", csv).attachment("run.csv").into()),
        Err(Stale) => Ok(req.redirect(&url::Run { id: &at.id, said: Some(Said::Stale) })?.into()),
    }
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

A GET form sends only its own controls, and drops its `action`'s query. On a screen whose address
holds more than one form changes (the seller and the page, and the research a second form sets),
each form wraps its controls in `keep_query(&at, &html! { .. })`: the controls are drawn, and every
key of the route's query that none of them names (`name=`, a `#[route(query)]` set's key, a
`#[route(prefix)]` entry's) becomes a hidden input, in the encoding the route's links use. There
is no list of names, so a key is neither dropped nor sent twice, and a field added to the route
later is kept by every such form. Every control the form sends goes inside, a named submit button
included; the route can be a `&dyn Href` a shared form is given. Lent (`&html!`), the controls
borrow what they use, so the page around the form reads the same locals. A form that starts a new
search passes the route with the fields it resets cleared (`&url::Search { page: None, ..at.clone() }`),
so they are not kept.

```rust,ignore
<form method="get" action={&at}>
    {keep_query(&at, &html! {
        <select name={f.grade}>..</select>
        <button>"Price"</button>
    })}
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

### Saying what a write did

The page a write redirects to often says what it did ("Batch 3517 deleted."). That is not part of
the page's address: written into a query value, a reload says it again, a copied link shares it
and Back to the page asks for it again. `req.redirect_with_notice(&to, text)` carries it instead, and the page it
lands on reads it once with `req.notice()`:

```rust,ignore
async fn delete(req: Req, Params(at): Params<url::Delete>) -> HtmlResult {
    store::delete(at.id).await?;
    req.redirect_with_notice(&url::Batches, format_args!("Batch {} deleted.", at.id))
}

// In the layout, where the app says what an action did:
<Toaster close="Dismiss" seconds={5} toast={Toast { message: req.notice() }} />
```

The text rides in a cookie (`__Host-corten-notice`, `HttpOnly`, a minute) for the path it
redirected to (its escapes decoded, so `a%20b` and `a b` are one page); another page neither shows
nor removes it, and the answer of the page it was for removes it, so it is said once. A `HEAD`
request leaves it for the `GET`. That answer is private, as one for a signed-in caller is. A
notice is text, escaped where it renders, cut at 500 bytes. A page the browser restores from its
back-forward cache is the page as it was left, toaster and all; Corten does not ask the server for
it. It is for a page of the app: a route, a place on one, a checked `Local` or a
`#[derive(Href)]` enum of those (`OnSite`); a URL of another site does not compile.

A client navigation runs only the page inside `<main>`, not the layout around it, so a notice the
layout holds is not drawn there. When the page did not render the notice, its answer hands it to
the runtime, which dispatches `corten:notice` on the document (`detail.message`) once the page is
swapped in; corten-ui's `Toaster` shows it. A page that renders it itself (a status line in the
form it sent the user back to) is not sent it twice. When the navigation has to load the page in
full instead (its layout changed), the runtime keeps the notice for that load in session storage
and dispatches the event once the page's modules have connected.

A browser refuses a `Secure` cookie, and so a `__Host-` one, from a plain `http://` origin other
than loopback: a tool served on the LAN as `http://automation.lan:3001` or `http://192.168.1.5`.
Corten sees that in the write (an `Origin` of this host over `http://`, no `Sec-Fetch-Site`) and
sets the notice in a plain `corten-notice` cookie instead (`HttpOnly`, `SameSite=Lax`, a minute),
with nothing for the app to do. A page reads that one only from a request without
`Sec-Fetch-Site`, which current browsers send to every secure origin, so on HTTPS another site on
the domain cannot plant a notice in it there; a browser too old to send it (Safari before 16.4,
Firefox before 90) could be shown one, as text. An app served only over HTTPS says so with
`Policy::hsts()`, and then Corten never sets or reads the plain cookie. `localhost` and
`127.0.0.1` count as secure and keep `__Host-corten-notice`. A notice from a `GET` handler (no
`Origin` to tell by) always rides in `__Host-corten-notice`, so on a plain-http LAN address it
is lost: redirect with a notice from the write.

On a full load (the first page, or without JavaScript) the server draws the page, layout and all,
so the notice shows only where the page renders `req.notice()`: render it in the layout, once. A
page that renders it twice shows it twice, and one that does not render it drops it (a debug
build logs a warning).

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
to show "saving", the patch says where to read it again: `ReadAgain { after }.from(route)`, a
patch instruction like `Remove` and `Append`. It takes the route by value (a borrowed one works where the route outlives the patch, as in a
handler), so a component that is both part of the page and the patch builds it in its body and
returns it. After the delay the runtime fetches the route as a
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
request under an [`App::api`](app.md#json-only-routes) prefix, gets every failure in one shape,
the standard one: an RFC 9457 Problem Details document, sent as `application/problem+json`,
`{"title":"Not Found","status":404,"detail":"No item 7."}`. `title` is the status's reason. A
4xx's `detail` is the error's message or the rejection's text; a 5xx has no `detail`, only the
status (the message is logged, and shown under `corten dev`). Validation errors add the fields
as an extension member:
`{"title":"Unprocessable Entity","status":422,"detail":"email: ...","fields":{"email":"..."}}`,
and a message about the whole form (`Errors::add_form`) is `form` too.
