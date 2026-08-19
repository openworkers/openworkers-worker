//! The guest executor.
//!
//! `wasi:http/incoming-handler.handle` is a synchronous export, so an async
//! handler has to be driven to completion inside it. Every host call at WASI
//! 0.2 is also available in blocking form (`pollable.block`,
//! `blocking-read`), so a guest future never parks waiting for a host
//! wakeup: a poll loop over a no-op waker is enough, and it keeps the SDK
//! free of an executor dependency. `futures::executor::block_on` would park
//! the thread instead, and wasm32-wasip2 has no second thread to unpark it.
//!
//! Spawned tasks exist so that the `futures_channel::oneshot` bridge that
//! workers-rs applications use to get `Send` futures keeps working: the
//! sender runs here, and re-polling the receiver picks up the value.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

type Task = Pin<Box<dyn Future<Output = ()>>>;

thread_local! {
    static TASKS: RefCell<Vec<Task>> = const { RefCell::new(Vec::new()) };
}

/// A task that never finishes would otherwise spin until the host's CPU
/// limit fires; stopping first turns it into a message.
const MAX_IDLE_ROUNDS: u32 = 100_000;

const STALLED: &str = "worker future stalled: it is waiting on something no \
                       host call can complete (WASI 0.2 has no guest reactor)";

/// Queues a future to run alongside the handler. It is polled between polls
/// of the handler and drained before the export returns.
pub fn spawn_local<F: Future<Output = ()> + 'static>(future: F) {
    TASKS.with(|tasks| tasks.borrow_mut().push(Box::pin(future)));
}

/// Drives `future` to completion, polling queued tasks in between.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut future = Box::pin(future);
    let mut idle = 0;

    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }

        if !poll_tasks(&mut cx) {
            panic!("{STALLED}");
        }

        idle += 1;

        if idle > MAX_IDLE_ROUNDS {
            panic!("{STALLED}");
        }
    }
}

/// Drives a future that only ever waits on values already in memory, without
/// touching the task queue. `None` means it parked, which nothing can undo.
pub fn poll_to_end<F: Future>(future: F) -> Option<F::Output> {
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut future = Box::pin(future);

    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

/// Runs whatever `wait_until` and `spawn_local` left behind.
pub fn drain_tasks() {
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut idle = 0;

    while poll_tasks(&mut cx) {
        idle += 1;

        if idle > MAX_IDLE_ROUNDS {
            crate::console_error!("{STALLED}");
            return;
        }
    }
}

/// Polls every queued task once and drops the finished ones. Returns whether
/// any task is still queued, which is the only way progress can still happen.
fn poll_tasks(cx: &mut Context<'_>) -> bool {
    let mut queued = TASKS.with(|tasks| std::mem::take(&mut *tasks.borrow_mut()));

    queued.retain_mut(|task| task.as_mut().poll(cx).is_pending());

    TASKS.with(|tasks| {
        let mut tasks = tasks.borrow_mut();
        // A task may have spawned another while it was polled; keep both.
        queued.append(&mut tasks);
        *tasks = queued;
        !tasks.is_empty()
    })
}
