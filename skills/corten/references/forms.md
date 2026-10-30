# Forms

## A form that patches part of the page

```rust
#[derive(serde::Deserialize, Fields)]
struct GreetForm { name: String }

fn home_page(req: &Req, name: &str) -> impl Document {
    let f = GreetForm::FIELDS;
    html! {
        <Layout req={req} title="Home">
            <form method="post" action={url::Greet} data-patch>
                <input name={f.name} required>
                <button>"Greet me"</button>
            </form>
            <Greeting name={name} />
        </Layout>
    }
}

#[derive(Id)]
struct GreetingId;

#[component]
fn Greeting(name: &str) -> impl Patch {
    html! { <p id={GreetingId} aria-live="polite">if !name.is_empty() { "Hello, "{name}"!" }</p> }
}

async fn greet(req: Req, Form(form): Form<GreetForm>) -> HtmlResult {
    // With JavaScript the runtime swaps #greeting; without, the whole page. The page closure
    // runs only then, so a patch never loads the page's data.
    req.patch_or(Greeting { name: &form.name }, || async { req.page(home_page(&req, &form.name)) }).await
}
```

A patch may hold several top-level elements; each replaces the element with the same `id`, so
each needs one, a `#[derive(Id)]` type: `req.patch` and `req.invalid` take a `Patch`, and a view
whose top level has text, an element without an id or an id written as text does not compile. A
component a patch sends is rooted at the element with the id and declared `-> impl Patch`. Fields
the user edited (after the form was sent), the files chosen in a file input (until a successful
answer to the form that sent them) and the focus carry over to the new markup, so a patch can
include the form the user is typing in, and a refusal can mark a file input invalid.
After a write that should change the URL, use `req.redirect(&Route)` instead. To come back to
the element the user acted on, redirect to `route.at(RowId(&id))` (the route with `#row-7`,
from the element's `#[derive(Id)]`); a link within the page, such as an error summary's link to
its field, is `href={FieldId.fragment()}`. Never format a `#fragment` by hand.

## Files

A form with `<input type="file">` posts `multipart/form-data` (`enctype="multipart/form-data"`).
Name the input with a `Files` field of the form struct and take `Upload<T>`, never `Form` (it
answers 415) or a raw body with a parser of your own:

```rust
#[derive(serde::Deserialize, Fields)]
struct NewListing { title: String, photos: Files }   // <input type="file" name={f.photos} multiple>

async fn create(req: Req, mut upload: Upload<NewListing>) -> HtmlResult {
    let f = NewListing::FIELDS;
    upload.limit(60 << 20);                     // the whole body (2 MiB unless set), before reading
    let mut errors = Errors::new();
    while let Some(mut file) = upload.next_file().await? {
        if !file.is(f.photos) { continue }      // skipped, not stored
        match file.bytes_up_to(10 << 20).await? {   // or file.chunk() to stream it on
            Some(bytes) => { /* keep it */ }
            None => errors.add(f.photos, "Each photo can be up to 10 MB."),
        }
    }
    let input = upload.fields().await?;         // the text fields, by Form's rules
    ..
}
```

A body over `upload.limit` makes every read return `UploadError::TooLarge` (`?`: a 413 and the
error page). To answer it in the form, put the reads in one `async` block returning
`Result<_, UploadError>` and match `Err(UploadError::TooLarge { .. })` once; never compare
statuses or check `Content-Length` by hand.

Types, counts and per-file sizes are the handler's checks, reported with `errors.add(f.photos, ..)`
so the form is drawn again with the message beside the input.

To serve an upload back, read its type from its first bytes (never `file.content_type()`, which is
the client's word) and return `corten::Media::new("image/png", bytes)`. At a URL that names the
bytes (a hash), return `Public::immutable(answer)` so every cache keeps it for a year; its handler
cannot read who is asking. A download names its file with `.attachment("cards.csv")` (or
`.inline(..)` to show it), which escapes the name; never set `Content-Disposition` yourself.
Link to it with `download` (`data-reload` for an `inline` file), so the runtime does not fetch
it on hover and again on click. Never build the response by hand.

## Checkboxes and lists

`Form` and a route's `Params` read a key sent several times into a `Vec`. A `bool` or `Vec` field needs
`#[serde(default)]` (`#[derive(Fields)]` does not compile without it), because the browser sends
nothing for an unchecked box:

```rust
#[derive(serde::Deserialize, Fields)]
struct ArchiveInput {
    #[serde(default)]
    ids: Vec<String>,   // one <input type="checkbox" name={f.ids} value={id}> per row
    #[serde(default)]
    notify: bool,       // a checkbox: "on" when checked
}
```

Never join values into one field with commas in a script; give each box the name. A box outside
the form joins it with `form={ArchiveFormId}` (a `#[derive(Id)]` type). A required list is a
validation message (`errors.add(f.ids, ..)` when empty), not a missing field; wrap the boxes in
`corten_ui::CheckboxGroup` (`missing=".."`) so the browser says so before sending. A blank field is
`None` for an `Option`. A typed route's list field writes `?condition=nm&condition=lp`, which
its `Params` reads back.

## Groups and rows

A group several forms share (an address) is its own `Fields` struct, put in a form with
`#[fields(group)] ship_to: Address` (posted `ship_to.street`, named `f.ship_to.street`); never
`#[serde(flatten)]`, which does not compile. Rows of the same fields (an editable table, line
items) are `#[fields(rows)] #[serde(default)] lines: BTreeMap<Key, Line>`, and a row's field is
`f.lines.at(&key).qty` (posted `lines[key].qty`), in markup and in `errors.add`. Never post
rows as parallel `Vec` columns: the key joins a row's values and gives each row's field its own
error. A key type is a `RowKey`: text, an integer, or a `#[derive(UrlValue)]` enum. One plain
value per key (a count per SKU) is `#[fields(map)] #[serde(default)] counts: BTreeMap<Sku, u32>`,
a field per key: `f.counts.at(&sku)`, posted `counts[sku]`.

## Validation

Validate on the server into `Errors`, keyed by `FieldName`, never by string:

```rust
pub fn check(t: Msg, input: &AlertInput) -> Result<f64, Errors> {
    let mut errors = Errors::new();
    if !input.email.contains('@') {
        errors.add(AlertInput::FIELDS.email, t.alert_invalid_email());
    }
    let price: f64 = input.target_price.trim().parse().unwrap_or(f64::NAN);
    if !price.is_finite() || price <= 0.0 {
        errors.add(AlertInput::FIELDS.target_price, t.alert_invalid_price());
    }
    errors.into_result(price)
}
```

The form component shows them next to each field, read within the id of the element a refusal
replaces (so two forms with a field of the same name have different message ids):

```rust
let e = form.errors.within(AlertFormId);
// in html!:
<input type="email" name={f.email} value={form.email} required {..e.aria(f.email)}>
{e.message(f.email)}
```

A field with a hint of its own gives the hint's id to the spread,
`{..e.aria(f.email).hint(&EmailHintId)}`, never a second `aria-describedby` on the tag (the
browser keeps only the first, and `html!` refuses it).

A refusal that belongs to no field (a cross-field rule, or the backend saying no) is
`errors.add_form(text)`, shown once at the top of the form with `{form.errors.form_message()}`.
Never attach it to an arbitrary field, and never pass it beside `Errors` in a separate prop: data
clients and agents only see what is in `Errors`.

A write that must apply once (an order, a payment) takes a `submit: SubmitKey` field, drawn with
`{SubmitKey::field(f.submit)}`; the app stores the key with the write and answers a repeat with the
first outcome. The runtime keeps the key until the submit has an answer below 500, then renews it.

The handler answers a failed check with `req.invalid`. It replies 422 with the form as a patch for
the runtime, a Problem Details document (`application/problem+json`, `{"title","status","detail","fields","form"}`) for JSON clients, or the whole page for a browser without
JavaScript. The page closure runs only in that last case, so put the expensive loads inside it:

```rust
let price = match check(t, &form) {
    Ok(price) => price,
    Err(errors) => {
        let view = AlertFormView { errors, ..AlertFormView::from(&form) };
        let page = || async { req.page(item_page(&req, &load(&id).await?, view.clone())) };
        return req.invalid(&view.errors, AlertForm { t, form: view.clone() }, page).await;
    }
};
```

Keep the HTML attributes (`required`, `type="email"`, `inputmode`) too: the browser checks them
first, and the server check is the one that counts. A field required only for some values of
another goes in `corten_ui::RequiredWhen` (`control={f.kind} values={&[Kind::Partial]}`), never a
script of the app's, and the handler checks the same rule. A field with a fixed set of values
(a select, radios, a `corten_ui::ComboBox`) is a `#[derive(UrlValue, Deserialize)]` enum in the form
struct, and the markup writes its variants (`value={Kind::Partial}`, `Choice { value: Kind::Partial, .. }`),
not their words as strings.
