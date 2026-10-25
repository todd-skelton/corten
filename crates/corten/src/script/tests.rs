use crate::{
    html,
    prelude::*,
    script::{self, __private::scoped},
};

script::script!(mod shared = "shared.js");

crate::i18n::messages! { struct Msg { "en" => "src/i18n/en.json", "fr" => "src/i18n/fr.json" } }
use crate::i18n::Messages as _;

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

/// A count a script computes, shown with a message of the page's locale, alone and in a group
/// with a plain value.
#[component]
fn Count(t: Msg) -> impl Render {
    let out = refs();
    let counted = shared::Counted { entity: "Levels", label: t.js().item_listing_count() };
    html! {
        <p>
            <output ref={out} on:connect={shared::say(out, t.js().item_listing_count())}></output>
            <output ref={out} on:connect={shared::tally(out, counted)}></output>
        </p>
    }
}

#[test]
fn a_live_elements_status_event_takes_a_handler() {
    let html =
        scoped("x", || html! { <corten-live src="/live" on:corten-status={shared::log("x", 1, true)}></corten-live> }.to_html());
    assert_eq!(html, r#"<corten-live src="/live" data-on="corten-status:shared.log(&quot;x&quot;,1,true)"></corten-live>"#);
}

#[test]
fn handlers_are_calls_with_refs_keywords_and_values() {
    let html = LineItem { name: "a\"b", quantity: 2 }.to_html();
    assert_eq!(
        html,
        concat!(
            r#"<li data-scope="line-item"><input value="2" data-on="input:line_item.enable(@save)">"#,
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
    assert!(html.starts_with(r#"<table data-scope="row-list"><tbody data-on="row-toggled:row_list.count($e,*row)">"#), "{html}");
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
            r#"<div data-scope="form"><label data-scope="field"><input data-ref="input" "#,
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
            r#"<table data-scope="stock-grid"><tbody data-on="click:stock_grid.toggle(^row,{rows:*row,count:@count}) "#,
            r#"row-toggled:stock_grid.toggled($e,{rows:*row,count:@count})">"#,
            r#"<tr data-ref="row" id="a"><td data-ref="qty" data-key="a">1</td></tr>"#,
            r#"<tr data-ref="row" id="b"><td data-ref="qty" data-key="b">2</td></tr></tbody>"#,
            r#"<caption><output data-ref="count">0</output></caption>"#,
            r#"<tfoot data-on="connect:stock_grid.show(#qty,&quot;a&quot;)"></tfoot></table>"#,
        )
    );
}

/// A view, for the page test below.
#[component(view)]
fn Badge(n: u32) -> impl Render {
    html! { <b>{n}</b> }
}

/// A view with a 64-bit number property.
#[component(view)]
fn Order(id: u64) -> impl Render {
    html! { <b>{id}</b> }
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "view property id = 18446744073709551615 is beyond what a JavaScript number holds exactly")]
fn a_view_number_javascript_cannot_hold_exactly_stops_debug_builds() {
    assert_eq!(Order { id: (1 << 53) - 1 }.to_html(), "<b>9007199254740991</b>");
    Order { id: u64::MAX }.to_html();
}

#[tokio::test]
async fn a_page_loads_its_handler_modules_as_module_scripts_with_the_views_they_may_import() {
    use axum::{body::Body, http::Request, routing::get, Router};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let get_page = |app: Router| async move {
        let res = app.oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    };
    let handlers =
        Router::new().route(
            "/",
            get(|req: crate::Req| async move {
                crate::tests::TestPage::test_page(&req, "Cart", LineItem { name: "Lamp", quantity: 2 })
            }),
        );
    let body = get_page(crate::App::new(handlers).components(&[LineItem, Badge]).router()).await;
    let views = body.split_once(r#""corten/views":""#).expect("the import map maps corten/views").1.split_once('"').unwrap().0;
    assert!(views.ends_with("/views.js"), "{body}");
    assert!(body.contains(&format!(r#"<link rel="modulepreload" href="{views}">"#)), "{body}");
    // The handler module is a module script of the page, and hands its exports to the runtime as it runs.
    let module = views.replace("/views.js", "/scripts/line_item.js");
    assert!(body.contains(&format!(r#"<script type="module" src="{module}"></script>"#)), "{body}");
    let app = crate::App::new(Router::new()).components(&[LineItem, Badge]).router();
    let res = app.oneshot(Request::get(&module).body(Body::empty()).unwrap()).await.unwrap();
    let source = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(
        source.ends_with(concat!(
            "import \"corten\";\n",
            "import * as __corten_self from \"./line_item.js\";\n",
            "document.dispatchEvent(new CustomEvent(\"corten:module\", { detail: { name: \"line_item\", url: import.meta.url, exports: __corten_self } }));\n"
        )),
        "{source}"
    );
    // A page whose handlers need no module has nothing to import the views.
    let plain = Router::new()
        .route("/", get(|req: crate::Req| async move { crate::tests::TestPage::test_page(&req, "Badge", Badge { n: 1 }) }));
    let body = get_page(crate::App::new(plain).components(&[LineItem, Badge]).router()).await;
    assert!(!body.contains("modulepreload") && !body.contains(r#"src="/assets/"#), "{body}");
}

#[test]
fn view_ids_are_fnv1a_as_the_macros_compute_them() {
    assert_eq!((script::view_id(""), script::view_id("a")), (0xcbf2_9ce4_8422_2325, 0xaf63_dc4c_8601_ec8c));
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
    #[derive(crate::json::ToJson)]
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
        Count { t: Msg::for_locale("en") }.to_html(),
        Count { t: Msg::for_locale("fr") }.to_html(),
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
