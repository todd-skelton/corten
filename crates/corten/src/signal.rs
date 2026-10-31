//! [`Signal`]: when the work for a request should stop, like the browser's `AbortSignal`.
//!
//! Work a handler awaits needs none of this to stop: when the request ends early (the client went
//! away, or the deadline in [`crate::App::deadline`] passed) Rust drops the handler's future and
//! everything it was awaiting. The signal is for work that drop cannot reach: tasks the handler
//! spawned, a blocking thread, a query running on another server, a loop over a long stream. A drop
//! also cannot run async cleanup, so work that must end a certain way (release a lease it claimed)
//! goes in [`Signal::finish`].
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
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
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

/// Begins the shutdown of the task whose signal [`Signal::task`] made with it: what `serve` does
/// to a task's signal on Ctrl-C or `SIGTERM`, for a test. It aborts that one signal; the app's
/// own shutdown is [`crate::shutdown`].
///
/// Only a signal made with it can be aborted: a request's signal, or one a test has, cannot.
///
/// ```compile_fail,E0423
/// fn stop(signal: corten::Signal) {
///     corten::Shutdown(signal).begin();
/// }
/// ```
///
/// ```compile_fail,E0624
/// fn stop(signal: corten::Signal) {
///     signal.abort(corten::Abort::Shutdown);
/// }
/// ```
pub struct Shutdown(Signal);

impl Shutdown {
    /// Aborts the task's signal with [`Abort::Shutdown`].
    pub fn begin(self) {
        self.0.abort(Abort::Shutdown);
    }
}

/// Why a request's [`Signal`] aborted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abort {
    /// The whole response was sent (for a stream, its last event). Work started for the request
    /// ends with it, so a spawned task cannot outlive the request by accident; work meant to
    /// outlive it is a task of the app's ([`crate::App::task`]), which has a signal of its own.
    /// Work the handler must finish ([`Signal::finish`]) has a signal of its own, which this reason
    /// reaches only if the handler stops awaiting the work (a timeout around it) and answers first.
    Responded,
    /// The client went away before the response was sent: it closed the connection, cancelled the
    /// fetch, or left a stream.
    Disconnected,
    /// The handler ran past the app's deadline ([`crate::App::deadline`]) and the client got 503.
    Deadline,
    /// The app is shutting down: the reason a task's signal aborts ([`crate::App::task`]), and the
    /// signal of work a handler finishes ([`Signal::finish`]); a request's never does.
    Shutdown,
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
    /// Something read who is asking ([`crate::Caller`]), so the answer is theirs alone.
    personal: AtomicBool,
    /// The hashes of the callers it read, mixed: part of the page's key, so a page for someone
    /// else (after signing in or out) is loaded in full.
    callers: AtomicU64,
    /// The app's error page is answering it.
    error_page: AtomicBool,
    /// Its answer is `Public`, the same for everyone: written as for a visitor whose zone is not
    /// known, its live part's fragments too.
    #[cfg(feature = "zones")]
    for_everyone: AtomicBool,
    /// With [`crate::App::client_zone`]: the zone the browser reported, read from its cookie once
    /// for the request, for its page and its live part's fragments alike.
    #[cfg(feature = "zones")]
    zone: Option<crate::client::ZoneName>,
    /// The page rendered the request's notice ([`crate::Req::notice`]), so a navigation's answer
    /// does not send it to the runtime as well.
    notice_drawn: AtomicBool,
    /// The first callers marked, so the error page, which looks them up again, does not mark one
    /// twice (the key mixes them by XOR, so a second mark would take it out).
    marked: [AtomicU64; 4],
    /// The app's, for a request it serves: carried here rather than as a request extension of
    /// its own, so the request's extensions stay within their first allocation.
    shared: Option<Arc<crate::page::Shared>>,
}

impl Signal {
    /// A request's signal in an app.
    pub(crate) fn for_app(shared: Arc<crate::page::Shared>) -> Signal {
        Signal(Arc::new(Inner { shared: Some(shared), ..Inner::default() }))
    }

