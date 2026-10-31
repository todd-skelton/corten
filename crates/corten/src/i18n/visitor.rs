//! The visitor's time zone (`zones` feature, [`crate::App::client_zone`]): [`Visitor`], and the
//! zone dates are written in while Corten renders a request's answer, so a handler takes no zone
//! and a component is given none.

use std::time::SystemTime;

#[cfg(feature = "zones")]
pub use zones::Visitor;
#[cfg(feature = "zones")]
pub(crate) use zones::{Scope, ZoneScope};

/// Without the `zones` feature there is no zone to keep: `Option<Scope>` is always `None`, and
/// takes no space in a stream.
#[cfg(not(feature = "zones"))]
pub(crate) enum Scope {}

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
    use std::marker::PhantomData;
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

    /// The zone an answer is written in: the name the browser reported, looked up in the tz
    /// database at the first date written (a render with no date looks nothing up), and kept.
    pub(crate) enum Scope {
        Named(crate::client::ZoneName, crate::Signal),
        Known(Visitor, crate::Signal),
        Unknown,
    }

    impl Scope {
        /// `name`: the zone the browser reported, if any. Writing a date in a known zone makes the
        /// answer private (`signal`), as reading [`crate::ClientZone`] does.
        pub(crate) fn of(name: Option<crate::client::ZoneName>, signal: &crate::Signal) -> Self {
            match name {
                Some(name) => Scope::Named(name, signal.clone()),
                None => Scope::Unknown,
            }
        }

        fn resolved(&mut self) -> &Self {
            if let Scope::Named(name, signal) = self {
                let zone = Visitor::named(name.as_str());
                *self = if zone.is_known() { Scope::Known(zone, signal.clone()) } else { Scope::Unknown };
            }
            self
        }
    }

    std::thread_local! {
        static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
    }

    /// While it lives, dates with no zone of their own are written in the request's zone; the one
    /// before is put back when it drops (a render inside a render). Not `Send`: it is entered
    /// and left on one thread, never held across an `.await`.
    pub(crate) struct ZoneScope(Option<Scope>, PhantomData<*const ()>);

    impl ZoneScope {
        pub(crate) fn enter(scope: Scope) -> Self {
            ZoneScope(SCOPE.with(|s| s.replace(Some(scope))), PhantomData)
        }

        /// Leaves it, giving back its scope as it is now (the zone looked up, if a date was
        /// written), for the next time (a stream's next poll).
        pub(crate) fn leave(mut self) -> Scope {
            let before = self.0.take();
            let now = SCOPE.with(|s| s.replace(before));
            std::mem::forget(self);
            now.unwrap_or(Scope::Unknown)
        }
    }

    impl Drop for ZoneScope {
        fn drop(&mut self) {
            let before = self.0.take();
            SCOPE.with(|s| *s.borrow_mut() = before);
        }
    }

    pub(super) fn offset(at: SystemTime) -> i32 {
        SCOPE.with(|s| match s.borrow_mut().as_mut().map(Scope::resolved) {
            Some(Scope::Known(zone, signal)) => {
                signal.mark_personal();
                zone.offset(at)
            }
            _ => 0,
        })
    }

    pub(super) fn unknown() -> bool {
        SCOPE.with(|s| matches!(s.borrow_mut().as_mut().map(Scope::resolved), Some(Scope::Unknown)))
    }
}
