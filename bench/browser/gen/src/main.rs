//! Renders the view apps' pages with html! and writes what #[component(view)] generates for `Row`:
//! the `corten/views` module and its declarations. `node build.mjs` runs it:
//!
//! `cargo run --release --manifest-path gen/Cargo.toml -- <rows.json> <out dir>`

use corten::{prelude::*, script::Bind, types::Types};
use serde::Deserialize;

/// A row's data, as the bench's generator makes it.
#[derive(Deserialize)]
struct Data {
    id: u32,
    label: String,
}

/// The prototype's handler names: `data-on="click:select"`, which state.js's `mount` looks up in
/// the object the setup returns. (The framework's own handlers are `js::name(..)` calls.)
struct On(&'static str);

impl Bind for On {
    fn write_bind(&self, out: &mut String) {
        out.push_str(self.0)
    }
}

/// One row. `view` makes `Row` in `corten/views`: `id`, `label` and `selected` are its
/// properties, and `key={id}` is what `rows()` keys it by.
#[component(view)]
pub fn Row<'a>(id: u32, label: &'a str, selected: bool) -> impl Render {
    html! {
        <tr key={id} data-selected={selected}>
            <td class="col-md-1">{id}</td>
            <td class="col-md-4"><a class="select" on:click={On("select")}>{label}</a></td>
            <td class="col-md-1">
                <a class="remove" on:click={On("remove")}><span class="remove" aria-hidden="true">"x"</span></a>
            </td>
            <td class="col-md-6"></td>
        </tr>
    }
}

const BUTTONS: [(&str, &str); 8] = [
    ("create", "Create 1,000"),
    ("create10k", "Create 10,000"),
    ("append", "Append 1,000"),
    ("update", "Update every 10th"),
    ("swap", "Swap rows"),
    ("clear", "Clear"),
    ("patch1", "Patch 1"),
    ("patch100", "Patch 100"),
];

/// The page's `<main>`: the controls, the rows, and the template new rows are cloned from.
#[component]
fn Bench<'a>(rows: &'a [Data]) -> impl Render {
    let (body, row) = refs();
    html! {
        <main id="main">
            <div class="controls">
                for (id, text) in BUTTONS {
                    <button id={id} on:click={On(id)}>{text}</button>
                }
            </div>
            <table>
                <tbody id="tbody" ref={body}>
                    for r in rows.iter() {
                        <Row id={r.id} label={&r.label} selected={false} />
                    }
                </tbody>
            </table>
            <template ref={row}><Row id={0} label="" selected={false} /></template>
        </main>
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, input, out] = args.as_slice() else { panic!("usage: corten-bench-gen <rows.json> <out dir>") };
    let rows: Vec<Data> = serde_json::from_str(&std::fs::read_to_string(input).expect("read the rows")).expect("parse the rows");
    let out = std::path::Path::new(out);
    std::fs::create_dir_all(out).expect("make the out dir");
    let write = |name: &str, text: String| std::fs::write(out.join(name), text).expect("write");
    write("view-main.html", Bench { rows: &[] }.to_html());
    write("view-ssr-main.html", Bench { rows: &rows }.to_html());
    write("views.js", corten::script::views_module(&[Row]).expect("one view per name").expect("Row is a view"));
    write("views.d.ts", Types::new().components(&[Row]).typescript());
}
