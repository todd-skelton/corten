//! Work that lives as long as the app ([`crate::App::task`]): started by [`crate::serve`], told to
//! stop when shutdown begins ([`crate::Abort::Shutdown`]) and waited for up to the app's grace.

use std::{
    future::Future,
    panic::AssertUnwindSafe,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use crate::{Abort, Signal};

type Start = Box<dyn FnOnce(Signal) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// A task given to [`crate::App::task`], not started yet.
pub(crate) struct Task {
    pub(crate) name: &'static str,
    pub(crate) start: Start,
}

/// The app's tasks once started, with the signal they share.
pub(crate) struct Running {
    signal: Signal,
    tasks: Vec<(&'static str, tokio::task::JoinHandle<()>)>,
}

impl Running {
    /// Starts every task; one that panics is logged with its name and ends there (restarting it
    /// is the app's choice).
    /// `signal` is the app's, which work its handlers finish shares ([`Finishing::signal`]).
    pub(crate) fn start(tasks: Vec<Task>, signal: Signal) -> Self {
        let tasks = tasks
            .into_iter()
            .map(|task| {
                let run = Logged { name: task.name, run: (task.start)(signal.clone()) };
                (task.name, tokio::spawn(run))
            })
            .collect();
        Running { signal, tasks }
    }

    /// Tells every task, and the work handlers finish, to stop ([`Abort::Shutdown`]).
    pub(crate) fn stop(&self) {
        self.signal.abort(Abort::Shutdown);
    }

    /// Waits for the tasks until `until`, then ends the ones still running and logs their names.
    pub(crate) async fn finish(self, until: tokio::time::Instant, grace: Duration) {
        self.stop();
        let mut late = Vec::new();
        for (name, mut task) in self.tasks {
            if tokio::time::timeout_at(until, &mut task).await.is_err() {
                task.abort();
                late.push(name);
            }
        }
        if !late.is_empty() {
            tracing::warn!("corten: ended tasks still running {grace:?} after shutdown began: {}", late.join(", "));
        }
    }
}

/// Work handlers finish whether or not they are dropped ([`Signal::finish`]): told to stop when
/// shutdown begins, by the signal the app's tasks share, and waited for after them, within the
/// same grace.
#[derive(Default)]
pub(crate) struct Finishing {
    stop: Signal,
    running: std::sync::atomic::AtomicUsize,
    idle: tokio::sync::Notify,
}

/// One piece of work [`Finishing`] counts, until it is dropped.
pub(crate) struct Counted(std::sync::Arc<Finishing>);

impl Finishing {
    /// Counts one piece of work until the guard is dropped.
    pub(crate) fn enter(self: &std::sync::Arc<Self>) -> Counted {
        self.running.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        Counted(self.clone())
    }

    /// Aborts with [`Abort::Shutdown`] when shutdown begins: the app's tasks get it too
    /// ([`Running::start`]).
    pub(crate) fn signal(&self) -> &Signal {
        &self.stop
    }

    /// Waits until no work is running, or `until`; work still running then is left to end with
    /// the process, and logged.
    pub(crate) async fn wait(&self, until: tokio::time::Instant, grace: Duration) {
        let idle = async {
            loop {
                let notified = self.idle.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.running.load(std::sync::atomic::Ordering::Acquire) == 0 {
                    return;
                }
                notified.await;
            }
        };
        if tokio::time::timeout_at(until, idle).await.is_err() {
            let left = self.running.load(std::sync::atomic::Ordering::Acquire);
            tracing::warn!("corten: {left} piece(s) of Signal::finish work still running {grace:?} after shutdown began");
        }
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        if self.0.running.fetch_sub(1, std::sync::atomic::Ordering::AcqRel) == 1 {
            self.0.idle.notify_waiters();
        }
    }
}

/// A task's future, which logs a panic with the task's name instead of tokio's anonymous one.
struct Logged {
    name: &'static str,
    run: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl Future for Logged {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let name = self.name;
        match std::panic::catch_unwind(AssertUnwindSafe(|| self.run.as_mut().poll(cx))) {
            Ok(polled) => polled,
            Err(panic) => {
                let message = panic_message(&*panic);
                tracing::error!("corten: task {name:?} panicked: {message}");
                Poll::Ready(())
            }
        }
    }
}

/// What a panic says, for the log.
pub(crate) fn panic_message(panic: &(dyn std::any::Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("a panic with no message")
}
