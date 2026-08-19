//! The panic hook, standing in for `console_error_panic_hook`.
//!
//! A panic already reaches the host through stderr; this only makes the call
//! site from a workers-rs application resolve, and flattens the message to
//! one line so the host logs it as one entry.

use std::sync::Once;

static SET_HOOK: Once = Once::new();

/// Routes panics to stderr, which the host records as an error log.
pub fn set_once() {
    SET_HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let location = info
                .location()
                .map_or_else(String::new, |at| format!(" at {}:{}", at.file(), at.line()));

            let payload = info.payload();
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("<unknown>");

            eprintln!("panic{location}: {message}");
        }));
    });
}
