//! `RequestInit` and the `cf` bag that goes with it. The host honours the
//! method, headers and body; the Cloudflare-specific knobs are carried so
//! that call sites compile, and ignored.

use std::collections::HashMap;

use crate::wasm_bindgen::JsValue;
use crate::Headers;
use crate::Method;

/// Optional properties for [`crate::Request::new_with_init`].
#[derive(Debug, Clone, Default)]
pub struct RequestInit {
    pub body: Option<JsValue>,
    pub headers: Headers,
    pub cf: CfProperties,
    pub method: Method,
    pub redirect: RequestRedirect,
    pub cache: Option<CacheMode>,
}

impl RequestInit {
    pub fn new() -> Self {
        RequestInit::default()
    }

    pub fn with_headers(&mut self, headers: Headers) -> &mut Self {
        self.headers = headers;
        self
    }

    pub fn with_method(&mut self, method: Method) -> &mut Self {
        self.method = method;
        self
    }

    pub fn with_redirect(&mut self, redirect: RequestRedirect) -> &mut Self {
        self.redirect = redirect;
        self
    }

    pub fn with_cache(&mut self, cache: CacheMode) -> &mut Self {
        self.cache = Some(cache);
        self
    }

    pub fn with_body(&mut self, body: Option<JsValue>) -> &mut Self {
        self.body = body;
        self
    }

    pub fn with_cf_properties(&mut self, props: CfProperties) -> &mut Self {
        self.cf = props;
        self
    }
}

/// The `cf` object of a request. OpenWorkers has no edge cache to steer, so
/// these are accepted and dropped.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CfProperties {
    pub apps: Option<bool>,
    pub cache_everything: Option<bool>,
    pub cache_key: Option<String>,
    pub cache_ttl: Option<i32>,
    pub cache_ttl_by_status: Option<HashMap<String, i32>>,
    pub mirage: Option<bool>,
    pub resolve_override: Option<String>,
    pub scrape_shield: Option<bool>,
}

impl CfProperties {
    pub fn new() -> Self {
        CfProperties::default()
    }

    pub fn is_default(&self) -> bool {
        *self == CfProperties::default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RequestRedirect {
    Error,
    #[default]
    Follow,
    Manual,
}

impl From<RequestRedirect> for &str {
    fn from(redirect: RequestRedirect) -> Self {
        match redirect {
            RequestRedirect::Error => "error",
            RequestRedirect::Follow => "follow",
            RequestRedirect::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    Default,
    NoStore,
}

impl From<CacheMode> for &str {
    fn from(cache: CacheMode) -> Self {
        match cache {
            CacheMode::Default => "default",
            CacheMode::NoStore => "no-store",
        }
    }
}
