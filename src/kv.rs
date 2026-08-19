//! Key-value storage through `openworkers:bindings/kv`.
//!
//! The store holds JSON documents, so a plain string is stored quoted and
//! unquoted on the way back, and `put_bytes` stores a JSON array of bytes.
//! That is the one place where this differs from Cloudflare KV, which stores
//! opaque bytes.

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use crate::env::EnvBinding;
use crate::wit::openworkers::bindings::kv;
use crate::Env;
use crate::Error;
use crate::Result;

impl Env {
    /// A KV namespace binding.
    pub fn kv(&self, binding: &str) -> Result<KvStore> {
        self.get_binding(binding)
    }
}

/// A KV namespace.
#[derive(Debug, Clone)]
pub struct KvStore {
    pub binding: String,
}

impl EnvBinding for KvStore {
    const TYPE_NAME: &'static str = "KvNamespace";

    fn get(name: &str) -> Result<Self> {
        Ok(KvStore {
            binding: name.to_string(),
        })
    }
}

impl KvStore {
    pub fn create(binding: &str) -> std::result::Result<Self, KvError> {
        Ok(KvStore {
            binding: binding.to_string(),
        })
    }

    pub fn get(&self, name: &str) -> GetOptionsBuilder {
        GetOptionsBuilder {
            binding: self.binding.clone(),
            name: name.to_string(),
        }
    }

    pub fn put<T: ToRawKvValue>(
        &self,
        name: &str,
        value: T,
    ) -> std::result::Result<PutOptionsBuilder, KvError> {
        Ok(PutOptionsBuilder {
            binding: self.binding.clone(),
            name: name.to_string(),
            value: value.raw_kv_value()?,
            expiration: None,
            expiration_ttl: None,
        })
    }

    /// Bytes are stored as a JSON array of numbers, which is what a JSON
    /// document store can hold.
    pub fn put_bytes(
        &self,
        name: &str,
        value: &[u8],
    ) -> std::result::Result<PutOptionsBuilder, KvError> {
        Ok(PutOptionsBuilder {
            binding: self.binding.clone(),
            name: name.to_string(),
            value: serde_json::to_string(value)?,
            expiration: None,
            expiration_ttl: None,
        })
    }

    pub fn list(&self) -> ListOptionsBuilder {
        ListOptionsBuilder {
            binding: self.binding.clone(),
            prefix: None,
            limit: None,
        }
    }

    pub async fn delete(&self, name: &str) -> std::result::Result<(), KvError> {
        kv::delete(&self.binding, name).map_err(KvError::Host)
    }
}

/// Reads one key.
#[derive(Debug, Clone)]
pub struct GetOptionsBuilder {
    binding: String,
    name: String,
}

impl GetOptionsBuilder {
    /// Cloudflare's read-cache hint. The host caches nothing here.
    pub fn cache_ttl(self, _cache_ttl: u64) -> Self {
        self
    }

    /// The stored value as text: a stored JSON string comes back unquoted,
    /// anything else comes back as the JSON document it is.
    pub async fn text(self) -> std::result::Result<Option<String>, KvError> {
        let Some(raw) = self.raw()? else {
            return Ok(None);
        };

        Ok(Some(match serde_json::from_str::<Value>(&raw) {
            Ok(Value::String(text)) => text,
            _ => raw,
        }))
    }

    pub async fn json<T>(self) -> std::result::Result<Option<T>, KvError>
    where
        T: DeserializeOwned,
    {
        match self.raw()? {
            None => Ok(None),
            Some(raw) => Ok(Some(serde_json::from_str(&raw)?)),
        }
    }

    pub async fn bytes(self) -> std::result::Result<Option<Vec<u8>>, KvError> {
        let Some(raw) = self.raw()? else {
            return Ok(None);
        };

        // What `put_bytes` wrote reads back as an array; anything else was
        // written as text, so its own bytes are the answer.
        Ok(Some(match serde_json::from_str::<Vec<u8>>(&raw) {
            Ok(bytes) => bytes,
            Err(_) => match serde_json::from_str::<Value>(&raw) {
                Ok(Value::String(text)) => text.into_bytes(),
                _ => raw.into_bytes(),
            },
        }))
    }

