//! `Env`: the worker's variables and its bindings.
//!
//! Variables and secrets are process environment variables here, fed by the
//! host through `wasi:cli/environment`; workers-rs cannot tell the two
//! apart either, since both arrive as strings on the same object.

use std::fmt::Display;

use serde::de::DeserializeOwned;

use crate::Error;
use crate::Result;

/// The worker's bindings and variables.
#[derive(Debug, Clone, Default)]
pub struct Env;

impl Env {
    /// Looks up a binding by name.
    pub fn get_binding<T: EnvBinding>(&self, name: &str) -> Result<T> {
        T::get(name)
    }

    /// A secret, which is an environment variable the host marked as one.
    pub fn secret(&self, binding: &str) -> Result<Secret> {
        self.get_binding(binding)
    }

    /// An environment variable.
    pub fn var(&self, binding: &str) -> Result<Var> {
        self.get_binding(binding)
    }

    /// An environment variable holding JSON.
    pub fn object_var<T: DeserializeOwned>(&self, binding: &str) -> Result<T> {
        let raw = self.var(binding)?;

        serde_json::from_str(&raw.0).map_err(Error::SerdeJsonError)
    }
}

/// Anything that can be pulled off [`Env`] by name.
pub trait EnvBinding: Sized {
    const TYPE_NAME: &'static str;

    fn get(name: &str) -> Result<Self>;
}

/// A binding that is just a string: a variable or a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringBinding(pub(crate) String);

impl EnvBinding for StringBinding {
    const TYPE_NAME: &'static str = "String";

    fn get(name: &str) -> Result<Self> {
        std::env::var(name)
            .map(StringBinding)
            .map_err(|_| Error::BindingError(name.to_string()))
    }
}

impl Display for StringBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<StringBinding> for String {
    fn from(binding: StringBinding) -> Self {
        binding.0
    }
}

impl AsRef<str> for StringBinding {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

pub type Var = StringBinding;
pub type Secret = StringBinding;
