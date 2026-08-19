//! The `console_*` macros workers-rs exposes.
//!
//! The host reads the guest's stdout as info logs and its stderr as error
//! logs, so `console_error!` goes to stderr and everything else to stdout.

/// Logs a message at info level.
#[macro_export]
macro_rules! console_log {
    ($($t:tt)*) => {
        ::std::println!($($t)*)
    };
}

/// Logs a message at info level.
#[macro_export]
macro_rules! console_debug {
    ($($t:tt)*) => {
        ::std::println!($($t)*)
    };
}

/// Logs a message at info level; the host has no separate warn stream.
#[macro_export]
macro_rules! console_warn {
    ($($t:tt)*) => {
        ::std::println!($($t)*)
    };
}

/// Logs a message at error level.
#[macro_export]
macro_rules! console_error {
    ($($t:tt)*) => {
        ::std::eprintln!($($t)*)
    };
}
