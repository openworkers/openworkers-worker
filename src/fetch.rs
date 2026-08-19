//! Outbound requests, through `wasi:http/outgoing-handler`.

use url::Url;

use crate::wit::wasi::http::outgoing_handler;
use crate::wit::wasi::http::types::Fields;
use crate::wit::wasi::http::types::IncomingBody;
use crate::wit::wasi::http::types::OutgoingBody;
use crate::wit::wasi::http::types::OutgoingRequest;
use crate::wit::wasi::http::types::Scheme;
use crate::Error;
use crate::Headers;
use crate::Method;
use crate::Request;
use crate::Response;
use crate::ResponseBuilder;
use crate::Result;

/// `wasi:io` caps `blocking-write-and-flush` at this many bytes
const CHUNK_SIZE: usize = 4096;

/// Either end of a `fetch` call: a bare URL or a prepared request.
#[derive(Debug, Clone)]
pub enum Fetch {
    Url(Url),
    Request(Request),
}

impl Fetch {
    pub async fn send(&self) -> Result<Response> {
        match self {
            Fetch::Url(url) => send(Request::new(url.as_str(), Method::Get)?).await,
            Fetch::Request(request) => send(request.clone()?).await,
        }
    }

    /// The signal is accepted and ignored: the host runs a request to
    /// completion and enforces its own deadline.
    pub async fn send_with_signal(&self, _signal: &AbortSignal) -> Result<Response> {
        self.send().await
    }
}

/// An abort handle, kept so that [`Fetch::send_with_signal`] has an argument
/// type. Nothing on this platform listens to it.
#[derive(Debug, Clone, Default)]
pub struct AbortSignal;

async fn send(mut request: Request) -> Result<Response> {
    let url = request.url()?;
    let body = request.bytes().await.unwrap_or_default();

    let headers = Fields::try_from(request.headers())?;
    let outgoing = OutgoingRequest::new(headers);

    outgoing
        .set_method(&(&request.method()).into())
        .map_err(|_| Error::RustError("fetch: unsupported method".into()))?;

    let scheme = match url.scheme() {
        "http" => Scheme::Http,
        "https" => Scheme::Https,
        other => Scheme::Other(other.to_string()),
    };

    outgoing
        .set_scheme(Some(&scheme))
        .map_err(|_| Error::RustError("fetch: unsupported scheme".into()))?;

    let authority = match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_string(),
    };

    outgoing
        .set_authority(Some(&authority))
        .map_err(|_| Error::RustError(format!("fetch: invalid authority `{authority}`")))?;

    let target = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_string(),
    };

    outgoing
        .set_path_with_query(Some(&target))
        .map_err(|_| Error::RustError(format!("fetch: invalid path `{target}`")))?;

    let outgoing_body = outgoing
        .body()
        .map_err(|_| Error::RustError("fetch: request body taken twice".into()))?;

    let pending = outgoing_handler::handle(outgoing, None);

    write_body(&outgoing_body, &body)?;
    OutgoingBody::finish(outgoing_body, None)
        .map_err(|e| Error::RustError(format!("fetch: {e}")))?;

    let pending = pending.map_err(|e| Error::RustError(format!("fetch: {e}")))?;

    pending.subscribe().block();

    let incoming = pending
        .get()
        .ok_or_else(|| Error::RustError("fetch: response was not ready".into()))?
        .map_err(|_| Error::RustError("fetch: response taken twice".into()))?
        .map_err(|e| Error::RustError(format!("fetch: {e}")))?;

    let status = incoming.status();
    let headers = fields_to_headers(&incoming.headers());
    let body = incoming
        .consume()
        .map_err(|_| Error::RustError("fetch: response body taken twice".into()))?;

    let bytes = read_body(body)?;

    Ok(ResponseBuilder::new()
        .with_status(status)
        .with_headers(headers)
        .fixed(bytes))
}

pub(crate) fn fields_to_headers(fields: &Fields) -> Headers {
    let headers = Headers::new();

    for (name, value) in fields.entries() {
        if let Ok(value) = String::from_utf8(value) {
            let _ = headers.append(&name, &value);
        }
    }

    headers
}

/// Reads an incoming body to its end; `blocking-read` reports the end as an
/// error, which is why the loop stops on `Err` rather than reporting it.
pub(crate) fn read_body(body: IncomingBody) -> Result<Vec<u8>> {
    let stream = body
        .stream()
        .map_err(|_| Error::RustError("body stream taken twice".into()))?;

    let mut bytes = Vec::new();

    while let Ok(chunk) = stream.blocking_read(CHUNK_SIZE as u64) {
        bytes.extend_from_slice(&chunk);
    }

    Ok(bytes)
}

pub(crate) fn write_body(body: &OutgoingBody, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() {
        return Ok(());
    }

    let stream = body
        .write()
        .map_err(|_| Error::RustError("body stream taken twice".into()))?;

    for chunk in bytes.chunks(CHUNK_SIZE) {
        stream
            .blocking_write_and_flush(chunk)
            .map_err(|e| Error::RustError(format!("body write failed: {e:?}")))?;
    }

    Ok(())
}