    /// A request's signal in an app that learns its visitors' zones, with the one the browser
    /// reported.
    #[cfg(feature = "zones")]
    pub(crate) fn for_app_in(shared: Arc<crate::page::Shared>, zone: Option<crate::client::ZoneName>) -> Signal {
        Signal(Arc::new(Inner { shared: Some(shared), zone, ..Inner::default() }))
    }

    /// A task's signal and the handle that begins its shutdown, for a test of an app's task: the
    /// signal [`crate::App::task`] gives aborts with [`Abort::Shutdown`] when `serve` begins to stop,
    /// and this does the same when the test calls [`Shutdown::begin`], so the task's shutdown path
    /// (a job released, a loop that returns at once) is tested as it runs. `app.router()` starts no
    /// tasks, so a test runs the task's function itself. Only a signal made here can be aborted: a
    /// request's is the app's alone.
    ///
    /// ```
    /// use corten::{Abort, Signal};
    /// use std::time::Duration;
    ///
    /// /// Polls every 10 s; a poll is a step that takes 6 s and stops at the signal.
    /// async fn poll(signal: Signal) -> &'static str {
    ///     loop {
    ///         tokio::select! {
    ///             biased;
    ///             _ = signal.aborted() => return "stopped while idle",
    ///             _ = tokio::time::sleep(Duration::from_secs(10)) => {}
    ///         }
    ///         tokio::select! {
    ///             biased;
    ///             _ = signal.aborted() => return "stopped mid-step",
    ///             _ = tokio::time::sleep(Duration::from_secs(6)) => {}
    ///         }
    ///     }
    /// }
    ///
    /// # tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(async {
    /// let (signal, shutdown) = Signal::task();
    /// let task = tokio::spawn(poll(signal.clone()));
    /// tokio::time::sleep(Duration::from_secs(12)).await; // 2 s into the first step
    /// let at = tokio::time::Instant::now();
    /// shutdown.begin();
    /// assert_eq!(task.await.unwrap(), "stopped mid-step");
    /// assert_eq!((at.elapsed(), signal.reason()), (Duration::ZERO, Some(Abort::Shutdown)));
    /// # });
    /// ```
    pub fn task() -> (Signal, Shutdown) {
        let signal = Signal::default();
        (signal.clone(), Shutdown(signal))
    }

    /// The app's shared state, for a request an app serves.
    pub(crate) fn shared(&self) -> Option<&Arc<crate::page::Shared>> {
        self.0.shared.as_ref()
    }

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
            4 => Some(Abort::Shutdown),
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

