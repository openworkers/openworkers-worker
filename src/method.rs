use std::fmt::Display;

use crate::wit::wasi::http0_2_0::types::Method as WasiMethod;

/// The method of a request, as workers-rs spells it.
#[derive(Default, Debug, Clone, PartialEq, Hash, Eq)]
pub enum Method {
    Head = 0,
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Connect,
    Trace,
    Report,
}

impl Method {
    pub fn all() -> Vec<Method> {
        vec![
            Method::Head,
            Method::Get,
            Method::Post,
            Method::Put,
            Method::Patch,
            Method::Delete,
            Method::Options,
            Method::Connect,
            Method::Trace,
            Method::Report,
        ]
    }
}

impl From<String> for Method {
    fn from(method: String) -> Self {
        match method.to_ascii_uppercase().as_str() {
            "HEAD" => Method::Head,
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "PATCH" => Method::Patch,
            "DELETE" => Method::Delete,
            "OPTIONS" => Method::Options,
            "CONNECT" => Method::Connect,
            "TRACE" => Method::Trace,
            "REPORT" => Method::Report,
            _ => Method::Get,
        }
    }
}

impl From<Method> for String {
    fn from(method: Method) -> Self {
        method.as_ref().to_string()
    }
}

impl AsRef<str> for Method {
    fn as_ref(&self) -> &'static str {
        match self {
            Method::Head => "HEAD",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Options => "OPTIONS",
            Method::Connect => "CONNECT",
            Method::Trace => "TRACE",
            Method::Get => "GET",
            Method::Report => "REPORT",
        }
    }
}

impl Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

impl From<WasiMethod> for Method {
    fn from(method: WasiMethod) -> Self {
        match method {
            WasiMethod::Get => Method::Get,
            WasiMethod::Head => Method::Head,
            WasiMethod::Post => Method::Post,
            WasiMethod::Put => Method::Put,
            WasiMethod::Delete => Method::Delete,
            WasiMethod::Connect => Method::Connect,
            WasiMethod::Options => Method::Options,
            WasiMethod::Trace => Method::Trace,
            WasiMethod::Patch => Method::Patch,
            WasiMethod::Other(other) => Method::from(other),
        }
    }
}

impl From<&Method> for WasiMethod {
    fn from(method: &Method) -> Self {
        match method {
            Method::Get => WasiMethod::Get,
            Method::Head => WasiMethod::Head,
            Method::Post => WasiMethod::Post,
            Method::Put => WasiMethod::Put,
            Method::Delete => WasiMethod::Delete,
            Method::Connect => WasiMethod::Connect,
            Method::Options => WasiMethod::Options,
            Method::Trace => WasiMethod::Trace,
            Method::Patch => WasiMethod::Patch,
            Method::Report => WasiMethod::Other("REPORT".to_string()),
        }
    }
}

#[cfg(feature = "p3")]
use crate::wit_v3::wasi::http0_3_0::types::Method as V3Method;

#[cfg(feature = "p3")]
impl From<V3Method> for Method {
    fn from(method: V3Method) -> Self {
        match method {
            V3Method::Get => Method::Get,
            V3Method::Head => Method::Head,
            V3Method::Post => Method::Post,
            V3Method::Put => Method::Put,
            V3Method::Delete => Method::Delete,
            V3Method::Connect => Method::Connect,
            V3Method::Options => Method::Options,
            V3Method::Trace => Method::Trace,
            V3Method::Patch => Method::Patch,
            V3Method::Other(other) => Method::from(other),
        }
    }
}

#[cfg(feature = "p3")]
impl From<&Method> for V3Method {
    fn from(method: &Method) -> Self {
        match method {
            Method::Get => V3Method::Get,
            Method::Head => V3Method::Head,
            Method::Post => V3Method::Post,
            Method::Put => V3Method::Put,
            Method::Delete => V3Method::Delete,
            Method::Connect => V3Method::Connect,
            Method::Options => V3Method::Options,
            Method::Trace => V3Method::Trace,
            Method::Patch => V3Method::Patch,
            Method::Report => V3Method::Other("REPORT".to_string()),
        }
    }
}
