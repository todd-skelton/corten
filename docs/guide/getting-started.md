# Getting started

[Back to the guides](../../README.md#documentation)

## Install and create an app

```sh
cargo install corten-cli    # or cargo binstall corten-cli, for the prebuilt binary
corten new my-app
cd my-app
corten dev        # http://localhost:3000, rebuilds on save, open pages reload
```

`corten new` writes a working app: a page, a form that patches part of the page, a component
with a script, a
test and a Dockerfile.

| Command | What it does |
|---|---|
| `corten new <dir>` | Creates an app that depends on the `corten` release matching the command's version. `--name` sets the crate name, `--corten-path` uses a local checkout instead. |
| `corten dev` | Builds and runs the app, rebuilds and restarts on every change (a failed build keeps the last good server running), and reloads open pages. `--port` or `PORT`. |
| `corten fmt` | Lays out the markup inside `html!`, which rustfmt leaves alone (`--check` only reports). |
| `corten check` | `cargo fmt --check`, `corten fmt --check`, `cargo clippy -D warnings`, `cargo test`. |
| `corten build` | The release binary. |

The CLI is a convenience over cargo, not a build system: an app is an ordinary crate, and
`cargo run` works too.

## What `corten new` writes

| Path | What |
|---|---|
| `src/main.rs` | the routes (`#[derive(Route)]` types), the handlers and `App` |
| `src/views.rs` | the shell and pages, in `html!` |
| `src/click_counter.js`, `.css` | the `ClickCounter` component's handlers and stylesheet, next to it |
| `static/` | the app's own CSS (element defaults, the shell), images; served at versioned URLs |
| `AGENTS.md` | the rules for an AI agent working on the app |
| `Dockerfile` | a release build in a slim image |

## A whole app in one file

Every page is a handler that returns `req.page(title, view)`. A form with `data-patch` is
submitted by the runtime, and its handler answers with the fragment to replace; without
JavaScript the same handler returns the whole page.

```rust,no_run
use corten::prelude::*;

#[derive(Route)]
#[route("/")]
struct Home;

#[derive(Route)]
#[route("/greet")]
struct Greet;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let routes = Router::new().route(Home::PATH, get(home)).route(Greet::PATH, post(greet));
    corten::serve(App::new(routes).shell(shell)).await
}

async fn home(req: Req) -> corten::Result {
    req.page("Home", html! {
        <h1>"Welcome"</h1>
        <form method="post" action={Greet} data-patch>
            <input name={GreetForm::FIELDS.name} required>
            <button>"Greet me"</button>
        </form>
        <Greeting name="" />
    })
}

#[derive(serde::Deserialize, Fields)]
struct GreetForm { name: String }

/// With JavaScript the runtime swaps in the patch; without it the form posts and the whole page
/// comes back. One handler serves both.
async fn greet(req: Req, Form(form): Form<GreetForm>) -> corten::Result {
    match req.mode {
        Mode::Action => req.patch(Greeting { name: &form.name }),
        _ => req.page("Home", html! { <Greeting name={&form.name} /> }),
    }
}

#[component]
fn Greeting(name: &str) -> impl Render {
    html! { <p id="greeting" aria-live="polite">if !name.is_empty() { "Hello, "{name}"!" }</p> }
}

fn shell(doc: Document) -> impl Render {
    html! {
        <!DOCTYPE html>
        <html lang={doc.lang()}>
            <head><meta charset="utf-8"><link rel="stylesheet" href={doc.asset("style.css")}>{doc.head}</head>
            <body><main>{doc.main}</main></body>
        </html>
    }
}
```

Handlers are plain axum handlers. `App` adds everything a production app needs around them.

Next: [the app's defaults](app.md), then [views](views.md).
