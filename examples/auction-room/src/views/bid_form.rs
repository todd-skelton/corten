use corten::prelude::*;

use crate::app::{BidInput, me};

/// The bid form's id: a bid's answer replaces it, cleared with a new submit key, or with its error.
#[derive(Id)]
pub struct BidFormId;

/// The bid form: an amount as the reader writes money, and this submit's key, so a double click
/// or a retry is one bid. Without JavaScript it posts and the page comes back.
#[component(css)]
pub fn BidForm<'a>(lot: &'a str, typed: &'a str, errors: &'a Errors) -> impl Patch {
    let f = BidInput::FIELDS;
    let e = errors.within(BidFormId);
    html! {
        <form id={BidFormId} class={Bid} method="post" action={me::Bid { lot }} data-patch>
            <label>
                "Your bid"
                <input name={f.amount} value={typed} inputmode="decimal" autocomplete="off" required {..e.aria(f.amount)}>
            </label>
            {SubmitKey::field(f.submit)}
            <button>"Place bid"</button>
            {e.message(f.amount)}
        </form>
    }
}
