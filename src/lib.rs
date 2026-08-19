//! A guest SDK for OpenWorkers, source-compatible with Cloudflare's
//! `worker` crate 0.8.
//!
//! A workers-rs application moves over by renaming the dependency:
//!
//! ```toml
//! worker = { package = "openworkers-worker", version = "0.1", features = ["d1"] }
//! ```
//!
//! The same `use worker::*`, the same `#[event(fetch)]`, the same
//! `Request`/`Response`/`Headers`/`Env`. Underneath there is no JavaScript:
//! the crate targets `wasm32-wasip2` and speaks `wasi:http/proxy@0.2.0`
//! plus OpenWorkers' own WIT to the host.
//!
//! # What is not here
//!
//! Queues, Durable Objects, WebSockets, Cache and Images have no OpenWorkers
//! counterpart and are absent rather than stubbed.

#![allow(clippy::new_without_default)]

use std::result::Result as StdResult;

/// Bindings for `wasi:http/proxy` plus the imports a guest needs. The
/// `#[event]` macro expands against this module; nothing else should.
#[doc(hidden)]
pub mod wit {
    wit_bindgen::generate!({
        world: "fetch-worker",
        path: "wit",
        generate_all,
        pub_export_macro: true,
        export_macro_name: "__export_worker_http",
        default_bindings_module: "openworkers_worker::wit",
    });
}

/// The cron export, generated on its own so that a worker without
/// `#[event(scheduled)]` carries no scheduled export at all.
#[doc(hidden)]
pub mod wit_scheduled {
    wit_bindgen::generate!({
        world: "scheduled-only",
        path: "wit",
        generate_all,
        pub_export_macro: true,
        export_macro_name: "__export_worker_scheduled",
        default_bindings_module: "openworkers_worker::wit_scheduled",
    });
}

/// Bindings for the WASI 0.3 world. The wit declares `handle` as an async
/// func, so wit-bindgen emits async bindings without further options.
#[cfg(feature = "p3")]
#[doc(hidden)]
pub mod wit_v3 {
    wit_bindgen::generate!({
        world: "fetch-worker-v3",
        path: "wit",
        generate_all,
        pub_export_macro: true,
        export_macro_name: "__export_worker_http_v3",
        default_bindings_module: "openworkers_worker::wit_v3",
    });
}

#[macro_use]
mod console;

mod context;
mod date;
mod email;
mod env;
mod error;
mod fetch;
mod glue;
mod headers;
mod http_body;
mod jsvalue;
mod method;
mod request;
mod request_init;
mod response;
mod rt;
#[cfg(feature = "p3")]
mod rt_v3;
mod schedule;
mod streams;

pub mod kv;
pub mod panic_hook;
pub mod r2;
pub mod send;

#[cfg(feature = "d1")]
pub mod d1;

pub use openworkers_worker_macros::consume;
pub use openworkers_worker_macros::durable_object;
pub use openworkers_worker_macros::event;
pub use openworkers_worker_macros::send;

pub use url::Url;

pub use crate::context::Context;
#[cfg(feature = "d1")]
pub use crate::d1::*;
pub use crate::date::Date;
pub use crate::date::DateInit;
pub use crate::email::*;
pub use crate::env::Env;
pub use crate::env::EnvBinding;
pub use crate::env::Secret;
pub use crate::env::StringBinding;
pub use crate::env::Var;
pub use crate::error::Error;
pub use crate::fetch::AbortSignal;
pub use crate::fetch::Fetch;
pub use crate::headers::HeaderIterator;
pub use crate::headers::Headers;
pub use crate::jsvalue::js_sys;
pub use crate::jsvalue::wasm_bindgen;
pub use crate::jsvalue::wasm_bindgen_futures;
pub use crate::kv::KvError;
pub use crate::kv::KvStore;
pub use crate::method::Method;
pub use crate::r2::*;
pub use crate::request::Cf;
pub use crate::request::FromRequest;
pub use crate::request::Request;
pub use crate::request_init::CacheMode;
pub use crate::request_init::CfProperties;
pub use crate::request_init::RequestInit;
pub use crate::request_init::RequestRedirect;
pub use crate::response::EncodeBody;
pub use crate::response::IntoResponse;
pub use crate::response::Response;
pub use crate::response::ResponseBody;
pub use crate::response::ResponseBuilder;
pub use crate::schedule::ScheduleContext;
pub use crate::schedule::ScheduledEvent;
pub use crate::streams::ByteStream;

/// The panic hook module workers-rs users reach for. Ours writes to stderr,
/// which the host forwards as an error log.
pub use crate::panic_hook as console_error_panic_hook;

/// A `Result` alias defaulting to [`Error`].
pub type Result<T, E = Error> = StdResult<T, E>;

// The crate root cannot re-export a bare `Ok` without shadowing the
// built-in pattern for anyone writing `use worker::*`.
/// Use `ok::Ok(value)` as a shorthand for `Result::<_, Error>::Ok(value)`.
pub mod ok {
    use super::Result;

    #[allow(non_snake_case)]
    /// Returns `Result::Ok(value)` with [`crate::Error`] as the error type.
    pub fn Ok<T>(value: T) -> Result<T> {
        Result::Ok(value)
    }
}

pub use crate::http_body::Body;

/// **Requires** the `http` feature. Type alias for `http::Request<worker::Body>`.
#[cfg(feature = "http")]
pub type HttpRequest = ::http::Request<crate::http_body::Body>;

/// **Requires** the `http` feature. Type alias for `http::Response<worker::Body>`.
#[cfg(feature = "http")]
pub type HttpResponse = ::http::Response<crate::http_body::Body>;

#[doc(hidden)]
pub mod __private {
    #[cfg(not(feature = "p3"))]
    pub use crate::glue::serve_fetch;
    #[cfg(feature = "p3")]
    pub use crate::glue::serve_fetch_v3;
    pub use crate::glue::serve_scheduled;
}

/// What `#[event(fetch)]` expands through: the same macro name emits the
/// 0.2 or the 0.3 export depending on the `p3` feature, so the application
/// code does not change when it switches worlds.
#[cfg(not(feature = "p3"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __emit_fetch_export {
    ($component:ident, $handler:path, $respond_with_errors:expr) => {
        impl $crate::wit::exports::wasi::http0_2_0::incoming_handler::Guest for $component {
            fn handle(
                request: $crate::wit::wasi::http0_2_0::types::IncomingRequest,
                response_out: $crate::wit::wasi::http0_2_0::types::ResponseOutparam,
            ) {
                $crate::__private::serve_fetch(
                    request,
                    response_out,
                    $respond_with_errors,
                    $handler,
                )
            }
        }

        $crate::wit::__export_worker_http!($component with_types_in $crate::wit);
    };
}

#[cfg(feature = "p3")]
#[doc(hidden)]
#[macro_export]
macro_rules! __emit_fetch_export {
    ($component:ident, $handler:path, $respond_with_errors:expr) => {
        impl $crate::wit_v3::exports::wasi::http0_3_0::handler::Guest for $component {
            async fn handle(
                request: $crate::wit_v3::wasi::http0_3_0::types::Request,
            ) -> ::std::result::Result<
                $crate::wit_v3::wasi::http0_3_0::types::Response,
                $crate::wit_v3::wasi::http0_3_0::types::ErrorCode,
            > {
                $crate::__private::serve_fetch_v3(request, $respond_with_errors, $handler).await
            }
        }

        $crate::wit_v3::__export_worker_http_v3!($component with_types_in $crate::wit_v3);
    };
}
