use std::fmt;

use crate::view::{component, html, raw, Attrs, Document, Part, Render};
use crate::Corten;

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

/// A pattern's braces are the pattern's, as in Rust: an `if let` up to its `=`, a `for` up to
/// `in`. A range pattern's `..=` and a comparison's `==` are not the end of one.
#[test]
fn patterns_with_braces_are_patterns() {
    enum Shape {
        Circle { r: u32 },
        Square { side: u32 },
        Dot,
    }
    struct Point {
        x: i32,
        y: i32,
    }
    let shapes = [Shape::Circle { r: 2 }, Shape::Square { side: 3 }, Shape::Dot];
    let points = [Point { x: 1, y: 2 }];
    let view = html! {
        for shape in &shapes {
            if let Shape::Circle { r } = shape {
                <i>{r}</i>
            } else if let Shape::Square { side: 1..=5 } = shape {
                <b>"small"</b>
            } else if points.len() == usize::from(matches!(shape, Shape::Dot)) {
                <u>"dot"</u>
            }
        }
        for Point { x, y } in &points { <p>{x}","{y}</p> }
    };
    assert_eq!(view.to_html(), "<i>2</i><b>small</b><u>dot</u><p>1,2</p>");
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

#[component]
fn Bar<'a>(action: &'a dyn Render) -> impl Render {
    html! { <div>{action}</div> }
}

#[component]
fn Slot<C: Render>(view: C) -> impl Render {
    html! { <b>{view}</b> }
}

/// A view written as a prop borrows what it uses, so the rest of the page reads the same local
/// (#414), as a view in a view in a prop does, and one given by value (`view={html! { .. }}`).
#[test]
fn a_view_in_a_prop_borrows_what_it_uses() {
    let name = Some(String::from("Ship"));
    let view = html! {
        <Panel title="Steps" owned={String::new()}>
            <Bar action={&html! { if let Some(n) = &name { <span>{n}</span> } }} />
            <Slot view={html! { <Bar action={&html! { {name.as_deref().unwrap_or("")} }} /> {&name} }} />
            if let Some(n) = &name { <p>{n}</p> }
        </Panel>
    };
    assert_eq!(
        view.to_html(),
        r#"<section data-owned=""><h2>Steps</h2><div><span>Ship</span></div><b><div>Ship</div>Ship</b><p>Ship</p></section>"#
    );
    // Within an element too, at the top of a view that is not one component.
    let name = String::from("Ship");
    let view = html! { <main><Bar action={&html! { {&name} }} />{&name}</main> };
    assert_eq!(view.to_html(), "<main><div>Ship</div>Ship</main>");
}

/// A view returned from a function still owns what it uses, its props' views included.
fn shipped(name: String) -> impl Render {
    html! { <Slot view={html! { <i>{&name}</i> }} /> }
}

#[test]
fn a_returned_view_owns_its_props_views() {
    assert_eq!(shipped(String::from("Ship")).to_html(), "<b><i>Ship</i></b>");
}

/// A view that is one component whose props borrow what it makes: the `let`s inside the markup
/// belong to the view, so it can be returned (#77).
fn counted(n: usize) -> impl Render {
    html! {
        let title = format!("{n} items");
        let label = String::from("New");
        <Panel title={&title} owned={label.clone()}>
            <Badge label={&label} count={n} hot={false} />
            {&title}
        </Panel>
    }
}

#[test]
fn a_view_that_is_one_component_owns_what_its_props_borrow() {
    assert_eq!(
        counted(3).to_html(),
        r#"<section data-owned="New"><h2>3 items</h2><span class="badge">New 3</span>3 items</section>"#
    );
}

#[component]
fn Doc<'a, C: Render>(title: &'a str, children: C) -> impl Document + 'a {
    html! {
        <html>
            <head>
                <title>{title}</title>
                <Corten />
            </head>
            <body><main>{children}</main></body>
        </html>
    }
}

/// The same for a layout: `let`s and a component that is a document make a document.
fn page(n: usize) -> impl Document {
    html! {
        let title = format!("{n} items");
        <Doc title={&title}><p>{&title}</p></Doc>
    }
}

