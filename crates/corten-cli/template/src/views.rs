use corten::{axum::http::StatusCode, prelude::*};

use crate::{GreetForm, url};

/// The document around every page; it is yours to change. `<Corten />` is where Corten adds the
/// runtime and the page's scripts and stylesheets. A client navigation gets only the `<title>`
/// and `<main>`'s children, so the rest runs on full page loads only. For tags a page sets itself
/// (a description, a canonical link), add a prop and put it in `<head>`: `{head}`.
#[component]
pub fn Layout(req: &Req, title: &str, children: impl Render) -> impl Document {
    html! {
        <!DOCTYPE html>
        <html lang={req.lang()}>
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={req.asset("style.css")}>
                <title>{title}</title>
                <Corten />
            </head>
            <body>
                <header><a href={url::Home}>"{{name}}"</a></header>
                <main>{children}</main>
            </body>
        </html>
    }
}

/// The page for every way a request can fail, sent with `failure.status()`. Each case is named,
/// so when Corten adds one this stops compiling instead of showing the wrong text. Under
/// `corten dev`, `failure.detail()` is what went wrong; in production it is only logged. It takes
/// extractors as a handler does: add `viewer: Option<Caller<Viewer>>` to draw it for who is
/// asking, once the app has a caller.
pub async fn error_page(failure: Failure, req: Req) -> HtmlResult {
    let (title, text) = match &failure {
        Failure::NotFound => ("Not found", "There is nothing at this address."),
        Failure::CrossSite => ("Not allowed", "This request came from another site."),
        Failure::Refused { status } if *status == StatusCode::UNAUTHORIZED => {
            ("Sign in needed", "Sign in to see this page.")
        }
        Failure::Refused { .. } => ("Not allowed", "You do not have access to this page."),
        Failure::Unidentified { .. } => (
            "Not available",
            "We could not check who you are. Try again in a moment.",
        ),
        Failure::TimedOut | Failure::Offline => {
            ("Not available", "Check your connection and try again.")
        }
        Failure::WrongMethod
        | Failure::Rejected { .. }
        | Failure::Failed { .. }
        | Failure::Panicked { .. } => ("Something went wrong", "Please try again."),
    };
    req.page(html! {
        <Layout req={&req} title={title}>
            <h1>{title}</h1>
            <p>{text}</p>
            if let Some(detail) = failure.detail() {
                <pre>{detail}</pre>
            }
        </Layout>
    })
}

pub fn home(req: &Req, name: &str) -> impl Document {
    let f = GreetForm::FIELDS;
    html! {
        <Layout req={req} title="{{name}}">
            <h1>"Welcome to Corten"</h1>
            <p>
                "This page is rendered on the server. Edit " <code>"src/views.rs"</code>
                " and save: the page reloads."
            </p>
            <form method="post" action={url::Greet} data-patch>
                <label>"Your name " <input name={f.name} value={name} required></label>
                <button>"Greet me"</button>
            </form>
            <Greeting name={name} />
            <ClickCounter />
        </Layout>
    }
}

/// A button that counts its clicks in the browser. The markup is here; `click_counter.js`, next
/// to this file, has the handler, and `click_counter.css` the look. `on:click={js::count(total)}`
/// calls its `count` function with the element `ref={total}` is on (a compile error unless the
/// function takes an `<output>`), and `class={Button}` is the stylesheet's `.button`, scoped to
/// this component.
#[component(css, js)]
pub fn ClickCounter() -> impl Render {
    let total = refs();
    html! {
        <button type="button" class={Button} on:click={js::count(total)}>
            "Clicks: "<output ref={total}>"0"</output>
        </button>
    }
}

/// The greeting's id, so the page and the patch that replaces it name it the same way.
#[derive(Id)]
pub struct GreetingId;

/// Inside the page, and on its own as the form's patch (the runtime swaps it in by `id`).
#[component]
pub fn Greeting(name: &str) -> impl Patch {
    html! {
        <p id={GreetingId} aria-live="polite">
            if !name.is_empty() { "Hello, "{name}"!" }
        </p>
    }
}
