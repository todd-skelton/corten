use super::format_source;

/// Formats `input` at `width`, checks the result is stable, and returns it.
fn fmt(input: &str, width: usize) -> String {
    let (once, warnings) = format_source(input, width);
    assert!(warnings.is_empty(), "{warnings:?}");
    let (twice, _) = format_source(&once, width);
    assert_eq!(once, twice, "formatting is stable");
    once
}

#[test]
fn short_markup_stays_on_one_line_and_long_markup_breaks_by_element() {
    let input = "fn a() -> impl Render {\n    html! {   <p   class={X}>{text}</p>  }\n}\n";
    assert_eq!(fmt(input, 100), "fn a() -> impl Render {\n    html! { <p class={X}>{text}</p> }\n}\n");

    let input = r#"fn shell(doc: Document) -> impl Render {
    html! { <!DOCTYPE html> <html lang={doc.lang()}><head><meta charset="utf-8"><title>"T"</title></head>
    <body><header class="site"><a href={routes::Home}>"Card Market"</a></header><main>{doc.main}</main></body></html> }
}
"#;
    let expected = r#"fn shell(doc: Document) -> impl Render {
    html! {
        <!DOCTYPE html>
        <html lang={doc.lang()}>
            <head>
                <meta charset="utf-8">
                <title>"T"</title>
            </head>
            <body>
                <header class="site"><a href={routes::Home}>"Card Market"</a></header>
                <main>{doc.main}</main>
            </body>
        </html>
    }
}
"#;
    assert_eq!(fmt(input, 100), expected);
}

#[test]
fn attributes_that_do_not_fit_go_one_per_line() {
    let input = r#"    html! {
        <input name={f.price} value={shown} inputmode="decimal" aria-label="Price" on:input={js::edited(target, save)} {..errors.aria(f.price)}>
    }
"#;
    let expected = r#"    html! {
        <input
            name={f.price}
            value={shown}
            inputmode="decimal"
            aria-label="Price"
            on:input={js::edited(target, save)}
            {..errors.aria(f.price)}
        >
    }
"#;
    assert_eq!(fmt(input, 80), expected);
}

#[test]
fn text_and_inline_elements_fill_lines_and_blocks_keep_their_shape() {
    let input = r#"html! {
    <ul>
        // One per offer.
        for offer in offers { <li>{offer.seller}" sells at "<strong>{offer.price}</strong></li> }

        if offers.is_empty() { <li>"None yet"</li> } else if all { "All" } else { <li>"More"</li> <li>"Again"</li> }
    </ul>
    match state { State::Open => <p>"Open"</p>, State::Closed(why) => { <p>"Closed: "{why}</p> } }
    let shown = saved.as_deref().unwrap_or(typed);
}
"#;
    let expected = r#"html! {
    <ul>
        // One per offer.
        for offer in offers { <li>{offer.seller}" sells at "<strong>{offer.price}</strong></li> }

        if offers.is_empty() {
            <li>"None yet"</li>
        } else if all {
            "All"
        } else {
            <li>"More"</li>
            <li>"Again"</li>
        }
    </ul>
    match state {
        State::Open => <p>"Open"</p>,
        State::Closed(why) => { <p>"Closed: "{why}</p> }
    }
    let shown = saved.as_deref().unwrap_or(typed);
}
"#;
    assert_eq!(fmt(input, 100), expected);
}

#[test]
fn rust_inside_braces_is_kept_and_moves_with_its_line() {
    let input = r#"fn a() {
    let v = html! {
            <div>
                {items.iter().map(|i| {
                    i.name
                }).count()}
                <p on:click={js::go(a,   b)}>"x"</p>
            </div>
    };
}
"#;
    let expected = r#"fn a() {
    let v = html! {
        <div>
            {items.iter().map(|i| {
                i.name
            }).count()}
            <p on:click={js::go(a,   b)}>"x"</p>
        </div>
    };
}
"#;
    assert_eq!(fmt(input, 100), expected);
}

#[test]
fn what_it_cannot_lay_out_is_left_as_written() {
    let raw = "fn a() {\n    html! { <pre>{r\"one\n   two\"}</pre> <p>\"x\"</p> }\n}\n";
    let (out, warnings) = format_source(raw, 20);
    assert!(out.contains("one\n   two"), "a multi-line string keeps its lines: {out}");
    assert!(warnings.is_empty(), "{warnings:?}");

    let broken = "fn a() { html! { <p>\"x\"</q> } }\n";
    let (out, warnings) = format_source(broken, 100);
    assert_eq!(out, broken);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

#[test]
fn components_fragments_and_closing_text_after_the_macro() {
    let input = "fn a() { req.patch(html! { <><StatusLine text={&status} />  <Card title=\"x\" compact>\"Body\"</Card></> }) }\n";
    let expected = r#"fn a() { req.patch(html! {
    <>
        <StatusLine text={&status} />
        <Card title="x" compact>"Body"</Card>
    </>
}) }
"#;
    assert_eq!(fmt(input, 60), expected);
}

