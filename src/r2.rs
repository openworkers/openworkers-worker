//! Object storage through `openworkers:bindings/storage`.
//!
//! Bodies are buffered end to end, so an `Object` carries its bytes and a
//! ranged read is a slice taken here rather than at the store.

use std::collections::HashMap;
use std::ops::Deref;

use crate::env::EnvBinding;
use crate::streams::ByteStream;
use crate::wit::openworkers::bindings::storage;
use crate::Date;
use crate::DateInit;
use crate::Env;
use crate::Error;
use crate::Headers;
use crate::ResponseBody;
use crate::Result;

impl Env {
    /// An R2 bucket binding.
    pub fn bucket(&self, binding: &str) -> Result<Bucket> {
        self.get_binding(binding)
    }
}

/// An object store.
#[derive(Debug, Clone)]
pub struct Bucket {
    binding: String,
}

impl EnvBinding for Bucket {
    const TYPE_NAME: &'static str = "R2Bucket";

    fn get(name: &str) -> Result<Self> {
        Ok(Bucket {
            binding: name.to_string(),
        })
    }
}

impl Bucket {
    /// Object metadata without its body.
    ///
    /// Unlike Cloudflare's, the host's `head` cannot separate a missing key
    /// from a failure, so a missing key is an `Err` here rather than
    /// `Ok(None)`. Use [`Bucket::get`] when the two have to be told apart.
    pub async fn head(&self, key: impl Into<String>) -> Result<Option<Object>> {
        let key = key.into();
        let info = storage::head(&self.binding, &key).map_err(storage_error)?;

        Ok(Some(Object {
            key,
            size: info.size,
            etag: info.etag,
            body: None,
        }))
    }

    pub fn get(&self, key: impl Into<String>) -> GetOptionsBuilder<'_> {
        GetOptionsBuilder {
            bucket: self,
            key: key.into(),
            range: None,
        }
    }

    pub fn put(&self, key: impl Into<String>, value: impl Into<Data>) -> PutOptionsBuilder<'_> {
        PutOptionsBuilder {
            bucket: self,
            key: key.into(),
            value: value.into(),
        }
    }

    pub async fn delete(&self, key: impl Into<String>) -> Result<()> {
        storage::delete(&self.binding, &key.into()).map_err(storage_error)
    }

    pub async fn delete_multiple(&self, keys: Vec<impl Deref<Target = str>>) -> Result<()> {
        for key in keys {
            storage::delete(&self.binding, &key).map_err(storage_error)?;
        }

        Ok(())
    }

    pub fn list(&self) -> ListOptionsBuilder<'_> {
        ListOptionsBuilder {
            bucket: self,
            prefix: None,
            limit: None,
        }
    }
}

/// One stored object, with its body when it was read.
#[derive(Debug, Clone)]
pub struct Object {
    key: String,
    size: u64,
    etag: Option<String>,
    body: Option<Vec<u8>>,
}

impl Object {
    pub fn key(&self) -> String {
        self.key.clone()
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn etag(&self) -> String {
        self.etag.clone().unwrap_or_default()
    }

    pub fn http_etag(&self) -> String {
        match &self.etag {
            Some(etag) => format!("\"{etag}\""),
            None => String::new(),
        }
    }

    /// The store keeps no version, so this is the etag when there is one.
    pub fn version(&self) -> String {
        self.etag()
    }

    /// The store keeps no upload time.
    pub fn uploaded(&self) -> Date {
        Date::new(DateInit::Millis(0))
    }

    /// The store keeps no HTTP metadata.
    pub fn http_metadata(&self) -> HttpMetadata {
        HttpMetadata::default()
    }

    /// The store keeps no custom metadata.
    pub fn custom_metadata(&self) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }

    pub fn body(&self) -> Option<ObjectBody<'_>> {
        self.body.as_ref().map(|bytes| ObjectBody { bytes })
    }

    pub fn body_used(&self) -> Option<bool> {
        Some(self.body.is_none())
    }

    /// Nothing to write: the store keeps no HTTP metadata to write out.
    pub fn write_http_metadata(&self, _headers: Headers) -> Result<()> {
        Ok(())
    }
}

/// The bytes of an object.
#[derive(Debug)]
pub struct ObjectBody<'body> {
    bytes: &'body [u8],
}

impl ObjectBody<'_> {
    pub fn stream(self) -> Result<ByteStream> {
        Ok(ByteStream::new(self.bytes.to_vec()))
    }

    pub fn response_body(self) -> Result<ResponseBody> {
        Ok(ResponseBody::Body(self.bytes.to_vec()))
    }

    pub async fn bytes(self) -> Result<Vec<u8>> {
        Ok(self.bytes.to_vec())
    }

    pub async fn text(self) -> Result<String> {
        String::from_utf8(self.bytes.to_vec()).map_err(Error::from)
    }
}

/// What a listing returned.
#[derive(Debug, Clone)]
pub struct Objects {
    objects: Vec<Object>,
    truncated: bool,
}

