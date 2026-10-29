//! The market the watchlist follows: the stocks it lists, their prices as a live feed, and the
//! last trade. A task of the app's moves one price a second (`App::task`), so the pages have
//! something to show.
use std::{sync::Arc, time::Duration};

use corten::{
    Signal,
    feed::{Feed, Row},
    i18n::{Currency, Decimal, Minor},
    prelude::*,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::app::Msg;

/// Every stock on the market: its ticker, the company and the price it opened at, in cents.
const LISTED: [(&str, &str, u32); 8] = [
    ("ACME", "Acme Corp", 12_999),
    ("GLBX", "Globex", 54_000),
    ("INIT", "Initech", 31_050),
    ("HLTH", "Halcyon Health", 8_840),
    ("PNTR", "Pantry & Co", 4_215),
    ("VTGO", "Vertigo Robotics", 71_525),
    ("NRTH", "Northgate Rail", 23_380),
    ("SOLR", "Sunreach Solar", 1_962),
];

/// The currency of every price.
pub const USD: Currency = Currency::new("USD");

#[derive(Clone)]
pub struct Market {
    pub stocks: Feed<Stock>,
    /// The last trade, as text: what `<corten-live>` shows, and how many trades came before it
    /// (the stream's cursor).
    pub trades: Arc<watch::Sender<(u64, String)>>,
}

impl Market {
    pub fn new() -> Self {
        Market {
            stocks: Feed::new(LISTED.map(|(id, name, cents)| Stock::new(id, name, cents))),
            trades: Arc::new(watch::Sender::new((0, "No trades yet.".into()))),
        }
    }

    /// The market's ticker `id`, when the market lists it.
    pub fn ticker(id: &str) -> Option<&'static str> {
        LISTED.iter().find(|(listed, ..)| *listed == id).map(|(ticker, ..)| *ticker)
    }

    /// Every stock on the market by name, as `(ticker, name)`: a form's choices, with no row
    /// copied out of the feed.
    pub fn listed() -> impl Iterator<Item = (&'static str, &'static str)> {
        let mut listed = LISTED.map(|(id, name, _)| (id, name));
        listed.sort_unstable_by_key(|(_, name)| *name);
        listed.into_iter()
    }

    /// One trade: the `n`th stock in a fixed walk moves by a fraction of its price, and the last
    /// trade's text follows.
    pub(crate) fn trade(&self, n: usize) {
        let (id, ..) = LISTED[n * 3 % LISTED.len()];
        // Basis points (hundredths of a percent) of the price.
        let step = [35i64, -20, 50, -45, 15, -30, 25][n % 7];
        let trade = self.stocks.write(|tx| {
            let stock = tx.get_mut(id)?;
            let cents = i64::from(stock.price_cents);
            // At least a cent, so a cheap stock moves too.
            let delta = match cents * step / 10_000 {
                0 => step.signum(),
                delta => delta,
            };
            stock.set_price((cents + delta).max(100) as u32);
            Some(format!("Last trade: {} at {}.", stock.name, stock.price))
        });
        if let Some(trade) = trade {
            self.trades.send_modify(|(n, text)| {
                *n += 1;
                *text = trade;
            });
        }
    }
}

/// Trades once a second until the app shuts down: the signal aborts, and the task returns between
/// trades, never in the middle of one.
pub async fn simulate(market: Market, signal: Signal) {
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    for n in 0.. {
        tokio::select! {
            biased;
            _ = signal.aborted() => return,
            _ = tick.tick() => market.trade(n),
        }
    }
}

/// A feed row. `price` and `change` are the display text, for clients with no formatting of their
/// own (a native screen, an agent); the page's script writes `price_cents` and `change_bp` with
/// the page's formats (`t.js().money(USD)`, `t.js().change(2)`), which write what the server
/// does.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToJson)]
pub struct Stock {
    /// The ticker.
    pub id: String,
    pub name: String,
    pub price_cents: u32,
    /// The price the day opened at.
    pub open_cents: u32,
    /// The change since the open, in basis points (hundredths of a percent), rounded.
    pub change_bp: i32,
    pub price: String,
    pub change: String,
}

impl Stock {
    fn new(id: &str, name: &str, cents: u32) -> Self {
        let mut stock = Stock {
            id: id.into(),
            name: name.into(),
            price_cents: cents,
            open_cents: cents,
            change_bp: 0,
            price: String::new(),
            change: String::new(),
        };
        stock.set_price(cents);
        stock
    }

    fn set_price(&mut self, cents: u32) {
        let (now, open) = (i64::from(cents), i64::from(self.open_cents));
        // Rounded half away from zero, as the percentage format rounds.
        let scaled = (now - open) * 10_000;
        let half = if scaled < 0 { -open / 2 } else { open / 2 };
        self.price_cents = cents;
        self.change_bp = ((scaled + half) / open) as i32;
        let t = Msg::default();
        self.price = t.money(Minor(cents), USD).to_string();
        self.change = t.change(self.ratio(), 2).to_string();
    }

    /// The change since the open as a ratio: `t.change(stock.ratio(), 2)` is `+1.23%`.
    pub fn ratio(&self) -> Decimal {
        Decimal::new(i128::from(self.change_bp), 4)
    }

    /// Which way the price went since the open, for the page's colors.
    pub fn trend(&self) -> &'static str {
        match self.change_bp {
            0 => "flat",
            bp if bp > 0 => "up",
            _ => "down",
        }
    }
}

impl Row for Stock {
    fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trade_moves_a_price_and_its_change_since_the_open() {
        let market = Market::new();
        market.trade(0);
        let acme = market.stocks.read(|rows| rows["ACME"].clone());
        // 35 basis points of $129.99 is 45 cents.
        assert_eq!(
            (acme.price_cents, acme.change_bp, acme.price.as_str(), acme.change.as_str(), acme.trend()),
            (13_044, 35, "$130.44", "+0.35%", "up")
        );
        assert_eq!(market.trades.borrow().clone(), (1, "Last trade: Acme Corp at $130.44.".to_string()));
        // The sixth trade is Sunreach Solar's, down 30 basis points: 5 cents of $19.62.
        market.trade(5);
        let solr = market.stocks.read(|rows| rows["SOLR"].clone());
        assert_eq!((solr.price_cents, solr.change_bp, solr.change.as_str(), solr.trend()), (1_957, -25, "-0.25%", "down"));
    }

    #[test]
    fn the_change_is_rounded_half_away_from_zero() {
        let mut stock = Stock::new("X", "X", 20_000);
        stock.set_price(19_999);
        // -0.5 basis points.
        assert_eq!((stock.change_bp, stock.change.as_str(), stock.trend()), (-1, "-0.01%", "down"));
    }
}