    /// Runs `work` to its end even if the handler is dropped, and waits for it: what a handler
    /// claimed (a lease, a lock in another service, a reservation) is released when the client
    /// leaves or the deadline passes, which a drop cannot do. `work` gets a signal of its own that
    /// aborts when this one does (the client left, the deadline passed) or shutdown begins, so it
    /// can stop a slow step early and still run what follows it; [`crate::serve`] waits for it at
    /// shutdown (below).
    ///
    /// ```
    /// use corten::{prelude::*, Signal};
    ///
    /// # struct Gate;
    /// # impl Gate {
    /// #     async fn claim(&self) -> u32 { 7 }
    /// #     async fn release(&self, _lease: u32, _outcome: &str) {}
    /// # }
    /// # async fn call_provider(_lease: u32) -> &'static str { "connected" }
    /// # static GATE: Gate = Gate;
    /// async fn check(signal: Signal) -> String {
    ///     signal
    ///         .finish(|signal| async move {
    ///             let lease = GATE.claim().await;
    ///             // The slow call stops when the client leaves; the release after it still runs.
    ///             let outcome = tokio::select! {
    ///                 outcome = call_provider(lease) => outcome,
    ///                 _ = signal.aborted() => "cancelled",
    ///             };
    ///             GATE.release(lease, outcome).await;
    ///             outcome.to_string()
    ///         })
    ///         .await
    /// }
    /// # let _: Methods = get(check);
    /// ```
    ///
    /// The work runs on a task of its own, so it is `Send + 'static`: it takes what it uses by
    /// value. A panic in it is the handler's while the handler awaits it, and is logged once the
    /// handler is gone. Keep it to the claim, the work and the release, and let each slow step end
    /// on the signal: while its client is still there, the handler awaits the work, and shutdown
    /// waits for the handler up to the [deadline](crate::App::deadline) (none with
    /// `deadline(None)`); once the handler is gone, shutdown waits for the work up to
    /// [`crate::App::shutdown_grace`] and then leaves it to end with the process. Served another
    /// way than [`crate::serve`] (the router passed to `axum::serve`), the work still runs to its
    /// end and sees the client leave and the deadline, but is not told of shutdown or waited for.
    pub async fn finish<F, Fut, T>(&self, work: F) -> T
    where
        F: FnOnce(Signal) -> Fut,
        Fut: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let finishing = self.shared().map(|shared| shared.finishing.clone());
        let counted = finishing.as_ref().map(|finishing| finishing.enter());
        let own = Signal::default();
        let run = work(own.clone());
        let (request, shutdown) = (self.clone(), finishing.map(|f| f.signal().clone()));
        // Held while the handler awaits the work: a panic after it is gone is logged here.
        let awaiting = std::sync::Arc::new(());
        let handler = std::sync::Arc::downgrade(&awaiting);
        let task = tokio::spawn(async move {
            let _counted = counted;
            // Passes on the request's abort, or shutdown's, then waits for `run` to end.
            let forward = async {
                let reason = match &shutdown {
                    Some(shutdown) => tokio::select! {
                        reason = request.aborted() => reason,
                        reason = shutdown.aborted() => reason,
                    },
                    None => request.aborted().await,
                };
                own.abort(reason);
                std::future::pending::<std::convert::Infallible>().await
            };
            let mut done = std::pin::pin!(async {
                tokio::select! {
                    biased;
                    out = run => out,
                    never = forward => match never {},
                }
            });
            std::future::poll_fn(|cx| {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| std::future::Future::poll(done.as_mut(), cx))) {
                    Ok(polled) => polled,
                    Err(panic) => {
                        if handler.upgrade().is_none() {
                            let message = crate::task::panic_message(&*panic);
                            tracing::error!("corten: Signal::finish work whose handler is gone panicked: {message}");
                        }
                        std::panic::resume_unwind(panic)
                    }
                }
            })
            .await
        });
        let out = task.await;
        drop(awaiting);
        match out {
            Ok(out) => out,
            Err(error) => match error.try_into_panic() {
                Ok(panic) => std::panic::resume_unwind(panic),
                Err(_) => panic!("corten: Signal::finish's work was cancelled: the runtime is shutting down"),
            },
        }
    }

    /// Marks the request as read by who is asking, whose hash is `hash` (each caller type is
    /// marked once per request).
    pub(crate) fn mark_caller(&self, hash: u64) {
        let marked = &self.0.marked;
        if self.is_error_page() && marked.iter().any(|m| m.load(Ordering::Relaxed) == hash) {
            return;
        }
        if let Some(free) = marked.iter().find(|m| m.load(Ordering::Relaxed) == 0) {
            free.store(hash, Ordering::Relaxed);
        }
        self.0.personal.store(true, Ordering::Relaxed);
        self.0.callers.fetch_xor(hash, Ordering::Relaxed);
    }

    /// Marks the answer as differing by something of the visitor's that does not change the page
    /// around `<main>` (their time zone): private, but not part of the page's key.
    pub(crate) fn mark_personal(&self) {
        self.0.personal.store(true, Ordering::Relaxed);
    }

    /// The request's answer is `Public` (its route says so when the crate compiles).
    #[cfg(feature = "zones")]
    pub(crate) fn mark_for_everyone(&self) {
        self.0.for_everyone.store(true, Ordering::Relaxed);
    }

    /// True when the request's answer is `Public`.
    #[cfg(feature = "zones")]
    pub(crate) fn is_for_everyone(&self) -> bool {
        self.0.for_everyone.load(Ordering::Relaxed)
    }

    /// The zone the request's answer is written in: the browser's, but none for a `Public`
    /// answer, the same for everyone (its error page, which is not, has it).
    #[cfg(feature = "zones")]
    pub(crate) fn zone(&self) -> Option<crate::client::ZoneName> {
        self.0.zone.filter(|_| !self.is_for_everyone() || self.is_error_page())
    }

    /// The page drew the request's notice.
    pub(crate) fn mark_notice_drawn(&self) {
        self.0.notice_drawn.store(true, Ordering::Relaxed);
    }

    pub(crate) fn notice_drawn(&self) -> bool {
        self.0.notice_drawn.load(Ordering::Relaxed)
    }

    /// The app's error page is answering the request.
    pub(crate) fn mark_error_page(&self) {
        self.0.error_page.store(true, Ordering::Relaxed);
    }

    /// True while the app's error page answers the request: an optional caller that cannot be
    /// looked up is none there.
    pub(crate) fn is_error_page(&self) -> bool {
        self.0.error_page.load(Ordering::Relaxed)
    }

    /// True when something read who is asking.
    pub(crate) fn is_personal(&self) -> bool {
        self.0.personal.load(Ordering::Relaxed)
    }

    /// The callers read so far, as one hash (0 for none).
    pub(crate) fn callers(&self) -> u64 {
        self.0.callers.load(Ordering::Relaxed)
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
///
/// `zoned`: the app writes dates in its visitors' zones (`App::zoned_dates`, `zones` feature), so
/// a stream's fragments (a live part, rendered as the stream is read) are written in the
/// request's, as the page was.
pub(crate) fn follow(res: Response, watch: Watch, #[cfg_attr(not(feature = "zones"), allow(unused))] zoned: bool) -> Response {
    if res.body().size_hint().exact().is_some() {
        watch.0.abort(Abort::Responded);
        return res;
    }
    #[cfg(feature = "zones")]
    let zone = zoned.then(|| crate::i18n::Scope::of(watch.0.zone(), &watch.0));
    #[cfg(not(feature = "zones"))]
    let zone = None;
    let (parts, body) = res.into_parts();
    Response::from_parts(parts, Body::new(Followed { body, watch, zone }))
}

struct Followed {
    body: Body,
    watch: Watch,
    /// With `App::zoned_dates`: the zone the stream's fragments are written in, kept
    /// between polls once it has been looked up.
    #[cfg_attr(not(feature = "zones"), allow(dead_code))]
    zone: Option<crate::i18n::Scope>,
}

impl HttpBody for Followed {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, axum::Error>>> {
        // Inside this poll only: the fragments the stream renders as it is read.
        #[cfg(feature = "zones")]
        let entered = self.zone.take().map(crate::i18n::ZoneScope::enter);
        let polled = Pin::new(&mut self.body).poll_frame(cx);
        #[cfg(feature = "zones")]
        if let Some(entered) = entered {
            self.zone = Some(entered.leave());
        }
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
        drop(follow(Response::new(Body::from("hello")), Watch(page.clone()), false));
        assert_eq!(page.reason(), Some(Abort::Responded), "a body in memory is the whole response");

        let sent = Signal::default();
        let stream = futures_util::stream::iter([Ok::<_, std::io::Error>(Bytes::from("a")), Ok(Bytes::from("b"))]);
        let res = follow(Response::new(Body::from_stream(stream)), Watch(sent.clone()), false);
        assert_eq!(sent.reason(), None, "a stream is running until its last frame");
        assert_eq!(res.into_body().collect().await.unwrap().to_bytes(), "ab");
        assert_eq!(sent.reason(), Some(Abort::Responded));

        let left = Signal::default();
        let stream = futures_util::stream::pending::<Result<Bytes, std::io::Error>>();
        drop(follow(Response::new(Body::from_stream(stream)), Watch(left.clone()), false));
        assert_eq!(left.reason(), Some(Abort::Disconnected));
    }
}
