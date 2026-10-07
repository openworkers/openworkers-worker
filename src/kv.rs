//! Key-value storage through `openworkers:bindings/kv`: `env.KV` in a
//! JavaScript worker. Values are JSON documents.

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::env::EnvBinding;
use crate::wit::openworkers::bindings::kv as host;
use crate::Env;
use crate::Error;
use crate::Result;

impl Env {
    /// A KV binding, `env.KV` in JavaScript.
    pub fn kv(&self, binding: &str) -> Result<BindingKv> {
        self.get_binding(binding)
    }
}

#[derive(Debug, Clone)]
pub struct BindingKv {
    binding: String,
}

impl EnvBinding for BindingKv {
    const TYPE_NAME: &'static str = "BindingKv";

    fn get(name: &str) -> Result<Self> {
        Ok(BindingKv {
            binding: name.to_string(),
        })
    }
}

impl BindingKv {
    /// The value under `key`, or `None` when the key is absent or expired.
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match host::get(&self.binding, key).map_err(|cause| self.failed(cause))? {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Stores `value` as JSON. It expires after `expires_in` seconds, when
    /// given.
    pub async fn put<T: Serialize + ?Sized>(
        &self,
        key: &str,
        value: &T,
        expires_in: Option<u64>,
    ) -> Result<()> {
        let json = serde_json::to_string(value)?;

        host::put(&self.binding, key, &json, expires_in).map_err(|cause| self.failed(cause))
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        host::delete(&self.binding, key).map_err(|cause| self.failed(cause))
    }

    /// The keys under `prefix`, at most `limit` of them; the platform default
    /// applies when `limit` is `None`.
    pub async fn list(&self, prefix: Option<&str>, limit: Option<u32>) -> Result<Vec<String>> {
        host::list_keys(&self.binding, prefix, limit).map_err(|cause| self.failed(cause))
    }

    fn failed(&self, cause: String) -> Error {
        Error::RustError(format!("kv {}: {cause}", self.binding))
    }
}