#[test]
fn a_document_that_is_one_component_owns_what_its_props_borrow() {
    let mut title = String::new();
    page(2).render_part(&mut title, Part::Title);
    assert_eq!(title, "2 items");
    assert!(page(2).to_html().starts_with("<html><head><title>2 items</title>"));
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

#[test]
fn floats_match_display_across_their_range() {
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let special = [
        0.0,
        -0.0,
        1.0,
        0.1,
        0.3,
        1e15,
        1e16,
        1e17,
        1e21,
        1e22,
        123456789012345680.0,
        1e-5,
        1e-7,
        5e-324,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN,
        f64::EPSILON,
        9007199254740993.0,
    ];
    for value in special {
        assert_eq!(value.to_html(), value.to_string(), "{value:e}");
    }
    // Ties: halfway between two shortest forms (zmij keeps the even digit, Display rounds up).
    assert_eq!(f32::from_bits(0x4898_d324).to_html(), "312985.13");
    for value in [312_985.125, 312_985.0 + 1.0 / 2048.0, 2f64.powi(50) + 0.25, 0.25, 7.375] {
        assert_eq!(value.to_html(), value.to_string(), "{value:e}");
    }
    // Short binary fractions in every binade, where ties live.
    for shift in 0..60 {
        for odd in (1..4000u64).step_by(2) {
            let value = odd as f64 * 2f64.powi(-shift) * (1 + (odd % 7) * 1000) as f64;
            assert_eq!(value.to_html(), value.to_string(), "{value:e}");
            assert_eq!((-value).to_html(), (-value).to_string(), "{value:e}");
        }
    }
    for _ in 0..200_000 {
        // Every bit pattern (subnormals, huge, tiny), and prices and other short decimals.
        let bits = f64::from_bits(next());
        let price = (next() % 10_000_000) as f64 / [1.0, 10.0, 100.0, 1000.0][(next() % 4) as usize];
        let float = f32::from_bits(next() as u32);
        assert_eq!(bits.to_html(), bits.to_string(), "{:#x}", bits.to_bits());
        assert_eq!(price.to_html(), price.to_string(), "{price:e}");
        assert_eq!(float.to_html(), float.to_string(), "{:#x}", float.to_bits());
    }
}

#[test]
fn a_page_s_head_tags_are_marked_at_the_top_level_and_raw_text_is_left_alone() {
    let cases = [
        (
            r#"<meta name="description" content="a &lt;b&gt;">"#,
            r#"<meta name="description" content="a &lt;b&gt;" data-corten-head>"#,
        ),
        (
            "<link rel=canonical href=/a><meta charset=utf-8>",
            "<link rel=canonical href=/a data-corten-head><meta charset=utf-8 data-corten-head>",
        ),
        ("<noscript><style>p{}</style></noscript>", "<noscript data-corten-head><style>p{}</style></noscript>"),
        (
            r#"<script type="application/ld+json">{"a":"<b>"}</script><meta x>"#,
            r#"<script type="application/ld+json" data-corten-head>{"a":"<b>"}</script><meta x data-corten-head>"#,
        ),
        ("<style>a>b{}</style>", "<style data-corten-head>a>b{}</style>"),
        ("<!-- a --><meta x/>", "<!-- a --><meta x data-corten-head/>"),
        ("", ""),
    ];
    for (page, marked) in cases {
        let mut out = String::new();
        crate::view::__private::head_slot(|out| out.push_str(page), &mut out);
        assert_eq!(out, marked, "{page}");
    }
}

/// Text that counts how often it renders.
struct Count<'a>(&'a std::cell::Cell<u32>);

impl Render for Count<'_> {
    fn render(&self, out: &mut String) {
        self.0.set(self.0.get() + 1);
        out.push_str("Shop");
    }
}

/// A layout whose header counts its renders: a navigation must not run it.
#[component]
fn Counted<'a, C: Render>(title: &'a str, renders: &'a std::cell::Cell<u32>, children: C) -> impl crate::Document + 'a {
    html! {
        <html>
            <head>
                <title>{title}" | Shop"</title>
                <crate::Corten />
            </head>
            <body>
                <header>{Count(renders)}</header>
                <main>{children}</main>
                <footer>"(c)"</footer>
            </body>
        </html>
    }
}

#[test]
fn a_document_renders_each_part_and_a_swap_skips_the_layout() {
    use crate::view::{Document, Part};
    let renders = std::cell::Cell::new(0);
    let doc = || Counted { title: "Lamp", renders: &renders, children: html! { <h1>"Lamp"</h1> } };
    let part = |part| {
        let mut out = String::new();
        doc().render_part(&mut out, part);
        out
    };
    assert_eq!(
        part(Part::Full),
        "<html><head><title>Lamp | Shop</title></head><body><header>Shop</header><main><h1>Lamp</h1></main><footer>(c)</footer></body></html>"
    );
    assert_eq!(renders.get(), 1);
    assert_eq!(part(Part::Swap), "<title>Lamp | Shop</title><h1>Lamp</h1>");
    assert_eq!(part(Part::Title), "Lamp | Shop");
    assert_eq!(renders.get(), 1, "only the full document runs the layout");
    assert_eq!(doc().to_html(), part(Part::Full), "Render is the full document");
}
