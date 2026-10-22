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

#[test]
fn rendering_collects_the_modules_a_page_uses() {
    let (_, modules) = script::collect(|| (Form {}.to_html(), LineItem { name: "x", quantity: 1 }.to_html()));
    assert_eq!(modules, vec!["field", "form", "line_item"]);
}
