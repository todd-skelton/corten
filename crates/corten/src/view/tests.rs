use std::fmt;

use crate::view::{component, html, raw, Attrs, Render};

struct Price(u32);

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.0)
    }
}

#[test]
fn text_and_expressions_are_escaped_and_static_markup_is_kept() {
    let name = "Pika <Promo> & \"Friends\"";
    let view = html! {
        <h1 class="title">"Tom & Jerry's"</h1>
        <p>{name}" · "{42}{-7i64}{'<'}</p>
    };
    assert_eq!(
        view.to_html(),
        "<h1 class=\"title\">Tom &amp; Jerry&#39;s</h1><p>Pika &lt;Promo&gt; &amp; &quot;Friends&quot; · 42-7&lt;</p>"
    );
}

#[test]
fn display_is_the_fallback_and_raw_is_not_escaped() {
    let view = html! { <span>{Price(3)}</span>{raw("<b>ok</b>")} };
    assert_eq!(view.to_html(), "<span>&lt;3&gt;</span><b>ok</b>");
}

#[test]
fn attributes_take_text_numbers_bools_options_and_display() {
    let id = "a\"b";
    let none: Option<&str> = None;
    let view = html! {
        <input
            type="text"
            name={id}
            value={12}
            required
            disabled={false}
            checked={true}
            title={none}
            placeholder={Some("x")}
            data-p={Price(1)}
        >
    };
    assert_eq!(
        view.to_html(),
        r#"<input type="text" name="a&quot;b" value="12" required checked placeholder="x" data-p="&lt;1&gt;">"#
    );
}

#[test]
fn control_flow_is_rust() {
    let items = ["a", "b", "c"];
    let selected = Some(1);
    let view = html! {
        <ul>
            for (i, item) in items.iter().enumerate() {
                let current = selected == Some(i);
                <li aria-current={current.then_some("true")}>{item}</li>
            }
        </ul>
        if items.is_empty() { <p>"none"</p> } else if items.len() > 5 { <p>"many"</p> } else { <p>"some"</p> }
        match selected {
            Some(0) => "first",
            Some(n) if n > 0 => { <b>{n}</b> }
            _ => {}
        }
    };
    assert_eq!(view.to_html(), r#"<ul><li>a</li><li aria-current="true">b</li><li>c</li></ul><p>some</p><b>1</b>"#);
}

#[test]
fn void_elements_doctype_fragments_and_raw_text() {
    let view = html! {
        <!DOCTYPE html>
        <>
            <br>
            <img src="/a.png" alt="" />
            <div />
            <style>"a > b { color: red }"</style>
        </>
    };
    assert_eq!(view.to_html(), r#"<!DOCTYPE html><br><img src="/a.png" alt=""><div></div><style>a > b { color: red }</style>"#);
}

#[component]
fn Badge(label: &str, count: usize, hot: bool) -> impl Render {
    let class = if hot { "badge hot" } else { "badge" };
    html! { <span class={class}>{label}" "{count}</span> }
}

#[component]
fn Panel<'a, C: Render>(title: &'a str, owned: String, children: C) -> impl Render {
    html! {
        <section data-owned={owned}>
            <h2>{title}</h2>
            {children}
        </section>
    }
}

mod ui {
    use super::*;

    #[component]
    pub fn Pill(r#type: &'static str) -> impl Render {
        html! { <i>{r#type}</i> }
    }
}

#[test]
fn components_take_props_children_and_paths() {
    let label = String::from("New");
    let view = html! {
        <Panel title="Deals" owned={label.clone()}>
            <Badge label={&label} count={3} hot />
            <ui::Pill type="x" />
        </Panel>
    };
    assert_eq!(
        view.to_html(),
        r#"<section data-owned="New"><h2>Deals</h2><span class="badge hot">New 3</span><i>x</i></section>"#
    );
}

/// A plain function is a component too, and views compose without intermediate strings.
fn rows<'a>(names: &'a [&'a str]) -> impl Render + 'a {
    html! { for name in names { <tr><td>{name}</td></tr> } }
}

#[test]
fn views_nest_and_render_lazily() {
    let names = ["x", "y"];
    let table = html! { <table>{rows(&names)}</table> };
    let page = html! { <main>{table}{Some(html! { <hr> })}{None::<&str>}</main> };
    assert_eq!(page.to_html(), "<main><table><tr><td>x</td></tr><tr><td>y</td></tr></table><hr></main>");
}

struct Aria(bool);

impl Attrs for Aria {
    fn render_attrs(&self, out: &mut String) {
        if self.0 {
            out.push_str(r#" aria-invalid="true""#);
        }
    }
}

#[test]
fn views_own_and_move_what_they_capture() {
    let viewer: Option<String> = Some("Ada".into());
    let names = vec![String::from("a"), String::from("b")];
    let view = html! {
        match viewer {
            Some(name) => <b>{name}</b>,
            None => "guest"
        }
        for name in names { <i>{name}</i> }
    };
    assert_eq!(view.to_html(), "<b>Ada</b><i>a</i><i>b</i>");
}

#[test]
#[should_panic(expected = "renders once")]
fn a_view_renders_once() {
    let view = html! { <p>"x"</p> };
    let _ = view.to_html();
    let _ = view.to_html();
}

#[test]
fn tuples_print_their_parts() {
    let id = "a&b";
    let view = html! { <a href={("/items/", id, "?x=", 1)}>{("#", 2)}</a> };
    assert_eq!(view.to_html(), r#"<a href="/items/a&amp;b?x=1">#2</a>"#);
}

#[test]
fn attributes_spread() {
    let pairs = [("data-id", "7"), ("bad name", "x")];
    let view = html! { <input name="a" {..Aria(true)} {..pairs}> };
    assert_eq!(view.to_html(), r#"<input name="a" aria-invalid="true" data-id="7">"#);
}

#[test]
fn escaping_matches_the_one_byte_at_a_time_rule() {
    fn reference(text: &str) -> String {
        let mut out = String::new();
        for c in text.chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&#39;"),
                c => out.push(c),
            }
        }
        out
    }
    // Each escaped character, and its neighbours in ASCII (which the eight-byte test must not
    // mistake for it), at every offset of strings longer than one chunk, among multibyte text.
    let pieces = ["a", "&", "<", ">", "\"", "'", "%", "=", "?", ";", ":", "!", "#", "é", "★", "\u{7f}", "\u{80}", "\0"];
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..4000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let len = (seed % 40) as usize;
        let text: String = (0..len).map(|i| pieces[((seed >> (i % 60)) as usize + i * 7) % pieces.len()]).collect();
        let mut out = String::new();
        crate::view::escape_into(&text, &mut out);
        assert_eq!(out, reference(&text), "{text:?}");
    }
    for at in 0..24 {
        for special in ["&", "<", ">", "\"", "'"] {
            let text = format!("{}{special}{}", "x".repeat(at), "y".repeat(24 - at));
            let mut out = String::new();
            crate::view::escape_into(&text, &mut out);
            assert_eq!(out, reference(&text));
        }
    }
}

#[test]
fn floats_render_as_display_writes_them() {
    for value in [0.5, -1.25, 3.0, 1e21, 1.5e-7, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0] {
        assert_eq!(value.to_html(), value.to_string());
        assert_eq!(html! { <i data-v={value}></i> }.to_html(), format!("<i data-v=\"{value}\"></i>"));
    }
    assert_eq!(2.5f32.to_html(), "2.5");
    assert_eq!(u128::MAX.to_html(), u128::MAX.to_string());
}
