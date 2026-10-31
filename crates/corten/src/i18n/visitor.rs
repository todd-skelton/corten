//! The visitor's time zone (`zones` feature, [`crate::App::client_zone`]): [`Visitor`], and the
//! zone dates are written in while Corten renders a request's answer, so a handler takes no zone
//! and a component is given none.

use std::time::SystemTime;

#[cfg(feature = "zones")]
pub use zones::{Visitor, ZoneScope};

/// The offset a date with no zone of its own is written at: the request's zone's while Corten
/// renders its answer, else UTC.
#[cfg(feature = "zones")]
#[inline]
pub(super) fn offset(at: SystemTime) -> i32 {
    zones::offset(at)
}

#[cfg(not(feature = "zones"))]
#[inline]
pub(super) fn offset(_: SystemTime) -> i32 {
    0
}

/// Whether the answer being rendered is for a visitor whose zone is not known yet.
#[cfg(feature = "zones")]
#[inline]
pub(super) fn unknown() -> bool {
    zones::unknown()
}

#[cfg(not(feature = "zones"))]
#[inline]
pub(super) fn unknown() -> bool {
    false
}

#[cfg(feature = "zones")]
mod zones {
    use std::cell::RefCell;
    use std::time::SystemTime;

    use super::super::Zone;

    /// The visitor's time zone, with its rules: what [`crate::Req::zone`] gives, from the name
    /// the browser reports ([`crate::ClientZone`]) and the tz database Corten bundles (jiff's). UTC
    /// when the name is not known (no zone cookie yet, a name the database lacks).
    ///
    /// ```
    /// use corten::i18n::{Visitor, Zone};
    /// use std::time::{Duration, UNIX_EPOCH};
    ///
    /// let chicago = Visitor::named("America/Chicago");
    /// let july = UNIX_EPOCH + Duration::from_secs(1_783_000_000); // 2026-07
    /// let january = UNIX_EPOCH + Duration::from_secs(1_768_000_000); // 2026-01
    /// assert_eq!((chicago.offset(july), chicago.offset(january)), (-5 * 3600, -6 * 3600));
    /// assert!(!Visitor::named("Mars/Olympus").is_known());
    /// assert_eq!(Visitor::named("Mars/Olympus").offset(july), 0);
    /// ```
    #[derive(Clone, Debug, Default)]
    pub struct Visitor(Option<jiff::tz::TimeZone>);

    impl Visitor {
        /// The zone an IANA name (`America/Chicago`) names; UTC when the database lacks it.
        pub fn named(name: &str) -> Self {
            Visitor(jiff::tz::TimeZone::get(name).ok())
        }

        /// Whether it is a zone the database knows, not the UTC it falls back to.
        pub fn is_known(&self) -> bool {
            self.0.is_some()
        }
    }

    impl Zone for Visitor {
        fn offset(&self, at: SystemTime) -> i32 {
            let (Some(zone), Ok(at)) = (&self.0, jiff::Timestamp::try_from(at)) else { return 0 };
            zone.to_offset(at).seconds()
        }
    }

    /// The zone of the answer being rendered on this thread.
    enum Scope {
        Known(Visitor, crate::Signal),
        Unknown,
    }

    std::thread_local! {
        static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
    }

    /// While it lives, dates with no zone of their own are written in the request's zone; the one
    /// before is put back when it drops (a render inside a render).
    pub struct ZoneScope(Option<Scope>);

    impl ZoneScope {
        /// `name`: the zone the browser reported, if any. Writing a date in a known zone makes the
        /// answer private (`signal`), as reading [`crate::ClientZone`] does.
        pub(crate) fn enter(name: Option<&str>, signal: &crate::Signal) -> Self {
            let zone = name.map(Visitor::named).filter(Visitor::is_known);
            let scope = match zone {
                Some(zone) => Scope::Known(zone, signal.clone()),
                None => Scope::Unknown,
            };
            ZoneScope(SCOPE.with(|s| s.replace(Some(scope))))
        }
    }

    impl Drop for ZoneScope {
        fn drop(&mut self) {
            let before = self.0.take();
            SCOPE.with(|s| *s.borrow_mut() = before);
        }
    }

    pub(super) fn offset(at: SystemTime) -> i32 {
        SCOPE.with(|s| match &*s.borrow() {
            Some(Scope::Known(zone, signal)) => {
                signal.mark_personal();
                zone.offset(at)
            }
            _ => 0,
        })
    }

    pub(super) fn unknown() -> bool {
        SCOPE.with(|s| matches!(&*s.borrow(), Some(Scope::Unknown)))
    }
}