#[test]
fn a_body_written_expanded_stays_expanded() {
    let input = r#"pub fn Greeting(name: &str) -> impl Render {
    html! {
        <p id={GreetingId} aria-live="polite">
            if !name.is_empty() { "Hello, "{name}"!" }
        </p>
    }
}
"#;
    assert_eq!(fmt(input, 100), input);

    let input = "fn a() {\n    html! {\n        for x in xs {\n            <li>{x}</li>\n        }\n    }\n}\n";
    assert_eq!(fmt(input, 100), input);

    let input = "fn a() {\n    html! {\n        <p>\n            \"x\"\n        </p>\n    }\n}\n";
    assert_eq!(fmt(input, 100), input, "the macro body itself was written expanded");
}

#[test]
fn spaces_between_inline_siblings_are_the_authors() {
    let input = r#"html! {
    <p>"This page is rendered on the server. Edit " <code>"src/views.rs"</code> " and save: the page reloads."</p>
    <label>"Your name " <input name={f.name} required></label>
    <span>"Signed in as "{viewer.name}</span>
}
"#;
    let expected = r#"html! {
    <p>
        "This page is rendered on the server. Edit " <code>"src/views.rs"</code>
        " and save: the page reloads."
    </p>
    <label>"Your name " <input name={f.name} required></label>
    <span>"Signed in as "{viewer.name}</span>
}
"#;
    assert_eq!(fmt(input, 90), expected);
}

#[test]
fn a_match_arm_holding_an_element_reads_like_rustfmt() {
    let input = r#"html! {
    match viewer {
        Some(viewer) => <span>"Signed in as "{viewer.name}</span>,
        None => <form method="post" action={url::SignIn} data-patch><button>"Sign in as Ada"</button></form>,
    }
}
"#;
    let expected = r#"html! {
    match viewer {
        Some(viewer) => <span>"Signed in as "{viewer.name}</span>,
        None => <form method="post" action={url::SignIn} data-patch>
            <button>"Sign in as Ada"</button>
        </form>,
    }
}
"#;
    assert_eq!(fmt(input, 90), expected);
    assert_eq!(fmt(input, 130), input, "it fits at the repo's width");
}

#[test]
fn it_never_joins_lines_the_author_split() {
    let input = r#"html! {
    <BulkBar
        count={count}
        on:clear={js::clear(scope, sel)}
    />
    <form>
        <button ref={save}>"Save"</button>
        {errors.message(f.price)}
    </form>
}
"#;
    assert_eq!(fmt(input, 100), input);
}

#[test]
fn the_new_app_template_is_formatted_at_the_default_width() {
    // A new app has no rustfmt.toml, so `corten check` formats it at 100 columns.
    let source = include_str!("../../template/src/views.rs");
    let (out, warnings) = format_source(source, 100);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(out == source, "run corten fmt on a new app and copy its src/views.rs back:\n{out}");
}

#[test]
fn a_pattern_with_braces_is_part_of_the_head() {
    let input = "    html! {\n        if let Shape::Circle { r } = shape { <i>{r}</i> } else if let Shape::Square { side: 1..=5 } = shape { <b>\"small\"</b> } else if a == b { <u>\"dot\"</u> }\n        for Point { x, y } in &points { <p>{x}</p> }\n    }\n";
    let expected = "    html! {\n        if let Shape::Circle { r } = shape {\n            <i>{r}</i>\n        } else if let Shape::Square { side: 1..=5 } = shape {\n            <b>\"small\"</b>\n        } else if a == b {\n            <u>\"dot\"</u>\n        }\n        for Point { x, y } in &points { <p>{x}</p> }\n    }\n";
    assert_eq!(fmt(input, 60), expected);
}

/// A file with CRLF line endings is laid out and keeps them.
#[test]
fn a_crlf_file_keeps_its_line_endings() {
    let dir = std::env::temp_dir().join(format!("corten-fmt-crlf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("view.rs");
    std::fs::write(&file, "fn a() -> impl Render {\r\n    html! {   <p   class={X}>{text}</p>  }\r\n}\r\n").unwrap();
    let mut warned = std::collections::BTreeSet::new();
    let wrote = super::markup(std::slice::from_ref(&file), false, &mut warned).unwrap();
    assert_eq!(wrote.len(), 1);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "fn a() -> impl Render {\r\n    html! { <p class={X}>{text}</p> }\r\n}\r\n"
    );
    assert!(super::markup(std::slice::from_ref(&file), true, &mut warned).unwrap().is_empty(), "laid out already");
    std::fs::remove_file(&file).unwrap();
    let _ = std::fs::remove_dir(&dir);
}
