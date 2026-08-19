//! The incoming request. On the 0.2 world bodies are buffered at the host
//! boundary, so a `Request` owns its bytes and `clone` really does hand out
//! a second readable copy the way `Request.clone()` does in JavaScript. On
//! the 0.3 world the body arrives as a stream: reading buffers as usual,
//! `stream()` hands the live stream out, and cloning an unread streaming
//! body is refused, as with a disturbed JavaScript body.

use bytes::Bytes;
use serde::de::DeserializeOwned;
use url::Url;

use crate::streams::ByteStream;
use crate::wasm_bindgen::Value;
use crate::Error;
use crate::Headers;
use crate::Method;
use crate::RequestInit;
use crate::Result;

/// Where the body bytes come from; only the 0.3 world produces streams
#[derive(Debug)]
pub(crate) enum BodySource {
    Buffered(Vec<u8>),
    #[cfg(feature = "p3")]
    Streamed(ByteStream),
}

impl Clone for BodySource {
    fn clone(&self) -> Self {
        match self {
            BodySource::Buffered(bytes) => BodySource::Buffered(bytes.clone()),
            #[cfg(feature = "p3")]
            BodySource::Streamed(_) => {
                panic!("a streaming request body cannot be cloned; read it first")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    method: Method,
    url: Url,
    path: String,
    headers: Headers,
    body: Option<BodySource>,
    immutable: bool,
}

impl Request {
    /// Builds a request from an absolute URL.
    pub fn new(uri: &str, method: Method) -> Result<Self> {
        let url = Url::parse(uri)?;

        Ok(Request {
            path: url.path().to_string(),
            method,
            url,
            headers: Headers::new(),
            body: None,
            immutable: false,
        })
    }

    pub fn new_with_init(uri: &str, init: &RequestInit) -> Result<Self> {
        let mut request = Request::new(uri, init.method.clone())?;

        request.headers = init.headers.clone();
        request.body = init.body.as_ref().map(|body| {
            BodySource::Buffered(match &body.0 {
                Value::Str(text) => text.clone().into_bytes(),
                Value::Bytes(bytes) => bytes.clone(),
                Value::Null | Value::Undefined => Vec::new(),
                _ => body.to_string().into_bytes(),
            })
        });

        Ok(request)
    }

    pub(crate) fn from_parts(
        method: Method,
        url: Url,
        headers: Headers,
        body: Option<BodySource>,
    ) -> Self {
        Request {
            path: url.path().to_string(),
            method,
            url,
            headers,
            body,
            immutable: true,
        }
    }

    pub async fn json<B: DeserializeOwned>(&mut self) -> Result<B> {
        serde_json::from_slice(&self.bytes().await?).map_err(Error::SerdeJsonError)
    }

    pub async fn text(&mut self) -> Result<String> {
        String::from_utf8(self.bytes().await?).map_err(Error::from)
    }

    pub async fn bytes(&mut self) -> Result<Vec<u8>> {
        match self.take_body()? {
            BodySource::Buffered(bytes) => Ok(bytes),
            #[cfg(feature = "p3")]
            BodySource::Streamed(stream) => stream.collect_bytes().await,
        }
    }

    pub fn stream(&mut self) -> Result<ByteStream> {
        match self.take_body()? {
            BodySource::Buffered(bytes) => Ok(ByteStream::new(bytes)),
            #[cfg(feature = "p3")]
            BodySource::Streamed(stream) => Ok(stream),
        }
    }

    fn take_body(&mut self) -> Result<BodySource> {
        self.body.take().ok_or(Error::BodyUsed)
    }

    /// Whether `Clone::clone` would panic on this request
    #[cfg(feature = "p3")]
    fn body_is_streaming(&self) -> bool {
        matches!(self.body, Some(BodySource::Streamed(_)))
    }

    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    pub fn headers_mut(&mut self) -> Result<&mut Headers> {
        if self.immutable {
            return Err(Error::RustError(
                "Cannot get a mutable reference to an immutable headers object.".into(),
            ));
        }

        Ok(&mut self.headers)
    }

    /// Cloudflare's request metadata. OpenWorkers does not supply one.
    pub fn cf(&self) -> Option<&Cf> {
        None
    }

    pub fn method(&self) -> Method {
        self.method.clone()
    }

    pub fn path(&self) -> String {
        self.path.clone()
    }

    pub fn path_mut(&mut self) -> Result<&mut String> {
        if self.immutable {
            return Err(Error::RustError(
                "Cannot get a mutable reference to an immutable path.".into(),
            ));
        }

        Ok(&mut self.path)
    }

    pub fn url(&self) -> Result<Url> {
        Ok(self.url.clone())
    }

    /// Deserializes the query string.
    pub fn query<Q: DeserializeOwned>(&self) -> Result<Q> {
        serde_urlencoded::from_str(self.url.query().unwrap_or_default()).map_err(Error::from)
    }

    /// A second handle on the same request, with its own readable body.
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> Result<Self> {
        #[cfg(feature = "p3")]
        if self.body_is_streaming() {
            return Err(Error::RustError(
                "a streaming request body cannot be cloned; read it first".into(),
            ));
        }

        Ok(Clone::clone(self))
    }

    pub fn clone_mut(&self) -> Result<Self> {
        let mut request = self.clone()?;
        request.immutable = false;
        request.headers = self.headers.entries().collect();

        Ok(request)
    }
}

/// Cloudflare's request properties. The type exists so that code reading
/// `req.cf()` compiles; OpenWorkers supplies no edge metadata to put in it.
#[derive(Debug, Clone, Default)]
pub struct Cf;

/// Any type a fetch handler can take as its request argument.
pub trait FromRequest: Sized {
    fn from_raw(
        request: Request,
    ) -> std::result::Result<Self, impl Into<Box<dyn std::error::Error>>>;
}

impl FromRequest for Request {
    fn from_raw(
        request: Request,
    ) -> std::result::Result<Self, impl Into<Box<dyn std::error::Error>>> {
        Ok::<Request, Error>(request)
    }
}

#[cfg(feature = "http")]
impl FromRequest for crate::HttpRequest {
    fn from_raw(
        request: Request,
    ) -> std::result::Result<Self, impl Into<Box<dyn std::error::Error>>> {
        crate::HttpRequest::try_from(request)
    }
}

impl<B: http_body::Body<Data = Bytes> + 'static> TryFrom<http::Request<B>> for Request {
    type Error = Error;

    fn try_from(request: http::Request<B>) -> Result<Self> {
        let (parts, body) = request.into_parts();
        let body = crate::http_body::collect(body)?;

        let url = Url::parse(&parts.uri.to_string())?;

        Ok(Request {
            path: url.path().to_string(),
            method: Method::from(parts.method.as_str().to_string()),
            url,
            headers: Headers::from(&parts.headers),
            body: Some(BodySource::Buffered(body.to_vec())),
            immutable: false,
        })
    }
}

impl TryFrom<Request> for http::Request<crate::http_body::Body> {
    type Error = Error;

    fn try_from(mut request: Request) -> Result<Self> {
        let body = match request.body.take() {
            None => crate::http_body::Body::empty(),
            Some(BodySource::Buffered(bytes)) => crate::http_body::Body::from(bytes),
            #[cfg(feature = "p3")]
            Some(BodySource::Streamed(stream)) => crate::http_body::Body::from_stream(stream),
        };

        let mut builder = http::Request::builder()
            .method(request.method.as_ref())
            .uri(request.url.as_str());

        for (name, value) in request.headers.entries() {
            builder = builder.header(name, value);
        }

        builder.body(body).map_err(Error::Http)
    }
}
