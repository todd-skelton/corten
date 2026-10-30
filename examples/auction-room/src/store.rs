//! What the auction keeps, in SQLite: who is signed in, the lots and their bids.
//!
//! A bid is checked and written in one `BEGIN IMMEDIATE` transaction, which takes SQLite's write
//! lock before it reads, and reads the time after it has the lock: two bids at once are one after
//! the other, the second is checked against the first, and a bid that waited for the lock past
//! the end is refused. Its submit key is unique, so the same bid sent twice is written once. Every
//! change to a lot (a bid, the house ending it, closing it) adds one to its `version` in the same
//! transaction, so of two readings of a lot the later one is the one with the higher version.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sqlx::{
    Row as _, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
};

pub use sqlx::Error;

/// The tables, made when the database is new.
const SCHEMA: [&str; 4] = [
    "CREATE TABLE IF NOT EXISTS sessions (
        token TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        -- The house (the auctioneer), who may end a lot early.
        house INTEGER NOT NULL,
        started_at INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS lots (
        id TEXT PRIMARY KEY,
        card TEXT NOT NULL,
        card_set TEXT NOT NULL,
        condition TEXT NOT NULL,
        opening_cents INTEGER NOT NULL,
        step_cents INTEGER NOT NULL,
        ends_at INTEGER NOT NULL,
        -- Counts the lot's changes.
        version INTEGER NOT NULL DEFAULT 0,
        -- Set once the lot closed and its winner was told.
        closed INTEGER NOT NULL DEFAULT 0
    )",
    "CREATE TABLE IF NOT EXISTS bids (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        lot TEXT NOT NULL REFERENCES lots (id),
        bidder TEXT NOT NULL,
        cents INTEGER NOT NULL,
        at INTEGER NOT NULL,
        submit TEXT NOT NULL UNIQUE
    )",
    "CREATE INDEX IF NOT EXISTS bids_by_lot ON bids (lot, id DESC)",
];

/// How many of a lot's bids its page lists, newest first.
pub const RECENT: i64 = 8;

/// How long a session lasts after signing in.
const SESSION_DAYS: u64 = 30;

/// The database. Cheap to clone; clones share the connections.
#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

/// A card up for auction.
#[derive(Clone, Debug, PartialEq)]
pub struct Lot {
    pub id: String,
    pub card: String,
    pub set: String,
    pub condition: String,
    pub opening_cents: u64,
    /// How much a bid must beat the price by.
    pub step_cents: u64,
    pub ends_at: SystemTime,
}

/// One bid on a lot.
#[derive(Clone, Debug, PartialEq)]
pub struct Bid {
    pub bidder: String,
    pub cents: u64,
    pub at: SystemTime,
}

/// A lot as it stands: its newest bids (the first is the price, since each bid beats the last),
/// how many it has had, and its version (how many times it changed).
#[derive(Clone, Debug, PartialEq)]
pub struct Standing {
    pub lot: Lot,
    pub recent: Vec<Bid>,
    pub count: u64,
    pub version: u64,
    /// Closed, and its winner told.
    pub closed: bool,
}

impl Standing {
    /// The highest bid, which leads.
    pub fn top(&self) -> Option<&Bid> {
        self.recent.first()
    }

    /// The least the next bid may be.
    pub fn minimum(&self) -> u64 {
        self.top().map_or(self.lot.opening_cents, |top| top.cents + self.lot.step_cents)
    }

    /// The lot no longer takes bids.
    pub fn ended(&self, now: SystemTime) -> bool {
        self.closed || now >= self.lot.ends_at
    }
}

/// A bid the store took.
#[derive(Debug, PartialEq)]
pub enum Placed {
    /// Written now. `outbid` is who led before, when it was someone else.
    New { outbid: Option<String> },
    /// Written before, by a submit with the same key (a double click, a retry): nothing changed.
    Again,
}

/// Why a bid was not taken.
#[derive(Debug, PartialEq)]
pub enum Refusal {
    NoLot,
    Closed,
    /// Below the price plus the step: the least it may be.
    TooLow {
        minimum: u64,
    },
    /// Above `MAX_CENTS`.
    TooHigh,
}

/// The most a bid may be, $1,000,000: far above any card here, and far below what an `i64` of
/// cents holds, so a price plus a step never overflows.
pub const MAX_CENTS: u64 = 100_000_000;

/// A lot that closed now: its top bid, the winner, if it had one.
#[derive(Debug, PartialEq)]
pub struct Closed {
    pub top: Option<Bid>,
}

