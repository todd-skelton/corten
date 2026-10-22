use corten::prelude::*;

use crate::{Greet, GreetForm, Home};

/// The document around every page. Client navigation swaps `<main>`.
pub fn shell(doc: Document) -> impl Render {
    html! {
        <!DOCTYPE html>
        <html lang={doc.lang()}>
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={doc.asset("style.css")}>
                {doc.head}
            </head>
            <body>
                <header><a href={Home}>"{{name}}"</a></header>
                <main>{doc.main}</main>
            </body>
        </html>
    }
}

pub fn home(name: &str) -> impl Render {
    let f = GreetForm::FIELDS;
    html! {
        <h1>"Welcome to Corten"</h1>
        <p>"This page is rendered on the server. Edit " <code>"src/views.rs"</code> " and save: the page reloads."</p>
        <form method="post" action={Greet} data-patch>
            <label>"Your name " <input name={f.name} value={name} required></label>
            <button>"Greet me"</button>
        </form>
        <Greeting name={name} />
        <ClickCounter />
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

/// Inside the page, and on its own as the form's patch (the runtime swaps it in by `id`).
#[component]
pub fn Greeting(name: &str) -> impl Render {
    html! {
        <p id="greeting" aria-live="polite">
            if !name.is_empty() { "Hello, "{name}"!" }
        </p>
    }
}
