//! [`crate::App::header_timeout`]: how long a connection may wait for a request's head. Each
//! connection has a [`Watch`], one atomic: busy from when hyper hands over a request's head until
//! its answer's body is dropped, otherwise the time it went idle. One thread per server ticks a
//! coarse clock a few times a second and expires the connections idle past the timeout; the
//! connection's next read then fails and hyper closes it. hyper's own header timer was measured
//! first: about 900 instructions a keep-alive request with a coarse timer of Corten's (1,400 with
//! tokio's), since it sets up a sleep for every head and reads once more to start it. This costs
//! about 300 (callgrind, keep-alive requests over TCP), which Corten's accept loop saves against
//! `axum::serve`.

use axum::body::{Body, Bytes};
use futures_util::task::AtomicWaker;
use std::{
    future::Future,
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering::Relaxed},
        Arc, Mutex, Weak,
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// A connection between a request's head and the end of its answer.
const BUSY: u64 = u64::MAX;
/// A connection idle past the timeout.
const EXPIRED: u64 = u64::MAX - 1;

/// The time, in milliseconds since `base`, as of the last tick, and the connections to watch.
pub(crate) struct Clock {
    base: Instant,
    now: AtomicU64,
    timeout: u64,
    watches: Mutex<Vec<Weak<Watch>>>,
}

impl Clock {
    /// A clock that expires connections idle for `timeout`, to within a tick either way: a tenth
    /// of it, at most a second. It ticks on a thread of its own, which ends at the first tick after the clock's
    /// last [`Watch`] has dropped.
    pub fn start(timeout: Duration) -> Arc<Clock> {
        let tick = (timeout / 10).clamp(Duration::from_millis(10), Duration::from_secs(1));
        let clock = Arc::new(Clock {
            base: Instant::now(),
            now: AtomicU64::new(0),
            timeout: timeout.as_millis().try_into().unwrap_or(u64::MAX),
            watches: Mutex::new(Vec::new()),
        });
        let weak = Arc::downgrade(&clock);
        std::thread::Builder::new()
            .name("corten-clock".into())
            .spawn(move || loop {
                std::thread::sleep(tick);
                let Some(clock) = weak.upgrade() else { return };
                clock.tick();
            })
            .expect("corten: could not start the clock's thread");
        clock
    }

    /// A watch for a connection that has just opened: idle from now.
    pub fn watch(self: &Arc<Self>) -> Arc<Watch> {
        let watch = Arc::new(Watch {
            since: AtomicU64::new(self.now.load(Relaxed)),
            heads: AtomicU64::new(0),
            waker: AtomicWaker::new(),
            clock: self.clone(),
        });
        self.watches.lock().expect("never poisoned").push(Arc::downgrade(&watch));
        watch
    }

    /// Advances the time, expires the connections idle past the timeout and wakes them; forgets
    /// the closed ones.
    fn tick(&self) {
        let now = Instant::now().saturating_duration_since(self.base).as_millis() as u64;
        self.now.store(now, Relaxed);
        self.watches.lock().expect("never poisoned").retain(|watch| {
            let Some(watch) = watch.upgrade() else { return false };
            let since = watch.since.load(Relaxed);
            // A head that arrives meanwhile makes it busy, and the exchange fails.
            if since < EXPIRED
                && now.saturating_sub(since) >= self.timeout
                && watch.since.compare_exchange(since, EXPIRED, Relaxed, Relaxed).is_ok()
            {
                watch.waker.wake();
            }
            true
        });
    }
}

/// One connection's state for its [`Clock`].
pub(crate) struct Watch {
    /// [`BUSY`], [`EXPIRED`], or the time the connection went idle.
    since: AtomicU64,
    /// How many heads have arrived, so an answer finished after the next head arrived does not
    /// mark the connection idle. Only the connection's task writes it.
    heads: AtomicU64,
    waker: AtomicWaker,
    clock: Arc<Clock>,
}

