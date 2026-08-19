//! The fetch handler's context.

use serde::de::DeserializeOwned;

#[cfg(not(feature = "p3"))]
use crate::rt;
#[cfg(feature = "p3")]
use crate::rt_v3;
use crate::Error;
use crate::Result;

/// Extends the life of the invocation past the response.
#[derive(Debug, Clone, Default)]
pub struct Context;

impl Context {
    /// Queues work to finish before the invocation ends. On the 0.2 world
    /// the host has no notion of an invocation outliving its response, so
    /// the future runs after the body is written and before the export
    /// returns; on the 0.3 world it runs after the response is handed over,
    /// and the trailers are held back until it is done.
    pub fn wait_until<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + 'static,
    {
        #[cfg(not(feature = "p3"))]
        rt::spawn_local(future);

        #[cfg(feature = "p3")]
        rt_v3::wait_until(future);
    }

    /// Cloudflare's escape hatch to the origin. There is no origin here.
    pub fn pass_through_on_exception(&self) {}

    /// Dispatch namespace properties, which OpenWorkers does not have.
    pub fn props<T: DeserializeOwned>(&self) -> Result<T> {
        Err(Error::RustError(
            "props are not supported by this platform".into(),
        ))
    }
}
