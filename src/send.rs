//! The `Send` wrappers workers-rs needs to hand JS-backed values to
//! frameworks that want `Send`. There is nothing JS-backed here and the
//! guest has one thread, so these only assert what is already true.

use std::future::Future;
use std::ops::Deref;
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Marks a future as `Send`.
pub struct SendFuture<F>(F);

impl<F> SendFuture<F> {
    pub fn new(inner: F) -> Self {
        SendFuture(inner)
    }
}

// SAFETY: a wasm32-wasip2 guest has a single thread, so nothing here can be
// observed from another one.
unsafe impl<F> Send for SendFuture<F> {}

impl<F: Future> Future for SendFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: the projection is structural and `SendFuture` is never
        // moved out of.
        let inner = unsafe { self.map_unchecked_mut(|this| &mut this.0) };

        inner.poll(cx)
    }
}

/// Marks an arbitrary value as `Send`.
pub struct SendWrapper<T>(pub T);

impl<T> SendWrapper<T> {
    pub fn new(inner: T) -> Self {
        SendWrapper(inner)
    }
}

// SAFETY: see `SendFuture`.
unsafe impl<T> Send for SendWrapper<T> {}

impl<T> Deref for SendWrapper<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for SendWrapper<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Clone> Clone for SendWrapper<T> {
    fn clone(&self) -> Self {
        SendWrapper(self.0.clone())
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for SendWrapper<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: std::fmt::Display> std::fmt::Display for SendWrapper<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
