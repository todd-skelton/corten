# Forms

## A form that patches part of the page

```rust
#[derive(serde::Deserialize, Fields)]
struct GreetForm { name: String }

fn page(name: &str) -> impl Render + '_ {
    let f = GreetForm::FIELDS;
    html! {
        <form method="post" action={Greet} data-patch>
            <input name={f.name} required>
            <button>"Greet me"</button>
        </form>
        <Greeting name={name} />
    }
}

#[component]
fn Greeting(name: &str) -> impl Render {
    html! { <p id="greeting" aria-live="polite">if !name.is_empty() { "Hello, "{name}"!" }</p> }
}

async fn greet(req: Req, Form(form): Form<GreetForm>) -> corten::Result {
    match req.mode {
        Mode::Action => req.patch(Greeting { name: &form.name }),  // the runtime swaps #greeting
        _ => req.page("Home", page(&form.name)),                    // no JavaScript: the whole page
    }
}
```

A patch may hold several top-level elements; each replaces the element with the same `id`.
After a write that should change the URL, use `req.redirect(&Route)` instead.

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

The form component shows them next to each field:

```rust
<input type="email" name={f.email} value={form.email} required {..form.errors.aria(f.email)}>
{form.errors.message(f.email)}
```

The handler answers a failed check with `req.invalid`. It replies 422 with the form as a patch for
the runtime, `{"errors": ...}` for JSON clients, or the whole page for a browser without
JavaScript. The page closure runs only in that last case, so put the expensive loads inside it:

```rust
let price = match check(t, &form) {
    Ok(price) => price,
    Err(errors) => {
        let view = AlertFormView { errors, ..AlertFormView::from(&form) };
        let page = || async { req.page("Item", item_page(&load(&id).await?, view.clone())) };
        return req.invalid(&view.errors, AlertForm { t, form: view.clone() }, page).await;
    }
};
```

Keep the HTML attributes (`required`, `type="email"`, `inputmode`) too: the browser checks them
first, and the server check is the one that counts.
