//! `Date`, as workers-rs spells a JavaScript timestamp.

use std::fmt::Display;

/// A point in time, held as milliseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    millis: u64,
}

/// How to build a [`Date`].
#[derive(Debug, Clone)]
pub enum DateInit {
    Millis(u64),
    String(String),
}

impl From<DateInit> for Date {
    fn from(init: DateInit) -> Self {
        Date::new(init)
    }
}

impl Date {
    pub fn new(init: DateInit) -> Self {
        match init {
            DateInit::Millis(millis) => Date { millis },
            // Parsing arbitrary date strings needs a calendar the guest does
            // not carry; only the millisecond form is honoured.
            DateInit::String(_) => Date { millis: 0 },
        }
    }

    pub fn now() -> Self {
        Date {
            millis: crate::js_sys::Date::now() as u64,
        }
    }

    pub fn as_millis(&self) -> u64 {
        self.millis
    }
}

impl Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.millis)
    }
}
