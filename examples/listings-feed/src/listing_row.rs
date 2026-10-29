use corten::prelude::*;

/// One listing. `view` makes `ListingRow` in `corten/views`, whose holes the table's script
/// writes by name when the feed changes the listing: `price` and `market` (written with the
/// page's money format), `qty`, `paused`, and `above` (its price is over the market low, which
/// the row marks). Its key is the listing's id, so the script finds it by the feed row's id
/// (`rows(body, ListingRow).get(id)`).
#[component(view)]
pub fn ListingRow<'a>(
    id: &'a str,
    title: &'a str,
    category: &'a str,
    condition: &'a str,
    price: &'a str,
    market: &'a str,
    qty: u32,
    paused: bool,
    above: bool,
) -> impl Render {
    html! {
        <tr key={id} data-paused={paused} data-above={above}>
            <td>
                <span>{title}</span>
                <small>{id}</small>
            </td>
            <td>{category}</td>
            <td>{condition}</td>
            <td>{qty}</td>
            <td>{price}</td>
            <td>{market}</td>
        </tr>
    }
}
