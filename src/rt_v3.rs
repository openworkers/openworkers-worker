//! The 0.3 counterpart of `rt`: the component-model async ABI brings a real
//! executor (wit-bindgen's), so the fetch path needs no poll loop. What is
//! left to manage is `wait_until` work, which must outlive the handler but
//! finish before the guest resolves its response trailers - that resolution
//! is the host's signal that the invocation is done.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;

type Task = Pin<Box<dyn Future<Output = ()>>>;

thread_local! {
    static WAIT_UNTIL: RefCell<Vec<Task>> = const { RefCell::new(Vec::new()) };
}

/// Queues work to finish after the response, before the invocation ends.
pub fn wait_until<F: Future<Output = ()> + 'static>(future: F) {
    WAIT_UNTIL.with(|tasks| tasks.borrow_mut().push(Box::pin(future)));
}

/// Runs queued `wait_until` work to completion, including work queued by the
/// tasks themselves.
pub async fn drain_wait_until() {
    loop {
        let tasks = WAIT_UNTIL.with(|tasks| std::mem::take(&mut *tasks.borrow_mut()));

        if tasks.is_empty() {
            return;
        }

        futures_util::future::join_all(tasks).await;
    }
}
