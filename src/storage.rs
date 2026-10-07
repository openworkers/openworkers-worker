//! Object storage through `openworkers:bindings/storage`: `env.STORAGE` in a
//! JavaScript worker. Bodies are bytes; JavaScript's `get` decodes them as
//! text, this one leaves that to the caller.

use crate::env::EnvBinding;
use crate::wit::openworkers::bindings::storage as host;
use crate::Env;
use crate::Error;
use crate::Result;

impl Env {
    /// A storage binding, `env.STORAGE` in JavaScript.
    pub fn storage(&self, binding: &str) -> Result<BindingStorage> {
        self.get_binding(binding)
    }
}

#[derive(Debug, Clone)]
pub struct BindingStorage {
    binding: String,
}

impl EnvBinding for BindingStorage {
    const TYPE_NAME: &'static str = "BindingStorage";

    fn get(name: &str) -> Result<Self> {
        Ok(BindingStorage {
            binding: name.to_string(),
        })
    }
}

/// What `head` reports about an object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageHeadResult {
    pub size: u64,
    pub etag: Option<String>,
}

/// What `list` returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageListResult {
    pub keys: Vec<String>,
    /// The store held more keys than the limit returned.
    pub truncated: bool,
}

impl BindingStorage {
    /// The object's body, or `None` when the key is absent.
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        host::get(&self.binding, key).map_err(|cause| self.failed(cause))
    }

    /// Stores `body`, replacing any object under `key`.
    pub async fn put(&self, key: &str, body: impl Into<Vec<u8>>) -> Result<()> {
        host::put(&self.binding, key, &body.into()).map_err(|cause| self.failed(cause))
    }

    /// The object's size and etag. A missing object is an error, because the
    /// host reports it as one; `get` tells absence apart.
    pub async fn head(&self, key: &str) -> Result<StorageHeadResult> {
        let info = host::head(&self.binding, key).map_err(|cause| self.failed(cause))?;

        Ok(StorageHeadResult {
            size: info.size,
            etag: info.etag,
        })
    }

    /// The keys under `prefix`, at most `limit` of them; the platform default
    /// applies when `limit` is `None`.
    pub async fn list(
        &self,
        prefix: Option<&str>,
        limit: Option<u32>,
    ) -> Result<StorageListResult> {
        let listing =
            host::list_keys(&self.binding, prefix, limit).map_err(|cause| self.failed(cause))?;

        Ok(StorageListResult {
            keys: listing.keys,
            truncated: listing.truncated,
        })
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        host::delete(&self.binding, key).map_err(|cause| self.failed(cause))
    }

    fn failed(&self, cause: String) -> Error {
        Error::RustError(format!("storage {}: {cause}", self.binding))
    }
}
