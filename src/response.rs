//! The response a handler hands back. On the 0.2 world bodies are buffered,
//! so `from_stream` collects; on the 0.3 world the stream variant flows to
//! the host as a real `stream<u8>`.

use bytes::Bytes;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::streams::ByteStream;
use crate::Error;
use crate::Headers;
use crate::Result;

/// The body of a [`Response`].
#[derive(Debug, Default)]
pub enum ResponseBody {
    #[default]
    Empty,
    Body(Vec<u8>),
    #[cfg(feature = "p3")]
    Stream(ByteStream),
}

impl Clone for ResponseBody {
    fn clone(&self) -> Self {
        match self {
            ResponseBody::Empty => ResponseBody::Empty,
            ResponseBody::Body(bytes) => ResponseBody::Body(bytes.clone()),
            #[cfg(feature = "p3")]
            ResponseBody::Stream(_) => {
                panic!("a streaming response body cannot be cloned; read it first")
            }
        }
    }
}

impl ResponseBody {
    fn into_bytes(self) -> Vec<u8> {
        match self {
            ResponseBody::Empty => Vec::new(),
            ResponseBody::Body(bytes) => bytes,
            #[cfg(feature = "p3")]
            ResponseBody::Stream(_) => {
                panic!("a streaming response body has no buffered form; read it as a stream")
            }
        }
    }

    async fn collect(self) -> Result<Vec<u8>> {
        match self {
            ResponseBody::Empty => Ok(Vec::new()),
            ResponseBody::Body(bytes) => Ok(bytes),
            #[cfg(feature = "p3")]
            ResponseBody::Stream(stream) => stream.collect_bytes().await,
        }
    }
}

/// Whether the host is allowed to encode the body. Carried for source
/// compatibility; OpenWorkers never re-encodes what a worker returns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EncodeBody {
    #[default]
    Automatic,
    Manual,
}

#[derive(Debug, Clone)]
pub struct Response {
    body: ResponseBody,
    init: ResponseBuilder,
}

impl Response {
    pub fn builder() -> ResponseBuilder {
        ResponseBuilder::new()
    }

    pub fn from_json<B: Serialize>(value: &B) -> Result<Self> {
        ResponseBuilder::new().from_json(value)
    }

    pub fn from_html(html: impl AsRef<str>) -> Result<Self> {
        ResponseBuilder::new().from_html(html)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        ResponseBuilder::new().from_bytes(bytes)
    }

    pub fn from_body(body: ResponseBody) -> Result<Self> {
        Ok(ResponseBuilder::new().body(body))
    }

    pub fn from_stream<S>(stream: S) -> Result<Self>
    where
        S: futures_util::Stream<Item = Result<Vec<u8>>> + 'static,
    {
        ResponseBuilder::new().from_stream(stream)
    }

    pub fn ok(body: impl Into<String>) -> Result<Self> {
        ResponseBuilder::new().ok(body)
    }

    pub fn empty() -> Result<Self> {
        Ok(ResponseBuilder::new().empty())
    }

    pub fn error(msg: impl Into<String>, status: u16) -> Result<Self> {
        ResponseBuilder::new().error(msg, status)
    }

    pub fn redirect(url: url::Url) -> Result<Self> {
        Response::redirect_with_status(url, 302)
    }

    pub fn redirect_with_status(url: url::Url, status_code: u16) -> Result<Self> {
        if !(300..=399).contains(&status_code) {
            return Err(Error::RustError(format!(
                "redirect: status must be in the 3xx range, got {status_code}"
            )));
        }

        ResponseBuilder::new()
            .with_status(status_code)
            .with_header("location", url.as_str())?
            .from_bytes(Vec::new())
    }

    pub fn status_code(&self) -> u16 {
        self.init.status_code
    }

    pub fn body(&self) -> &ResponseBody {
        &self.body
    }

    pub async fn text(&mut self) -> Result<String> {
        String::from_utf8(self.bytes().await?).map_err(Error::from)
    }

    pub async fn json<B: DeserializeOwned>(&mut self) -> Result<B> {
        serde_json::from_slice(&self.bytes().await?).map_err(Error::SerdeJsonError)
    }

    pub async fn bytes(&mut self) -> Result<Vec<u8>> {
        std::mem::take(&mut self.body).collect().await
    }

    pub fn stream(&mut self) -> Result<ByteStream> {
        match std::mem::take(&mut self.body) {
            #[cfg(feature = "p3")]
            ResponseBody::Stream(stream) => Ok(stream),
            body => Ok(ByteStream::new(body.into_bytes())),
        }
    }

    pub fn with_headers(mut self, headers: Headers) -> Self {
        self.init.headers = headers;
        self
    }

    pub fn with_status(mut self, status_code: u16) -> Self {
        self.init.status_code = status_code;
        self
    }

    pub fn encode_body(&self) -> &EncodeBody {
        &self.init.encode_body
    }

    pub fn with_encode_body(mut self, encode_body: EncodeBody) -> Self {
        self.init.encode_body = encode_body;
        self
    }

    pub fn headers(&self) -> &Headers {
        &self.init.headers
    }

    pub fn headers_mut(&mut self) -> &mut Headers {
        &mut self.init.headers
    }

    pub fn into_parts(self) -> (ResponseBuilder, ResponseBody) {
        (self.init, self.body)
    }

