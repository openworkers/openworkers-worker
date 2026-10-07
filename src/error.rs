//! The crate error, kept to a subset of workers-rs' variants so that a
//! ported application's `match` arms and `map_err` closures still compile.

use std::fmt::Display;
use std::fmt::Formatter;

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    BodyUsed,
    Json((String, u16)),
    Http(http::Error),
    Infallible,
    Io(std::io::Error),
    BindingError(String),
    RustError(String),
    SerdeJsonError(serde_json::Error),
    StatusCode(http::status::InvalidStatusCode),
    Utf8Error(std::str::Utf8Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::BodyUsed => write!(f, "body has already been read"),
            Error::Json((message, status)) => write!(f, "{message} (status: {status})"),
            Error::Http(e) => write!(f, "{e}"),
            Error::Infallible => write!(f, "invalid error type, this should never happen"),
            Error::Io(e) => write!(f, "{e}"),
            Error::BindingError(name) => write!(f, "no binding found for `{name}`"),
            Error::RustError(message) => write!(f, "{message}"),
            Error::SerdeJsonError(e) => write!(f, "{e}"),
            Error::StatusCode(e) => write!(f, "{e}"),
            Error::Utf8Error(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Error::RustError(message.to_string())
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Error::RustError(message)
    }
}

impl From<std::convert::Infallible> for Error {
    fn from(_: std::convert::Infallible) -> Self {
        Error::Infallible
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::SerdeJsonError(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<http::Error> for Error {
    fn from(e: http::Error) -> Self {
        Error::Http(e)
    }
}

impl From<http::status::InvalidStatusCode> for Error {
    fn from(e: http::status::InvalidStatusCode) -> Self {
        Error::StatusCode(e)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Self {
        Error::Utf8Error(e)
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(e: std::string::FromUtf8Error) -> Self {
        Error::Utf8Error(e.utf8_error())
    }
}

impl From<url::ParseError> for Error {
    fn from(e: url::ParseError) -> Self {
        Error::RustError(e.to_string())
    }
}

impl From<serde_urlencoded::de::Error> for Error {
    fn from(e: serde_urlencoded::de::Error) -> Self {
        Error::RustError(e.to_string())
    }
}