fn bid(row: &SqliteRow) -> Bid {
    Bid { bidder: row.get("bidder"), cents: row.get::<i64, _>("cents") as u64, at: time(row.get("at")) }
}

fn ms(at: SystemTime) -> i64 {
    at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn time(ms: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms.max(0) as u64)
}

fn lot(row: &SqliteRow) -> Lot {
    Lot {
        id: row.get("id"),
        card: row.get("card"),
        set: row.get("card_set"),
        condition: row.get("condition"),
        opening_cents: row.get::<i64, _>("opening_cents") as u64,
        step_cents: row.get::<i64, _>("step_cents") as u64,
        ends_at: time(row.get("ends_at")),
    }
}

impl Store {
    /// Opens (or makes) the database at `url`: `sqlite:auction.db?mode=rwc`, or
    /// `sqlite::memory:` for one that lives as long as the program.
    pub async fn open(url: &str) -> Result<Self, Error> {
        let options: SqliteConnectOptions = url.parse()?;
        let memory = options.get_filename().as_os_str().is_empty() || url.contains(":memory:");
        let options = options
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5))
            // Readers do not wait for a writer, and a bid's write waits for the one before it.
            .journal_mode(if memory { SqliteJournalMode::Memory } else { SqliteJournalMode::Wal });
        // An in-memory database is one connection's: keep that one, always.
        let pool = if memory {
            SqlitePoolOptions::new().max_connections(1).min_connections(1).idle_timeout(None).max_lifetime(None)
        } else {
            SqlitePoolOptions::new().max_connections(8)
        }
        .connect_with(options)
        .await?;
        for statement in SCHEMA {
            sqlx::query(statement).execute(&pool).await?;
        }
        Ok(Store { pool })
    }

    /// A new session for `name` (the house's when `house`), named by `token`. Sessions past their
    /// days go first, so the table holds only those that still sign someone in.
    pub async fn start_session(&self, token: &str, name: &str, house: bool) -> Result<(), Error> {
        let since = ms(SystemTime::now() - Duration::from_secs(SESSION_DAYS * 24 * 60 * 60));
        sqlx::query("DELETE FROM sessions WHERE started_at <= ?").bind(since).execute(&self.pool).await?;
        sqlx::query("INSERT INTO sessions (token, name, house, started_at) VALUES (?, ?, ?, ?)")
            .bind(token)
            .bind(name)
            .bind(house)
            .bind(ms(SystemTime::now()))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Who the session `token` names, and whether it is the house's, if it is one the store has
    /// and it started less than `SESSION_DAYS` ago.
    pub async fn session(&self, token: &str) -> Result<Option<(String, bool)>, Error> {
        let since = ms(SystemTime::now() - Duration::from_secs(SESSION_DAYS * 24 * 60 * 60));
        let row = sqlx::query("SELECT name, house FROM sessions WHERE token = ? AND started_at > ?")
            .bind(token)
            .bind(since)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|row| (row.get("name"), row.get("house"))))
    }

    pub async fn end_session(&self, token: &str) -> Result<(), Error> {
        sqlx::query("DELETE FROM sessions WHERE token = ?").bind(token).execute(&self.pool).await?;
        Ok(())
    }

    /// Adds lots (the demo's, at start).
    pub async fn add_lots(&self, lots: &[Lot]) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;
        for lot in lots {
            sqlx::query(
                "INSERT INTO lots (id, card, card_set, condition, opening_cents, step_cents, ends_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&lot.id)
            .bind(&lot.card)
            .bind(&lot.set)
            .bind(&lot.condition)
            .bind(lot.opening_cents as i64)
            .bind(lot.step_cents as i64)
            .bind(ms(lot.ends_at))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// Every lot, the one that ends first first.
    pub async fn lots(&self) -> Result<Vec<Lot>, Error> {
        let rows = sqlx::query("SELECT * FROM lots ORDER BY ends_at, id").fetch_all(&self.pool).await?;
        Ok(rows.iter().map(lot).collect())
    }

    /// How lot `id` stands, if there is one: read in one transaction, so the bids, the count and
    /// the version are of one moment.
    pub async fn standing(&self, id: &str) -> Result<Option<Standing>, Error> {
        let mut tx = self.pool.begin().await?;
        let Some(row) = sqlx::query("SELECT * FROM lots WHERE id = ?").bind(id).fetch_optional(&mut *tx).await? else {
            return Ok(None);
        };
        let recent = sqlx::query("SELECT bidder, cents, at FROM bids WHERE lot = ? ORDER BY id DESC LIMIT ?")
            .bind(id)
            .bind(RECENT)
            .fetch_all(&mut *tx)
            .await?
            .iter()
            .map(bid)
            .collect();
        let count: i64 = sqlx::query("SELECT COUNT(*) FROM bids WHERE lot = ?").bind(id).fetch_one(&mut *tx).await?.get(0);
        tx.commit().await?;
        Ok(Some(Standing {
            lot: lot(&row),
            recent,
            count: count as u64,
            version: row.get::<i64, _>("version") as u64,
            closed: row.get("closed"),
        }))
    }

    /// Ends lot `id` now, when it was to end later: the house closing it early. The time is read
    /// after the write lock, so a bid that waited for the lock is checked against the new end.
    pub async fn end_lot(&self, id: &str) -> Result<(), Error> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = ms(SystemTime::now());
        sqlx::query("UPDATE lots SET ends_at = ?, version = version + 1 WHERE id = ? AND ends_at > ?")
            .bind(now)
            .bind(id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        tx.commit().await
    }

    /// Closes lot `id` once its end has passed, and returns its top bid (the winner), the first
    /// time only. Bids are written in the same lock, so the top bid read here is the last one taken.
    pub async fn close_lot(&self, id: &str) -> Result<Option<Closed>, Error> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let closed =
            sqlx::query("UPDATE lots SET closed = 1, version = version + 1 WHERE id = ? AND closed = 0 AND ends_at <= ?")
                .bind(id)
                .bind(ms(SystemTime::now()))
                .execute(&mut *tx)
                .await?;
        if closed.rows_affected() == 0 {
            return Ok(None);
        }
        let top = sqlx::query("SELECT bidder, cents, at FROM bids WHERE lot = ? ORDER BY id DESC LIMIT 1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .map(|row| bid(&row));
        tx.commit().await?;
        Ok(Some(Closed { top }))
    }

    #[cfg(test)]
    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// Every bid on lot `id`, oldest first, for its history file.
    pub async fn history(&self, id: &str) -> Result<Vec<Bid>, Error> {
        let rows =
            sqlx::query("SELECT bidder, cents, at FROM bids WHERE lot = ? ORDER BY id").bind(id).fetch_all(&self.pool).await?;
        Ok(rows.iter().map(bid).collect())
    }

    /// Bids `cents` on lot `id` for `bidder`, once per `submit` key: taken only while the lot is
    /// open and at or above its price plus the step, and at most `MAX_CENTS`.
    pub async fn bid(&self, id: &str, bidder: &str, cents: u64, submit: &str) -> Result<Result<Placed, Refusal>, Error> {
        // The write lock first, so nothing changes between the checks and the write, and the time
        // after it: a bid that waited for the lock past the end is too late.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = SystemTime::now();
        if sqlx::query("SELECT 1 FROM bids WHERE submit = ?").bind(submit).fetch_optional(&mut *tx).await?.is_some() {
            return Ok(Ok(Placed::Again));
        }
        let Some(row) = sqlx::query("SELECT * FROM lots WHERE id = ?").bind(id).fetch_optional(&mut *tx).await? else {
            return Ok(Err(Refusal::NoLot));
        };
        let lot = lot(&row);
        if row.get::<bool, _>("closed") || now >= lot.ends_at {
            return Ok(Err(Refusal::Closed));
        }
        let top = sqlx::query("SELECT bidder, cents FROM bids WHERE lot = ? ORDER BY id DESC LIMIT 1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .map(|row| (row.get::<String, _>("bidder"), row.get::<i64, _>("cents") as u64));
        let minimum = top.as_ref().map_or(lot.opening_cents, |(_, cents)| cents + lot.step_cents);
        if cents < minimum {
            return Ok(Err(Refusal::TooLow { minimum }));
        }
        if cents > MAX_CENTS {
            return Ok(Err(Refusal::TooHigh));
        }
        sqlx::query("INSERT INTO bids (lot, bidder, cents, at, submit) VALUES (?, ?, ?, ?, ?)")
            .bind(id)
            .bind(bidder)
            .bind(cents as i64)
            .bind(ms(now))
            .bind(submit)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE lots SET version = version + 1 WHERE id = ?").bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Ok(Placed::New { outbid: top.map(|(who, _)| who).filter(|who| who != bidder) }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, ends_at: SystemTime) -> Lot {
        Lot {
            id: id.into(),
            card: "Ashen Drake".into(),
            set: "Ember Reach".into(),
            condition: "Near Mint".into(),
            opening_cents: 1000,
            step_cents: 100,
            ends_at,
        }
    }

    async fn store() -> Store {
        let store = Store::open("sqlite::memory:").await.unwrap();
        let now = SystemTime::now();
        store
            .add_lots(&[card("drake", now + Duration::from_secs(600)), card("done", now - Duration::from_secs(1))])
            .await
            .unwrap();
        store
    }

    #[tokio::test]
    async fn a_bid_below_the_price_and_the_step_is_refused() {
        let store = store().await;
        assert_eq!(store.bid("drake", "Ada", 999, "k1").await.unwrap(), Err(Refusal::TooLow { minimum: 1000 }));
        assert_eq!(store.bid("drake", "Ada", 1000, "k2").await.unwrap(), Ok(Placed::New { outbid: None }));
        assert_eq!(store.bid("drake", "Grace", 1099, "k3").await.unwrap(), Err(Refusal::TooLow { minimum: 1100 }));
        assert_eq!(store.bid("drake", "Grace", 1100, "k4").await.unwrap(), Ok(Placed::New { outbid: Some("Ada".into()) }));
        // Raising your own bid outbids no one.
        assert_eq!(store.bid("drake", "Grace", 1500, "k5").await.unwrap(), Ok(Placed::New { outbid: None }));
        let standing = store.standing("drake").await.unwrap().unwrap();
        assert_eq!((standing.count, standing.top().map(|b| b.cents), standing.minimum()), (3, Some(1500), 1600));
    }

    #[tokio::test]
    async fn the_same_submit_is_written_once() {
        let store = store().await;
        assert_eq!(store.bid("drake", "Ada", 1000, "same").await.unwrap(), Ok(Placed::New { outbid: None }));
        assert_eq!(store.bid("drake", "Ada", 1000, "same").await.unwrap(), Ok(Placed::Again));
        assert_eq!(store.standing("drake").await.unwrap().unwrap().count, 1);
    }

    #[tokio::test]
    async fn a_closed_lot_or_none_takes_no_bid() {
        let store = store().await;
        assert_eq!(store.bid("done", "Ada", 5000, "k1").await.unwrap(), Err(Refusal::Closed));
        assert_eq!(store.bid("nope", "Ada", 5000, "k2").await.unwrap(), Err(Refusal::NoLot));
    }

    #[tokio::test]
    async fn a_lot_closes_once_with_the_last_bid_taken_and_then_takes_none() {
        let store = store().await;
        assert_eq!(store.bid("drake", "Ada", 1000, "k1").await.unwrap(), Ok(Placed::New { outbid: None }));
        assert_eq!(store.close_lot("drake").await.unwrap(), None, "not ended yet");
        let before = store.standing("drake").await.unwrap().unwrap().version;
        store.end_lot("drake").await.unwrap();
        let closed = store.close_lot("drake").await.unwrap().unwrap();
        assert_eq!(closed.top.map(|bid| (bid.bidder, bid.cents)), Some(("Ada".to_string(), 1000)));
        assert_eq!(store.close_lot("drake").await.unwrap(), None, "closed once");
        let after = store.standing("drake").await.unwrap().unwrap();
        assert!(after.closed && after.version == before + 2, "{after:?}");
        assert_eq!(store.bid("drake", "Grace", 5000, "k2").await.unwrap(), Err(Refusal::Closed));
    }

    #[tokio::test]
    async fn a_bid_above_the_most_is_refused() {
        let store = store().await;
        assert_eq!(store.bid("drake", "Ada", MAX_CENTS + 1, "k1").await.unwrap(), Err(Refusal::TooHigh));
        assert_eq!(store.bid("drake", "Ada", u64::MAX, "k2").await.unwrap(), Err(Refusal::TooHigh));
    }

    #[tokio::test]
    async fn bids_at_once_are_checked_one_after_the_other() {
        let path = std::env::temp_dir().join(format!("auction-{}.db", std::process::id()));
        let url = format!("sqlite:{}?mode=rwc", path.display());
        let store = Store::open(&url).await.unwrap();
        let now = SystemTime::now();
        store.add_lots(&[card("drake", now + Duration::from_secs(600))]).await.unwrap();
        // Twenty bidders bid the opening price together: one is taken, the rest are too low.
        let bids = (0..20).map(|n| {
            let store = store.clone();
            tokio::spawn(async move { store.bid("drake", &format!("b{n}"), 1000, &format!("k{n}")).await.unwrap() })
        });
        let mut taken = 0;
        for bid in bids {
            if matches!(bid.await.unwrap(), Ok(Placed::New { .. })) {
                taken += 1;
            }
        }
        assert_eq!(taken, 1);
        assert_eq!(store.standing("drake").await.unwrap().unwrap().count, 1);
        drop(store);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
}
