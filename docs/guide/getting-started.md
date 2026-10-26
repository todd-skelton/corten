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
| `corten dev` | Builds and runs the app, rebuilds and restarts on every change in the app's directory or a local package its build reads, like a workspace's shared crate (a failed build keeps the last good server running), and reloads open pages. `--port` or `PORT`. |
| `corten fmt` | Lays out the markup inside `html!`, which rustfmt leaves alone (`--check` only reports). |
| `corten check` | `cargo fmt --check`, `corten fmt --check`, `cargo clippy -D warnings`, `tsc` over the app's scripts (when TypeScript is installed; otherwise a note says they were not checked), `cargo test`. At a workspace root, each member that depends on `corten` is checked. |
| `corten build` | The release binary. |

The CLI is a convenience over cargo, not a build system: an app is an ordinary crate, and
`cargo run` works too.

## What `corten new` writes

| Path | What |
|---|---|
| `src/main.rs` | the routes (`#[corten::routes] mod url`), their handlers and `App` |
| `src/views.rs` | the layout (the whole document), the error page and the pages, in `html!` |
| `src/click_counter.js`, `.css` | the `ClickCounter` component's handlers and stylesheet, next to it |
| `static/` | the app's own CSS (element defaults, the layout), its favicon (`favicon.svg`, Corten's mark until you replace it), images; served at versioned URLs |
| `AGENTS.md` | the rules for an AI agent working on the app |
| `Dockerfile` | a release build in a slim image |

## A whole app in one file

Every route is a struct in the routes module, and its attribute names the handler for each method
(`get = home`). Every page is a handler that returns `req.page(document)`, the app's layout
with the page in it. A form with `data-patch` is submitted by the runtime, and its handler
answers with the fragment to replace; without JavaScript the same handler returns the whole page.

```rust,no_run
use corten::prelude::*;

#[corten::routes]
mod url {
    #[route("/", get = home)]
    pub struct Home;

    #[route("/greet", post = greet)]
    pub struct Greet;
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let app = App::new(url::router()).error_page(error_page, ());
    corten::serve(app).await
}

async fn home(req: Req) -> HtmlResult {
    let name = GreetForm::FIELDS.name;
    req.page(html! {
        <Layout req={&req} title="Home">
            <h1>"Welcome"</h1>
            <form method="post" action={url::Greet} data-patch>
                <input name={name} required>
                <button>"Greet me"</button>
            </form>
            <Greeting name="" />
        </Layout>
    })
}

#[derive(serde::Deserialize, Fields)]
struct GreetForm { name: String }

/// With JavaScript the runtime swaps in the patch; without it the form posts and the whole page
/// comes back. One handler serves both.
async fn greet(req: Req, Form(form): Form<GreetForm>) -> HtmlResult {
    match req.mode {
        Mode::Action => req.patch(Greeting { name: &form.name }),
        _ => req.page(html! { <Layout req={&req} title="Home"><Greeting name={&form.name} /></Layout> }),
    }
}

#[derive(Id)]
struct GreetingId;

#[component]
fn Greeting(name: &str) -> impl Patch {
    html! { <p id={GreetingId} aria-live="polite">if !name.is_empty() { "Hello, "{name}"!" }</p> }
}

/// The whole document. `<Corten />` is where Corten adds the runtime and the page's scripts and
/// stylesheets; a client navigation gets only the `<title>` and `<main>`'s children.
#[component]
fn Layout(req: &Req, title: &str, children: impl Render) -> impl Document {
    html! {
        <!DOCTYPE html>
        <html lang={req.lang()}>
            <head>
                <meta charset="utf-8">
                <link rel="stylesheet" href={req.asset("style.css")}>
                <title>{title}</title>
                <Corten />
            </head>
            <body><main>{children}</main></body>
        </html>
    }
}

/// The page for a failed request, in the layout: an unknown URL, a handler's error or panic.
/// `corten new` writes one that names every `Failure`. It takes extractors as a handler does
/// (`viewer: Option<Caller<Viewer>>` to draw it for who is asking).
async fn error_page(failure: Failure, req: Req) -> HtmlResult {
    let title = if matches!(failure, Failure::NotFound) { "Not found" } else { "Something went wrong" };
    req.page(html! { <Layout req={&req} title={title}><h1>{title}</h1></Layout> })
}
```

Handlers are async functions whose arguments are extractors (axum's, and Corten's `Req`,
`Form`, `Caller`, ...) and whose return type says what the route answers: `HtmlResult` for a
page, a patch or a redirect, `JsonResult<T>` for JSON. The routes module checks both when the
app compiles, and `App` adds everything a production app needs around them.

Next: [the app's defaults](app.md), then [views](views.md).
