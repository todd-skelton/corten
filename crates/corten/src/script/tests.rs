use crate::{
    html,
    prelude::*,
    script::{self, __private::scoped},
};

script::script!(mod shared = "shared.js");

#[script::event]
#[allow(dead_code)]
struct RowToggled {
    id: String,
    selected: bool,
}

#[component(js)]
fn LineItem<'a>(name: &'a str, quantity: u32) -> impl Render {
    let (save, dialog) = refs();
    html! {
        <li>
            <input value={quantity} on:input={js::enable(save)}>
            <button ref={save} disabled>"Save"</button>
            <button on:click={js::confirm(event, dialog, name)}>"Remove"</button>
            <dialog ref={dialog}>"Sure?"</dialog>
        </li>
    }
}

#[test]
fn handlers_are_calls_with_refs_keywords_and_values() {
    let html = LineItem { name: "a\"b", quantity: 2 }.to_html();
    assert_eq!(
        html,
        concat!(
            r#"<li data-scope="line-item" data-events="input click"><input value="2" data-on="input:line_item.enable(@save)">"#,
            r#"<button data-ref="save" disabled>Save</button>"#,
            r#"<button data-on="click:line_item.confirm($e,@dialog,&quot;a\&quot;b&quot;)">Remove</button>"#,
            r#"<dialog data-ref="dialog">Sure?</dialog></li>"#,
        )
    );
    let script = LineItem.script.expect("a script");
    assert_eq!((script.name, script.file), ("line_item", "line_item.js"));
    assert_eq!(script.handlers, &["enable", "confirm"]);
}

#[component(js)]
fn RowList<'a>(rows: &'a [&'a str]) -> impl Render {
    let row = refs();
    html! {
        <table>
            <tbody on:RowToggled={js::count(event, row)}>
                for r in rows.iter() {
                    <tr ref={row} on:click={js::toggle(target, r)}><td>{r}</td></tr>
                }
            </tbody>
        </table>
    }
}