    /// A second response carrying the same body, leaving this one readable.
    pub fn cloned(&mut self) -> Result<Self> {
        Ok(Clone::clone(self))
    }
}

/// Everything about a response except its body.
#[derive(Debug, Clone)]
pub struct ResponseBuilder {
    pub status_code: u16,
    pub headers: Headers,
    pub encode_body: EncodeBody,
}

impl ResponseBuilder {
    pub fn new() -> Self {
        ResponseBuilder {
            status_code: 200,
            headers: Headers::new(),
            encode_body: EncodeBody::Automatic,
        }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status_code = status;
        self
    }

    pub fn with_headers(mut self, headers: Headers) -> Self {
        self.headers = headers;
        self
    }

    pub fn with_header(self, key: &str, value: &str) -> Result<Self> {
        self.headers.set(key, value)?;
        Ok(self)
    }

    pub fn with_encode_body(mut self, encode_body: EncodeBody) -> Self {
        self.encode_body = encode_body;
        self
    }

    pub fn fixed(self, body: Vec<u8>) -> Response {
        Response {
            body: ResponseBody::Body(body),
            init: self,
        }
    }

    pub fn body(self, body: ResponseBody) -> Response {
        Response { body, init: self }
    }

    pub fn empty(self) -> Response {
        Response {
            body: ResponseBody::Empty,
            init: self,
        }
    }

    pub fn from_json<B: Serialize>(self, value: &B) -> Result<Response> {
        let Ok(data) = serde_json::to_string(value) else {
            return Err(Error::Json(("Failed to encode data to json".into(), 500)));
        };

        Ok(self
            .with_header("content-type", "application/json")?
            .fixed(data.into_bytes()))
    }

    pub fn from_html(self, html: impl AsRef<str>) -> Result<Response> {
        let data = html.as_ref().as_bytes().to_vec();

        Ok(self
            .with_header("content-type", "text/html; charset=utf-8")?
            .fixed(data))
    }

    pub fn from_bytes(self, bytes: Vec<u8>) -> Result<Response> {
        Ok(self.fixed(bytes))
    }

    #[cfg(not(feature = "p3"))]
    pub fn from_stream<S>(self, stream: S) -> Result<Response>
    where
        S: futures_util::Stream<Item = Result<Vec<u8>>> + 'static,
    {
        let chunks = crate::rt::poll_to_end(futures_util::StreamExt::collect::<Vec<_>>(stream))
            .ok_or_else(|| {
                Error::RustError("response stream parked with nothing to wake it".into())
            })?;
        let mut body = Vec::new();

        for chunk in chunks {
            body.extend_from_slice(&chunk?);
        }

        Ok(self.fixed(body))
    }

    #[cfg(feature = "p3")]
    pub fn from_stream<S>(self, stream: S) -> Result<Response>
    where
        S: futures_util::Stream<Item = Result<Vec<u8>>> + 'static,
    {
        Ok(self.body(ResponseBody::Stream(ByteStream::boxed(stream))))
    }

    pub fn ok(self, body: impl Into<String>) -> Result<Response> {
        Ok(self.fixed(body.into().into_bytes()))
    }

    pub fn error(self, msg: impl Into<String>, status: u16) -> Result<Response> {
        if !(400..=599).contains(&status) {
            return Err(Error::RustError(
                "error status codes must be in the 4xx to 5xx range".into(),
            ));
        }

        Ok(self.with_status(status).fixed(msg.into().into_bytes()))
    }
}

impl Default for ResponseBuilder {
    fn default() -> Self {
        ResponseBuilder::new()
    }
}

/// Any type a fetch handler can return as its response.
pub trait IntoResponse {
    fn into_raw(self) -> std::result::Result<Response, impl Into<Box<dyn std::error::Error>>>;
}

impl IntoResponse for Response {
    fn into_raw(self) -> std::result::Result<Response, impl Into<Box<dyn std::error::Error>>> {
        Ok::<Response, Error>(self)
    }
}

impl<B: http_body::Body<Data = Bytes> + 'static> IntoResponse for http::Response<B> {
    fn into_raw(self) -> std::result::Result<Response, impl Into<Box<dyn std::error::Error>>> {
        Response::try_from(self)
    }
}

impl<B: http_body::Body<Data = Bytes> + 'static> TryFrom<http::Response<B>> for Response {
    type Error = Error;

    fn try_from(response: http::Response<B>) -> Result<Self> {
        let (parts, body) = response.into_parts();
        let body = crate::http_body::collect(body)?;

        Ok(ResponseBuilder::new()
            .with_status(parts.status.as_u16())
            .with_headers(Headers::from(&parts.headers))
            .fixed(body.to_vec()))
    }
}

impl TryFrom<Response> for http::Response<crate::http_body::Body> {
    type Error = Error;

    fn try_from(response: Response) -> Result<Self> {
        let (init, body) = response.into_parts();

        let mut builder = http::Response::builder().status(init.status_code);

        for (name, value) in init.headers.entries() {
            builder = builder.header(name, value);
        }

        let body = match body {
            ResponseBody::Empty => crate::http_body::Body::empty(),
            ResponseBody::Body(bytes) => crate::http_body::Body::from(bytes),
            #[cfg(feature = "p3")]
            ResponseBody::Stream(stream) => crate::http_body::Body::from_stream(stream),
        };

        builder.body(body).map_err(Error::Http)
    }
}