    fn raw(&self) -> std::result::Result<Option<String>, KvError> {
        kv::get(&self.binding, &self.name).map_err(KvError::Host)
    }
}

/// Writes one key.
#[derive(Debug, Clone)]
pub struct PutOptionsBuilder {
    binding: String,
    name: String,
    value: String,
    expiration: Option<u64>,
    expiration_ttl: Option<u64>,
}

impl PutOptionsBuilder {
    /// Expires the key at an absolute Unix timestamp, in seconds.
    pub fn expiration(mut self, expiration: u64) -> Self {
        self.expiration = Some(expiration);
        self
    }

    pub fn expiration_ttl(mut self, expiration_ttl: u64) -> Self {
        self.expiration_ttl = Some(expiration_ttl);
        self
    }

    /// Per-key metadata, which the host does not store.
    pub fn metadata<T: Serialize>(self, _metadata: T) -> std::result::Result<Self, KvError> {
        Ok(self)
    }

    pub async fn execute(self) -> std::result::Result<(), KvError> {
        let ttl = self.expiration_ttl.or_else(|| {
            let now = crate::js_sys::Date::now() as u64 / 1_000;

            self.expiration.map(|at| at.saturating_sub(now))
        });

        kv::put(&self.binding, &self.name, &self.value, ttl).map_err(KvError::Host)
    }
}

/// Lists keys.
#[derive(Debug, Clone)]
pub struct ListOptionsBuilder {
    binding: String,
    prefix: Option<String>,
    limit: Option<u64>,
}

impl ListOptionsBuilder {
    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Cursors need host support the binding does not have; a listing that
    /// asks for one gets the first page.
    pub fn cursor(self, _cursor: String) -> Self {
        self
    }

    pub fn prefix(mut self, prefix: String) -> Self {
        self.prefix = Some(prefix);
        self
    }

    pub async fn execute(self) -> std::result::Result<ListResponse, KvError> {
        let limit = self
            .limit
            .map(|limit| limit.min(u64::from(u32::MAX)) as u32);
        let keys =
            kv::list_keys(&self.binding, self.prefix.as_deref(), limit).map_err(KvError::Host)?;

        let list_complete = limit.is_none_or(|limit| keys.len() < limit as usize);

        Ok(ListResponse {
            keys: keys
                .into_iter()
                .map(|name| Key {
                    name,
                    expiration: None,
                    metadata: None,
                })
                .collect(),
            list_complete,
            cursor: None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ListResponse {
    pub keys: Vec<Key>,
    pub list_complete: bool,
    pub cursor: Option<String>,
}

/// One key of a listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Key {
    pub name: String,
    pub expiration: Option<u64>,
    pub metadata: Option<Value>,
}

/// What a KV call can go wrong with.
#[derive(Debug)]
pub enum KvError {
    Host(String),
    Serialization(serde_json::Error),
    InvalidKvStore(String),
}

impl std::fmt::Display for KvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KvError::Host(message) => write!(f, "kv error: {message}"),
            KvError::Serialization(e) => write!(f, "unable to serialize/deserialize: {e}"),
            KvError::InvalidKvStore(binding) => write!(f, "invalid kv store: {binding}"),
        }
    }
}

impl std::error::Error for KvError {}

impl From<serde_json::Error> for KvError {
    fn from(e: serde_json::Error) -> Self {
        KvError::Serialization(e)
    }
}

impl From<KvError> for Error {
    fn from(e: KvError) -> Self {
        Error::RustError(e.to_string())
    }
}

/// Anything [`KvStore::put`] accepts.
pub trait ToRawKvValue {
    fn raw_kv_value(&self) -> std::result::Result<String, KvError>;
}

impl<T: Serialize> ToRawKvValue for T {
    fn raw_kv_value(&self) -> std::result::Result<String, KvError> {
        serde_json::to_string(self).map_err(KvError::from)
    }
}