#[test]
fn a_ref_in_a_loop_is_an_array_and_typed_events_are_kebab_case() {
    let html = RowList { rows: &["x", "y"] }.to_html();
    assert!(
        html.starts_with(r#"<table data-scope="row-list" data-events="row-toggled click"><tbody data-on="row-toggled:row_list.count($e,*row)">"#),
        "{html}"
    );
    assert!(html.contains(r#"<tr data-ref="row" data-on="click:row_list.toggle($t,&quot;x&quot;)">"#), "{html}");
}

/// A child that takes a handler and a ref from its parent.
#[component]
fn Confirm<'a>(label: &'a str, on_confirm: Handler<'a>) -> impl Render {
    html! { <button type="button" on:click={on_confirm}>{label}</button> }
}

#[component(js)]
fn Dialog() -> impl Render {
    let (form, body) = refs();
    html! {
        <form ref={form}>
            <div ref={body}></div>
            <Confirm label="OK" on:confirm={js::submit(form, scope)} />
            <Confirm label="No" on:confirm={None::<Handler>} />
        </form>
    }
}

#[test]
fn a_parent_passes_a_handler_and_its_refs_resolve_in_its_scope() {
    // The child puts the handler on its button and picks the event, so the parent's root has no
    // data-events for it (known: a script started on the root would not hear that click).
    let html = Dialog {}.to_html();
    assert_eq!(
        html,
        concat!(
            r#"<form data-scope="dialog" data-ref="form"><div data-ref="body"></div>"#,
            r#"<button type="button" data-on="click:dialog.submit(@form,$c)">OK</button>"#,
            r#"<button type="button">No</button></form>"#,
        )
    );
}

/// A child with refs of its own, inside a parent with refs: the parent's handler names its
/// refs with the parent's scope.
#[component(js)]
fn Field<'a>(on_change: Handler<'a>) -> impl Render {
    let input = refs();
    html! { <label><input ref={input} on:input={js::trim(target)} on:change={on_change}></label> }
}

#[component(js)]
fn Form() -> impl Render {
    let save = refs();
    html! {
        <div>
            <Field on:change={js::enable(save, scope)} />
            <button ref={save}>"Save"</button>
        </div>
    }
}

#[test]
fn a_handler_made_by_a_parent_keeps_the_parents_scope_inside_a_child() {
    let html = Form {}.to_html();
    assert_eq!(
        html,
        concat!(
            r#"<div data-scope="form"><label data-scope="field" data-events="input change"><input data-ref="input" "#,
            r#"data-on="input:field.trim($t) change:form.enable(@form/save,$c/form)"></label>"#,
            r#"<button data-ref="save">Save</button></div>"#,
        )
    );
}

#[test]
fn shared_modules_and_events_are_components() {
    assert_eq!(shared::COMPONENT.script.map(|s| s.name), Some("shared"));
    let event = RowToggled.event.expect("an event");
    assert_eq!((event.class, event.name, event.fields.len()), ("RowToggled", "row-toggled", 2));
    let html = scoped("x", || html! { <p on:click={shared::log("hi", 2, true)}>"x"</p> }.to_html());
    assert_eq!(html, r#"<p data-on="click:shared.log(&quot;hi&quot;,2,true)">x</p>"#);
}

// The module docs' script, shared here.
script::script!(mod js = "../stock_grid.js");

/// A list with one handler on its container (`closest`), a group of refs, keyed cells and a
/// typed event's handler.
#[component]
fn StockGrid<'a>(rows: &'a [(&'a str, u32)]) -> impl Render {
    let (row, count, qty) = refs();
    let sel = js::Selection { rows: row, count };
    html! {
        <table>
            <tbody on:click={js::toggle(row.closest(), sel)} on:RowToggled={js::toggled(event, sel)}>
                for (id, n) in rows.iter() {
                    <tr ref={row} id={*id}><td ref={qty} key={*id}>{*n}</td></tr>
                }
            </tbody>
            <caption><output ref={count}>"0"</output></caption>
            <tfoot on:connect={js::show(qty, "a")}></tfoot>
        </table>
    }
}

#[test]
fn closest_groups_keyed_refs_and_typed_event_handlers_are_written_compactly() {
    let html = StockGrid { rows: &[("a", 1), ("b", 2)] }.to_html();
    assert_eq!(
        html,
        concat!(
            r#"<table data-scope="stock-grid" data-events="click row-toggled"><tbody data-on="click:stock_grid.toggle(^row,{rows:*row,count:@count}) "#,
            r#"row-toggled:stock_grid.toggled($e,{rows:*row,count:@count})">"#,
            r#"<tr data-ref="row" id="a"><td data-ref="qty" data-key="a">1</td></tr>"#,
            r#"<tr data-ref="row" id="b"><td data-ref="qty" data-key="b">2</td></tr></tbody>"#,
            r#"<caption><output data-ref="count">0</output></caption>"#,
            r#"<tfoot data-on="connect:stock_grid.show(#qty,&quot;a&quot;)"></tfoot></table>"#,
        )
    );
}

#[test]
fn rendering_collects_the_modules_a_page_uses() {
    let (_, modules) = script::collect(|| (Form {}.to_html(), LineItem { name: "x", quantity: 1 }.to_html()));
    assert_eq!(modules, vec!["field", "form", "line_item"]);
}

/// The strings handlers write, as the runtime's parser expects them (`wire.json`, which
/// `scripts/wire.mjs` parses with the runtime's own parser).
#[cfg(feature = "json")]
#[test]
fn handlers_write_the_shared_wire_vectors() {
    #[derive(serde::Serialize)]
    struct Options {
        id: &'static str,
        tags: [&'static str; 2],
    }
    let nasty = "),{\"x\":1}\\ \u{2028} <&'> é 🦀 ,a:b";
    let pages = [
        LineItem { name: "a\"b", quantity: 2 }.to_html(),
        RowList { rows: &["x"] }.to_html(),
        Form {}.to_html(),
        StockGrid { rows: &[("a", 1)] }.to_html(),
        scoped("x", || {
            html! {
                <p on:click={shared::log("hi", 2, true)}></p>
                <p on:click={shared::log(nasty, -1.5, false)}></p>
                <p on:click={shared::log("", f64::NAN, true)}></p>
                <p on:connect={shared::watch(signal, Options { id: "a}", tags: ["x,y", "]"] })}></p>
            }
            .to_html()
        }),
    ];
    let mut written = Vec::new();
    for page in &pages {
        for part in page.split(" data-on=\"").skip(1) {
            let value = &part[..part.find('"').expect("a closing quote")];
            let text = value
                .replace("&quot;", "\"")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&#39;", "'")
                .replace("&amp;", "&");
            written.push(text);
        }
    }
    let vectors: serde_json::Value = serde_json::from_str(include_str!("wire.json")).expect("wire.json is JSON");
    let mut expected: Vec<String> = vectors["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .filter(|case| case.get("from").is_none())
        .map(|case| case["attr"].as_str().expect("attr").to_string())
        .collect();
    written.sort();
    expected.sort();
    assert_eq!(written, expected, "what handlers write and wire.json differ; update both sides together");
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "beyond what a JavaScript number holds exactly")]
fn an_integer_javascript_cannot_hold_exactly_stops_debug_builds() {
    let at_limit = scoped("x", || html! { <p on:click={shared::log("id", 9_007_199_254_740_991u64, true)}></p> }.to_html());
    assert!(at_limit.contains("9007199254740991"), "{at_limit}");
    scoped("x", || html! { <p on:click={shared::log("id", 9_007_199_254_740_993u64, true)}></p> }.to_html());
}
