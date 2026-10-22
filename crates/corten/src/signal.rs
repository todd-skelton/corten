//! [`Signal`]: when the work for a request should stop, like the browser's `AbortSignal`.
//!
//! Work a handler awaits needs none of this: when the request ends early (the client went away,
//! or the deadline in [`crate::App::deadline`] passed) Rust drops the handler's future and
//! everything it was awaiting. The signal is for work that drop cannot reach: tasks the handler
//! spawned, a blocking thread, a query running on another server, a loop over a long stream.
//!
//! ```
//! use corten::Signal;
//!
//! async fn search(signal: Signal) -> String {
//!     // A blocking thread is not dropped with the handler, so it checks the signal.
//!     let scan = tokio::task::spawn_blocking(move || {
//!         for _shard in 0..64 {
//!             if signal.is_aborted() {
//!                 return None;
//!             }
//!             // ... scan one shard
//!         }
//!         Some("found".to_string())
//!     });
//!     scan.await.ok().flatten().unwrap_or_default()
//! }
//! ```

use std::{
    convert::Infallible,
    pin::{pin, Pin},
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use axum::{
    body::{Body, Bytes, HttpBody},
    extract::FromRequestParts,
    http::request::Parts,
    response::Response,
};
use http_body::{Frame, SizeHint};
use tokio::sync::Notify;

/// Why a request's [`Signal`] aborted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abort {
    /// The whole response was sent (for a stream, its last event). Work started for the request
    /// ends with it, so a spawned task cannot outlive the request by accident; work meant to
    /// outlive it does not take the signal.
    Responded,
    /// The client went away before the response was sent: it closed the connection, cancelled the
    /// fetch, or left a stream.
    Disconnected,
    /// The handler ran past the app's deadline ([`crate::App::deadline`]) and the client got 503.
    Deadline,
}

/// Aborts once, when the request's work should stop, and says why ([`Abort`]). Cloning is cheap
/// (one shared allocation per request). Take it as an extractor, `signal: Signal`, or from
/// [`crate::Req::signal`]. Outside an [`crate::App`] (a bare router in a test) it never aborts.
#[derive(Clone, Default)]
pub struct Signal(Arc<Inner>);

#[derive(Default)]
struct Inner {
    /// 0 while running, else an [`Abort`] + 1.
    reason: AtomicU8,
    notify: Notify,
}

impl Signal {
    /// Whether the signal has aborted: a cheap check for a loop on a blocking thread.
    pub fn is_aborted(&self) -> bool {
        self.0.reason.load(Ordering::Acquire) != 0
    }

    /// Why it aborted, or `None` while the request is running.
    pub fn reason(&self) -> Option<Abort> {
        match self.0.reason.load(Ordering::Acquire) {
            1 => Some(Abort::Responded),
            2 => Some(Abort::Disconnected),
            3 => Some(Abort::Deadline),
            _ => None,
        }
    }

    /// Resolves when the signal aborts (at once if it has), with the reason.
    pub async fn aborted(&self) -> Abort {
        let mut notified = pin!(self.0.notify.notified());
        // Registered before the check, so an abort between the two still wakes this future.
        notified.as_mut().enable();
        if let Some(reason) = self.reason() {
            return reason;
        }
        notified.await;
        self.reason().unwrap_or(Abort::Responded)
    }

    /// Aborts with `reason` unless it already aborted; the first reason stays.
    pub(crate) fn abort(&self, reason: Abort) {
        let code = reason as u8 + 1;
        if self.0.reason.compare_exchange(0, code, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            self.0.notify.notify_waiters();
        }
    }
}

impl std::fmt::Debug for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signal").field("reason", &self.reason()).finish()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Signal {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(parts.extensions.get::<Signal>().cloned().unwrap_or_default())
    }
}

/// Held while the request runs, first by the middleware's future and then by the response body.
/// Dropped before the body ended (the handler's future or the body was dropped: the client went
/// away), it aborts with [`Abort::Disconnected`].
pub(crate) struct Watch(pub(crate) Signal);

impl Drop for Watch {
    fn drop(&mut self) {
        self.0.abort(Abort::Disconnected);
    }
}

/// Ends `watch` with the response. A body of known length (every page, fragment and JSON answer,
/// which are in memory) counts as sent, so the signal aborts with [`Abort::Responded`] now, without a
/// wrapper. A stream takes `watch` along: [`Abort::Responded`] after its last frame, or
/// [`Abort::Disconnected`] if it is dropped before (the client left).
pub(crate) fn follow(res: Response, watch: Watch) -> Response {
    if res.body().size_hint().exact().is_some() {
        watch.0.abort(Abort::Responded);
        return res;
    }
    let (parts, body) = res.into_parts();
    Response::from_parts(parts, Body::new(Followed { body, watch }))
}

struct Followed {
    body: Body,
    watch: Watch,
}

impl HttpBody for Followed {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, axum::Error>>> {
        let polled = Pin::new(&mut self.body).poll_frame(cx);
        if let Poll::Ready(None) = polled {
            self.watch.0.abort(Abort::Responded);
        }
        polled
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_first_reason_wins_and_wakes_every_waiter() {
        let signal = Signal::default();
        assert_eq!(signal.reason(), None);
        let (a, b) = (signal.clone(), signal.clone());
        let waiters = tokio::spawn(async move { (a.aborted().await, b.aborted().await) });
        tokio::task::yield_now().await;
        signal.abort(Abort::Deadline);
        signal.abort(Abort::Responded);
        assert_eq!(waiters.await.unwrap(), (Abort::Deadline, Abort::Deadline));
        assert!(signal.is_aborted());
        assert_eq!(signal.aborted().await, Abort::Deadline);
    }

    #[tokio::test]
    async fn dropping_the_watch_means_the_client_left() {
        let signal = Signal::default();
        drop(Watch(signal.clone()));
        assert_eq!(signal.reason(), Some(Abort::Disconnected));
    }

    #[tokio::test]
    async fn a_body_read_to_its_end_was_sent_and_one_dropped_early_was_not() {
        use http_body_util::BodyExt;
        let page = Signal::default();
        drop(follow(Response::new(Body::from("hello")), Watch(page.clone())));
        assert_eq!(page.reason(), Some(Abort::Responded), "a body in memory is the whole response");

        let sent = Signal::default();
        let stream = futures_util::stream::iter([Ok::<_, std::io::Error>(Bytes::from("a")), Ok(Bytes::from("b"))]);
        let res = follow(Response::new(Body::from_stream(stream)), Watch(sent.clone()));
        assert_eq!(sent.reason(), None, "a stream is running until its last frame");
        assert_eq!(res.into_body().collect().await.unwrap().to_bytes(), "ab");
        assert_eq!(sent.reason(), Some(Abort::Responded));

        let left = Signal::default();
        let stream = futures_util::stream::pending::<Result<Bytes, std::io::Error>>();
        drop(follow(Response::new(Body::from_stream(stream)), Watch(left.clone())));
        assert_eq!(left.reason(), Some(Abort::Disconnected));
    }
}
