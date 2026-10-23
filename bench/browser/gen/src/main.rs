//! Renders the view apps' pages with html! and writes what #[component(view)] generates for `Row`
//! and `Card`: the `corten/views` module and its declarations. The handlers are the components'
//! scripts (bench.js, dashboard.js), run by Corten's runtime. `node build.mjs` runs it:
//!
//! `cargo run --release --manifest-path gen/Cargo.toml -- <rows.json> <out dir>`

use corten::{prelude::*, types::Types};
use serde::Deserialize;

/// A row's data, as the bench's generator makes it.
#[derive(Deserialize)]
struct Data {
    id: u32,
    label: String,
}

/// One row. `view` makes `Row` in `corten/views`: `id`, `label` and `selected` are its
/// properties, and `key={id}` is what `rows()` keys it by. `select` and `remove` are the
/// bench's refs, so its one click handler on the table body gets the link that was clicked.
#[component(view)]
pub fn Row<'a>(
    id: u32,
    label: &'a str,
    selected: bool,
    select: Ref<el::HTMLAnchorElement>,
    remove: Ref<el::HTMLAnchorElement>,
) -> impl Render {
    html! {
        <tr key={id} data-selected={selected}>
            <td class="col-md-1">{id}</td>
            <td class="col-md-4"><a class="select" ref={select}>{label}</a></td>
            <td class="col-md-1">
                <a class="remove" ref={remove}><span class="remove" aria-hidden="true">"x"</span></a>
            </td>
            <td class="col-md-6"></td>
        </tr>
    }
}

/// The page's `<main>`: the controls, the rows, and the template new rows are cloned from.
#[component(js)]
fn Bench<'a>(rows: &'a [Data]) -> impl Render {
    let (body, template, select, remove) = refs();
    html! {
        <main id="main" on:connect={js::ready()}>
            <div class="controls">
                <button id="create" on:click={js::create(body, template, 1000)}>"Create 1,000"</button>
                <button id="create10k" on:click={js::create(body, template, 10000)}>"Create 10,000"</button>
                <button id="append" on:click={js::append(body, template)}>"Append 1,000"</button>
                <button id="update" on:click={js::update(body)}>"Update every 10th"</button>
                <button id="swap" on:click={js::swap(body)}>"Swap rows"</button>
                <button id="clear" on:click={js::clear(body)}>"Clear"</button>
                <button id="patch1" on:click={js::patch1(body)}>"Patch 1"</button>
                <button id="patch100" on:click={js::patch100(body)}>"Patch 100"</button>
            </div>
            <table>
                <tbody
                    id="tbody"
                    ref={body}
                    on:click={js::select(select.closest(), body)}
                    on:click={js::remove(remove.closest(), body)}
                >
                    for r in rows.iter() {
                        <Row id={r.id} label={&r.label} selected={false} select={select} remove={remove} />
                    }
                </tbody>
            </table>
            <template ref={template}>
                <Row id={0} label="" selected={false} select={select} remove={remove} />
            </template>
        </main>
    }
}

/// One card of the dashboard: its price, marked when it went down.
#[component(view)]
pub fn Card<'a>(symbol: &'a str, price: &'a str, down: bool) -> impl Render {
    html! {
        <div class="card" key={symbol}><b>{symbol}</b>" "<span class="up" data-down={down}>{price}</span></div>
    }
}

/// The dashboard's `<main>`: the tick buttons, the total and 200 cards.
#[component(js)]
fn Dashboard<'a>(prices: &'a [(String, String)], total: &'a str) -> impl Render {
    let (cards, sum) = refs();
    html! {
        <main id="main" on:connect={js::ready()}>
            <div class="controls">
                <button id="tick20" on:click={js::tick(cards, sum, 20)}>"Tick 20"</button>
                <button id="tick200" on:click={js::tick(cards, sum, 200)}>"Tick 200"</button>
            </div>
            <p>"Total "<span ref={sum}>{total}</span></p>
            <div class="cards" ref={cards}>
                for (symbol, price) in prices.iter() {
                    <Card symbol={symbol} price={price} down={false} />
                }
            </div>
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
    // The market's 200 symbols at their start prices (apps/shared/market.js).
    let start = |i: usize| 100.0 + (i % 17) as f64;
    let prices: Vec<(String, String)> = (0..200).map(|i| (format!("S{i:03}"), format!("{:.2}", start(i)))).collect();
    let total = format!("{:.2}", (0..200).map(start).sum::<f64>());
    write("dashboard-view-main.html", Dashboard { prices: &prices, total: &total }.to_html());
    let views = [Row, Card];
    write("views.js", corten::script::views_module(&views).expect("one view per name").expect("Row is a view"));
    write("views.d.ts", Types::new().components(&views).typescript());
    // What Corten appends to each handler module it serves: the line that registers it.
    for module in ["bench", "dashboard"] {
        write(&format!("{module}.footer.js"), corten::script::served(module, ""));
    }
}
