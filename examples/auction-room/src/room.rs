//! What is live: each lot as it stands, for everyone watching it, and each bidder's own notices,
//! for them alone.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::sync::{Notify, watch};

use crate::store::{Lot, Standing};

/// A lot as the room last heard it, and how many times it has changed: the cursor a page's live
/// stream starts from, so a page already showing this version is sent nothing until the next.
#[derive(Clone, Debug)]
pub struct Live {
    pub version: u64,
    pub standing: Arc<Standing>,
}

/// What happened to one bidder. Kept as data, not text, so each page writes it in its reader's
/// locale.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum News {
    /// Nothing yet.
    #[default]
    None,
    /// Someone bid more: the price is now `cents`.
    Outbid { cents: u64 },
    /// The lot closed with their bid on top.
    Won { cents: u64 },
}

/// The latest news for one bidder, and the lot it is about. `seq` counts them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notice {
    pub seq: u64,
    pub news: News,
    pub lot: String,
    pub card: String,
}

/// The lots' standings and the bidders' notices, each in a `watch` channel: a page follows the
/// latest value, never a backlog. In memory, so one server process; across several, the store
/// would publish each change (Postgres `NOTIFY`, a message bus) and each process would send it
/// here. The store keeps the bids; this is what the pages show of them.
#[derive(Clone, Default)]
pub struct Room {
    lots: Arc<Mutex<HashMap<String, watch::Sender<Live>>>>,
    /// Counts every change to any lot: the lobby's list follows it.
    board: Arc<watch::Sender<u64>>,
    notices: Arc<Mutex<HashMap<String, watch::Sender<Notice>>>>,
    /// Woken when a lot's end moves (the house ended it early), so the closing task looks again.
    pub ends_moved: Arc<Notify>,
}

impl Room {
    /// Lot `id` as it stands now, if the room has it.
    pub fn lot(&self, id: &str) -> Option<Live> {
        self.lots.lock().expect("lots").get(id).map(|lot| lot.borrow().clone())
    }

    /// Every lot, the one that ends first first.
    pub fn all(&self) -> Vec<Arc<Standing>> {
        let mut all: Vec<_> = self.lots.lock().expect("lots").values().map(|lot| lot.borrow().standing.clone()).collect();
        all.sort_by(|a, b| (a.lot.ends_at, &a.lot.id).cmp(&(b.lot.ends_at, &b.lot.id)));
        all
    }

    /// Lot `id`'s changes, to follow, if the room has it.
    pub fn follow(&self, id: &str) -> Option<watch::Receiver<Live>> {
        self.lots.lock().expect("lots").get(id).map(watch::Sender::subscribe)
    }

    /// The lobby's version, to follow.
    pub fn board(&self) -> watch::Receiver<u64> {
        self.board.subscribe()
    }

    /// Sends a lot's new standing to everyone following it. Two bids' handlers can read their
    /// standings from the store in one order and get here in the other, so a standing with fewer
    /// bids than the room's is older and changes nothing (an end moved early keeps the count).
    pub fn update(&self, standing: Standing) {
        let mut lots = self.lots.lock().expect("lots");
        let standing = Arc::new(standing);
        match lots.get(&standing.lot.id) {
            Some(sender) => {
                let newer = sender.send_if_modified(|live| {
                    if standing.count < live.standing.count || *live.standing == *standing {
                        return false;
                    }
                    *live = Live { version: live.version + 1, standing };
                    true
                });
                if !newer {
                    return;
                }
            }
            None => {
                lots.insert(standing.lot.id.clone(), watch::channel(Live { version: 0, standing }).0);
            }
        }
        self.board.send_modify(|version| *version += 1);
    }

    /// The latest notice for `bidder`.
    pub fn notice(&self, bidder: &str) -> Notice {
        self.notices.lock().expect("notices").get(bidder).map(|n| n.borrow().clone()).unwrap_or_default()
    }

    /// `bidder`'s notices, to follow: the latest, then each new one.
    pub fn notices(&self, bidder: &str) -> watch::Receiver<Notice> {
        let mut notices = self.notices.lock().expect("notices");
        notices.entry(bidder.to_string()).or_insert_with(|| watch::channel(Notice::default()).0).subscribe()
    }

    /// Tells `bidder` `news` about `lot`, on every page they have open.
    pub fn tell(&self, bidder: &str, lot: &Lot, news: News) {
        let mut notices = self.notices.lock().expect("notices");
        let sender = notices.entry(bidder.to_string()).or_insert_with(|| watch::channel(Notice::default()).0);
        sender.send_modify(|notice| {
            notice.seq += 1;
            notice.news = news;
            notice.lot.clone_from(&lot.id);
            notice.card.clone_from(&lot.card);
        });
    }

    /// Lot `id` closed: its pages draw it again (closed now), though its standing is the same.
    pub fn closed(&self, id: &str) {
        let lots = self.lots.lock().expect("lots");
        if let Some(sender) = lots.get(id) {
            sender.send_modify(|live| live.version += 1);
            self.board.send_modify(|version| *version += 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;
    use crate::store::{Bid, Lot};

    fn standing(count: u64) -> Standing {
        let lot = Lot {
            id: "drake".into(),
            card: "Ashen Drake".into(),
            set: "Ember Reach".into(),
            condition: "Near Mint".into(),
            opening_cents: 1000,
            step_cents: 100,
            ends_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1),
        };
        let recent =
            (0..count).rev().map(|n| Bid { bidder: "Ada".into(), cents: 1000 + n * 100, at: SystemTime::UNIX_EPOCH }).collect();
        Standing { lot, recent, count }
    }

    #[test]
    fn an_older_standing_never_replaces_a_newer_one() {
        let room = Room::default();
        room.update(standing(0));
        room.update(standing(2));
        room.update(standing(1));
        let live = room.lot("drake").unwrap();
        assert_eq!((live.version, live.standing.count), (1, 2));
        // The same standing again is no change either: the pages already show it.
        room.update(standing(2));
        assert_eq!(room.lot("drake").unwrap().version, 1);
        assert_eq!(*room.board().borrow(), 2);
    }
}