impl Objects {
    pub fn objects(&self) -> Vec<Object> {
        self.objects.clone()
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// The binding pages by limit alone, so there is no cursor to hand back.
    pub fn cursor(&self) -> Option<String> {
        None
    }

    /// Delimited listing needs host support the binding does not have.
    pub fn delimited_prefixes(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Anything [`Bucket::put`] accepts as a body.
#[derive(Debug, Clone)]
pub enum Data {
    Text(String),
    Bytes(Vec<u8>),
}

impl Data {
    fn into_bytes(self) -> Vec<u8> {
        match self {
            Data::Text(text) => text.into_bytes(),
            Data::Bytes(bytes) => bytes,
        }
    }
}

impl From<String> for Data {
    fn from(text: String) -> Self {
        Data::Text(text)
    }
}

impl From<&str> for Data {
    fn from(text: &str) -> Self {
        Data::Text(text.to_string())
    }
}

impl From<Vec<u8>> for Data {
    fn from(bytes: Vec<u8>) -> Self {
        Data::Bytes(bytes)
    }
}

impl From<&[u8]> for Data {
    fn from(bytes: &[u8]) -> Self {
        Data::Bytes(bytes.to_vec())
    }
}

/// Reads one object.
#[derive(Debug)]
pub struct GetOptionsBuilder<'bucket> {
    bucket: &'bucket Bucket,
    key: String,
    range: Option<Range>,
}

impl GetOptionsBuilder<'_> {
    /// Conditional reads need an etag the host does not report on `get`.
    pub fn only_if(self, _only_if: Conditional) -> Self {
        self
    }

    /// The body is buffered, so the range is applied here.
    pub fn range(mut self, range: Range) -> Self {
        self.range = Some(range);
        self
    }

    pub async fn execute(self) -> Result<Option<Object>> {
        let body = storage::get(&self.bucket.binding, &self.key).map_err(storage_error)?;

        let Some(body) = body else {
            return Ok(None);
        };

        let size = body.len() as u64;
        let body = match self.range {
            None => body,
            Some(range) => range.slice(body),
        };

        Ok(Some(Object {
            key: self.key,
            size,
            etag: None,
            body: Some(body),
        }))
    }
}

/// Writes one object.
#[derive(Debug)]
pub struct PutOptionsBuilder<'bucket> {
    bucket: &'bucket Bucket,
    key: String,
    value: Data,
}

impl PutOptionsBuilder<'_> {
    /// The store keeps bytes only, so metadata is accepted and dropped.
    pub fn http_metadata(self, _metadata: HttpMetadata) -> Self {
        self
    }

    pub fn custom_metadata(self, _metadata: impl Into<HashMap<String, String>>) -> Self {
        self
    }

    pub fn only_if(self, _only_if: Conditional) -> Self {
        self
    }

    pub async fn execute(self) -> Result<Option<Object>> {
        let bytes = self.value.into_bytes();
        let size = bytes.len() as u64;

        storage::put(&self.bucket.binding, &self.key, &bytes).map_err(storage_error)?;

        Ok(Some(Object {
            key: self.key,
            size,
            etag: None,
            body: None,
        }))
    }
}

/// Lists objects.
#[derive(Debug)]
pub struct ListOptionsBuilder<'bucket> {
    bucket: &'bucket Bucket,
    prefix: Option<String>,
    limit: Option<u32>,
}

impl ListOptionsBuilder<'_> {
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Cursors, delimiters and inclusions need host support the binding does
    /// not have; the listing is by prefix and limit.
    pub fn cursor(self, _cursor: impl Into<String>) -> Self {
        self
    }

    pub fn start_after(self, _start_after: impl Into<String>) -> Self {
        self
    }

    pub fn delimiter(self, _delimiter: impl Into<String>) -> Self {
        self
    }

    pub fn include(self, _include: Vec<Include>) -> Self {
        self
    }

    pub async fn execute(self) -> Result<Objects> {
        let listing = storage::list_keys(&self.bucket.binding, self.prefix.as_deref(), self.limit)
            .map_err(storage_error)?;

        Ok(Objects {
            objects: listing
                .keys
                .into_iter()
                .map(|key| Object {
                    key,
                    size: 0,
                    etag: None,
                    body: None,
                })
                .collect(),
            truncated: listing.truncated,
        })
    }
}

/// A conditional read or write. Accepted for source compatibility.
#[derive(Debug, Clone, Default)]
pub struct Conditional {
    pub etag_matches: Option<String>,
    pub etag_does_not_match: Option<String>,
    pub uploaded_before: Option<Date>,
    pub uploaded_after: Option<Date>,
}

/// Which slice of an object to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    OffsetWithLength { offset: u64, length: u64 },
    OffsetToEnd { offset: u64 },
    Prefix { length: u64 },
    Suffix { suffix: u64 },
}

impl Range {
    fn slice(self, body: Vec<u8>) -> Vec<u8> {
        let len = body.len();

        let (start, end) = match self {
            Range::OffsetWithLength { offset, length } => {
                (offset as usize, offset.saturating_add(length) as usize)
            }
            Range::OffsetToEnd { offset } => (offset as usize, len),
            Range::Prefix { length } => (0, length as usize),
            Range::Suffix { suffix } => (len.saturating_sub(suffix as usize), len),
        };

        body[start.min(len)..end.min(len)].to_vec()
    }
}

/// The HTTP metadata Cloudflare stores alongside an object. Nothing fills
/// these in here.
#[derive(Debug, Clone, Default)]
pub struct HttpMetadata {
    pub content_type: Option<String>,
    pub content_language: Option<String>,
    pub content_disposition: Option<String>,
    pub content_encoding: Option<String>,
    pub cache_control: Option<String>,
    pub cache_expiry: Option<Date>,
}

/// What a listing should carry alongside its keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Include {
    HttpMetadata,
    CustomMetadata,
}

fn storage_error(message: String) -> Error {
    Error::RustError(format!("r2: {message}"))
}
