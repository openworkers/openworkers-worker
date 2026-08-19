//! Procedural macros for `openworkers-worker`.
//!
//! The attribute names and their argument shapes follow Cloudflare's
//! `worker-macros` 0.8 so that a workers-rs crate compiles unchanged; what
//! they expand to is a WASI component export rather than wasm-bindgen glue.

mod event;
mod send;

use proc_macro::TokenStream;

/// Marks a Worker handler. Supported forms are `#[event(fetch)]` and
/// `#[event(scheduled)]`, either with `respond_with_errors`.
///
/// ```ignore
/// #[event(fetch)]
/// async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
///     Response::ok("hello")
/// }
/// ```
#[proc_macro_attribute]
pub fn event(attr: TokenStream, item: TokenStream) -> TokenStream {
    event::expand_macro(attr, item)
}

/// Accepted for source compatibility with workers-rs. The guest is single
/// threaded and nothing here is `!Send`, so this only forwards the item.
#[proc_macro_attribute]
pub fn send(attr: TokenStream, item: TokenStream) -> TokenStream {
    send::expand_macro(attr, item)
}

/// Durable Objects have no counterpart on OpenWorkers.
#[proc_macro_attribute]
pub fn durable_object(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = proc_macro2::TokenStream::from(item);
    quote::quote! {
        ::std::compile_error!(
            "#[durable_object] is not supported by openworkers-worker: \
             Durable Objects have no OpenWorkers counterpart"
        );
        #item
    }
    .into()
}

#[doc(hidden)]
#[proc_macro_attribute]
pub fn consume(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    TokenStream::new()
}
