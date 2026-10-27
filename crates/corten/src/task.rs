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
    pub(crate) fn start(tasks: Vec<Task>) -> Self {
        let signal = Signal::default();
        let tasks = tasks
            .into_iter()
            .map(|task| {
                let run = Logged { name: task.name, run: (task.start)(signal.clone()) };
                (task.name, tokio::spawn(run))
            })
            .collect();
        Running { signal, tasks }
    }

    /// Tells every task to stop ([`Abort::Shutdown`]).
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
                let message = panic
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("a panic with no message");
                tracing::error!("corten: task {name:?} panicked: {message}");
                Poll::Ready(())
            }
        }
    }
}