impl Watch {
    /// A request's head has arrived. Also when the clock has just expired the connection: the
    /// head came in time for it to read.
    /// Returns the head's number, for [`Watch::idle`].
    pub fn busy(&self) -> u64 {
        self.since.store(BUSY, Relaxed);
        let head = self.heads.load(Relaxed) + 1;
        self.heads.store(head, Relaxed);
        head
    }

    /// The answer to `head` is done: idle from now, unless another head has arrived since.
    fn idle(&self, head: u64) {
        if self.heads.load(Relaxed) == head {
            self.since.store(self.clock.now.load(Relaxed), Relaxed);
        }
    }

    fn expired(&self) -> bool {
        self.since.load(Relaxed) == EXPIRED
    }
}

/// A connection's IO, whose reads fail once its [`Watch`] has expired.
pub(crate) struct Watched<I> {
    pub io: I,
    pub watch: Option<Arc<Watch>>,
}

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "corten: no request head within the header timeout")
}

impl<I: AsyncRead + Unpin> AsyncRead for Watched<I> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let Some(watch) = &this.watch else {
            return Pin::new(&mut this.io).poll_read(cx, buf);
        };
        if watch.expired() {
            return Poll::Ready(Err(timed_out()));
        }
        let read = Pin::new(&mut this.io).poll_read(cx, buf);
        if read.is_pending() && watch.since.load(Relaxed) != BUSY {
            watch.waker.register(cx.waker());
            // Expired between the first look and the registration: its wake may have been missed.
            if watch.expired() {
                return Poll::Ready(Err(timed_out()));
            }
        }
        read
    }
}

impl<I: AsyncWrite + Unpin> AsyncWrite for Watched<I> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().io).poll_write(cx, buf)
    }

    fn poll_write_vectored(self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[io::IoSlice<'_>]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().io).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_shutdown(cx)
    }
}

/// A request's answer, whose body marks its connection idle when hyper has done with it. A
/// switched connection (101) stays busy: it is the route's from then on.
pub(crate) struct Answer<F> {
    pub inner: F,
    /// The connection's watch and the head's number.
    pub watch: Option<(Arc<Watch>, u64)>,
}

impl<F: Future<Output = Result<axum::response::Response, std::convert::Infallible>> + Unpin> Future for Answer<F> {
    type Output = Result<axum::http::Response<Answered>, std::convert::Infallible>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let res = match std::task::ready!(Pin::new(&mut this.inner).poll(cx)) {
            Ok(res) => res,
            Err(never) => match never {},
        };
        let watch = this.watch.take().filter(|_| res.status() != axum::http::StatusCode::SWITCHING_PROTOCOLS);
        Poll::Ready(Ok(res.map(|body| Answered { body, watch })))
    }
}

/// An answer's body, for [`Answer`].
pub(crate) struct Answered {
    body: Body,
    watch: Option<(Arc<Watch>, u64)>,
}

impl http_body::Body for Answered {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<http_body::Frame<Bytes>, axum::Error>>> {
        Pin::new(&mut self.get_mut().body).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.body.size_hint()
    }
}

impl Drop for Answered {
    fn drop(&mut self) {
        if let Some((watch, head)) = &self.watch {
            watch.idle(*head);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connection_expires_only_while_idle_and_a_head_in_time_wins() {
        let clock = Clock::start(Duration::from_millis(40));
        let watch = clock.watch();
        let first = watch.busy();
        let second = watch.busy();
        std::thread::sleep(Duration::from_millis(150));
        watch.idle(first);
        assert!(!watch.expired(), "a busy connection does not expire, nor go idle on an earlier answer");
        watch.idle(second);
        let started = Instant::now();
        while !watch.expired() {
            assert!(started.elapsed() < Duration::from_secs(2), "an idle connection expires");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(started.elapsed() >= Duration::from_millis(25), "{:?}", started.elapsed());
        watch.busy();
        assert!(!watch.expired(), "a head that arrives as it expires is read");
    }
}
