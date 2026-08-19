//! What `#[event]` expands into: the bridge between a WASI export and an
//! async handler.

use std::future::Future;

#[cfg(not(feature = "p3"))]
use url::Url;

#[cfg(not(feature = "p3"))]
use crate::fetch::fields_to_headers;
#[cfg(not(feature = "p3"))]
use crate::fetch::read_body;
#[cfg(not(feature = "p3"))]
use crate::fetch::write_body;
use crate::rt;
#[cfg(not(feature = "p3"))]
use crate::wit::wasi::http0_2_0::types::Fields;
#[cfg(not(feature = "p3"))]
use crate::wit::wasi::http0_2_0::types::IncomingRequest;
#[cfg(not(feature = "p3"))]
use crate::wit::wasi::http0_2_0::types::OutgoingBody;
#[cfg(not(feature = "p3"))]
use crate::wit::wasi::http0_2_0::types::OutgoingResponse;
#[cfg(not(feature = "p3"))]
use crate::wit::wasi::http0_2_0::types::ResponseOutparam;
#[cfg(not(feature = "p3"))]
use crate::wit::wasi::http0_2_0::types::Scheme;
#[cfg(not(feature = "p3"))]
use crate::Context;
use crate::Env;
use crate::Error;
use crate::FromRequest;
#[cfg(not(feature = "p3"))]
use crate::IntoResponse;
#[cfg(not(feature = "p3"))]
use crate::Method;
use crate::Request;
use crate::Response;
use crate::Result;
use crate::ScheduleContext;
use crate::ScheduledEvent;

/// Drives a `#[event(fetch)]` handler for one request.
#[cfg(not(feature = "p3"))]
pub fn serve_fetch<Req, Fut, Res, Err, F>(
    request: IncomingRequest,
    response_out: ResponseOutparam,
    respond_with_errors: bool,
    handler: F,
) where
    Req: FromRequest,
    F: FnOnce(Req, Env, Context) -> Fut,
    Fut: Future<Output = std::result::Result<Res, Err>>,
    Res: IntoResponse,
    Err: Into<Box<dyn std::error::Error>>,
{
    crate::panic_hook::set_once();

    let response = match incoming(request).and_then(convert::<Req>) {
        Ok(request) => run(handler, request, respond_with_errors),
        Err(e) => failed(&e.to_string(), respond_with_errors),
    };

    emit(response_out, response);

    rt::drain_tasks();
}

/// Drives a `#[event(scheduled)]` handler for one cron tick.
pub fn serve_scheduled<Fut, F>(scheduled_time: u64, handler: F)
where
    F: FnOnce(ScheduledEvent, Env, ScheduleContext) -> Fut,
    Fut: Future<Output = ()>,
{
    crate::panic_hook::set_once();

    rt::block_on(handler(
        ScheduledEvent::new(scheduled_time),
        Env,
        ScheduleContext,
    ));

    rt::drain_tasks();
}

#[cfg(not(feature = "p3"))]
fn run<Req, Fut, Res, Err, F>(handler: F, request: Req, respond_with_errors: bool) -> Response
where
    F: FnOnce(Req, Env, Context) -> Fut,
    Fut: Future<Output = std::result::Result<Res, Err>>,
    Res: IntoResponse,
    Err: Into<Box<dyn std::error::Error>>,
{
    match rt::block_on(handler(request, Env, Context)) {
        Ok(response) => match response.into_raw() {
            Ok(response) => response,
            Err(e) => failed(&e.into().to_string(), respond_with_errors),
        },
        Err(e) => failed(&e.into().to_string(), respond_with_errors),
    }
}

/// A handler that failed still owes the caller a response.
fn failed(message: &str, respond_with_errors: bool) -> Response {
    crate::console_error!("{message}");

    let body = if respond_with_errors {
        message
    } else {
        "INTERNAL SERVER ERROR"
    };

    Response::error(body, 500).expect("500 is a valid error status")
}

fn convert<Req: FromRequest>(request: Request) -> Result<Req> {
    Req::from_raw(request).map_err(|e| Error::RustError(e.into().to_string()))
}

/// Reads the whole incoming request, body included: the host buffers bodies
/// at the boundary, so there is nothing to stream through.
#[cfg(not(feature = "p3"))]
fn incoming(request: IncomingRequest) -> Result<Request> {
    let method = Method::from(request.method());
    let headers = fields_to_headers(&request.headers());

    let scheme = match request.scheme() {
        Some(Scheme::Http) => "http".to_string(),
        Some(Scheme::Other(other)) => other,
        _ => "https".to_string(),
    };

    let authority = request
        .authority()
        .unwrap_or_else(|| "localhost".to_string());
    let target = request.path_with_query().unwrap_or_else(|| "/".to_string());

    let url = Url::parse(&format!("{scheme}://{authority}{target}"))?;

    let body = request
        .consume()
        .map_err(|_| Error::RustError("request body taken twice".into()))?;

    let bytes = read_body(body)?;

    Ok(Request::from_parts(
        method,
        url,
        headers,
        Some(crate::request::BodySource::Buffered(bytes)),
    ))
}

#[cfg(not(feature = "p3"))]
fn emit(response_out: ResponseOutparam, response: Response) {
    let status = response.status_code();
    let (init, body) = response.into_parts();

    let headers = Fields::try_from(&init.headers).unwrap_or_else(|_| Fields::new());

    let outgoing = OutgoingResponse::new(headers);
    let _ = outgoing.set_status_code(status);

    let Ok(outgoing_body) = outgoing.body() else {
        return;
    };

    ResponseOutparam::set(response_out, Ok(outgoing));

    let bytes = match body {
        crate::ResponseBody::Empty => Vec::new(),
        crate::ResponseBody::Body(bytes) => bytes,
    };

    if let Err(e) = write_body(&outgoing_body, &bytes) {
        crate::console_error!("{e}");
    }

    let _ = OutgoingBody::finish(outgoing_body, None);
}

#[cfg(feature = "p3")]
mod v3 {
    use std::future::Future;

    use futures_util::StreamExt;
    use url::Url;

    use crate::request::BodySource;
    use crate::streams::ByteStream;
    use crate::streams::IncomingStream;
    use crate::wit_v3::wasi::http0_3_0::types::ErrorCode;
    use crate::wit_v3::wasi::http0_3_0::types::Fields;
    use crate::wit_v3::wasi::http0_3_0::types::Request as WitRequest;
    use crate::wit_v3::wasi::http0_3_0::types::Response as WitResponse;
    use crate::wit_v3::wasi::http0_3_0::types::Scheme;
    use crate::wit_v3::wit_future;
    use crate::wit_v3::wit_stream;
    use crate::Context;
    use crate::Env;
    use crate::FromRequest;
    use crate::Headers;
    use crate::IntoResponse;
    use crate::Method;
    use crate::Request;
    use crate::Response;
    use crate::ResponseBody;
    use crate::Result;

    use super::convert;
    use super::failed;

    /// Drives a `#[event(fetch)]` handler for one request on the 0.3 world.
    /// The export is natively async, so the handler is awaited in place; the
    /// response body and the `wait_until` drain run on after returning.
    pub async fn serve_fetch_v3<Req, Fut, Res, Err, F>(
        request: WitRequest,
        respond_with_errors: bool,
        handler: F,
    ) -> std::result::Result<WitResponse, ErrorCode>
    where
        Req: FromRequest,
        F: FnOnce(Req, Env, Context) -> Fut,
        Fut: Future<Output = std::result::Result<Res, Err>>,
        Res: IntoResponse,
        Err: Into<Box<dyn std::error::Error>>,
    {
        crate::panic_hook::set_once();

        let response = match incoming(request).and_then(convert::<Req>) {
            Ok(request) => match handler(request, Env, Context).await {
                Ok(response) => match response.into_raw() {
                    Ok(response) => response,
                    Err(e) => failed(&e.into().to_string(), respond_with_errors),
                },
                Err(e) => failed(&e.into().to_string(), respond_with_errors),
            },
            Err(e) => failed(&e.to_string(), respond_with_errors),
        };

        Ok(emit(response))
    }

    /// Turns the wit request into ours; the body stays a stream.
    fn incoming(request: WitRequest) -> Result<Request> {
        let method = Method::from(request.get_method());
        let headers = fields_to_headers(&request.get_headers());

        let scheme = match request.get_scheme() {
            Some(Scheme::Http) => "http".to_string(),
            Some(Scheme::Other(other)) => other,
            _ => "https".to_string(),
        };

        let authority = request
            .get_authority()
            .unwrap_or_else(|| "localhost".to_string());
        let target = request
            .get_path_with_query()
            .unwrap_or_else(|| "/".to_string());

        let url = Url::parse(&format!("{scheme}://{authority}{target}"))?;

        // Dropping the writer resolves the future with its default Ok, which
        // tells the host body processing raised no error
        let (done_tx, done_rx) = wit_future::new(|| Ok(()));
        drop(done_tx);

        let (reader, _trailers) = WitRequest::consume_body(request, done_rx);
        let body = BodySource::Streamed(ByteStream::live(IncomingStream::new(reader)));

        Ok(Request::from_parts(method, url, headers, Some(body)))
    }

    /// Builds the wit response and hands the body to a task: the host only
    /// starts reading after `handle` returns. The trailers resolve once the
    /// body and every `wait_until` future are done, which is what lets the
    /// host wait for post-response work.
    fn emit(response: Response) -> WitResponse {
        let status = response.status_code();
        let (init, body) = response.into_parts();

        let headers = fields_from_headers(&init.headers).unwrap_or_else(|_| Fields::new());

        let (mut body_tx, body_rx) = wit_stream::new::<u8>();
        let (trailers_tx, trailers_rx) = wit_future::new(|| Ok(None));
        let (outgoing, _transmit) = WitResponse::new(headers, Some(body_rx), trailers_rx);

        let _ = outgoing.set_status_code(status);

        wit_bindgen::spawn_local(async move {
            match body {
                ResponseBody::Empty => {}
                ResponseBody::Body(bytes) => {
                    let _ = body_tx.write_all(bytes).await;
                }
                ResponseBody::Stream(mut stream) => {
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(chunk) => {
                                let _ = body_tx.write_all(chunk).await;
                            }
                            Err(e) => {
                                crate::console_error!("response stream failed: {e}");
                                break;
                            }
                        }
                    }
                }
            }

            drop(body_tx);

            crate::rt_v3::drain_wait_until().await;

            let _ = trailers_tx.write(Ok(None)).await;
        });

        outgoing
    }

    pub(crate) fn fields_to_headers(fields: &Fields) -> Headers {
        let headers = Headers::new();

        for (name, value) in fields.copy_all() {
            if let Ok(value) = String::from_utf8(value) {
                let _ = headers.append(&name, &value);
            }
        }

        headers
    }

    pub(crate) fn fields_from_headers(headers: &Headers) -> Result<Fields> {
        let entries: Vec<(String, Vec<u8>)> = headers
            .entries()
            .map(|(name, value)| (name, value.into_bytes()))
            .collect();

        Fields::from_list(&entries)
            .map_err(|e| crate::Error::RustError(format!("invalid headers: {e:?}")))
    }
}

#[cfg(feature = "p3")]
pub use v3::serve_fetch_v3;

#[cfg(feature = "p3")]
pub(crate) use v3::fields_from_headers;
#[cfg(feature = "p3")]
pub(crate) use v3::fields_to_headers as fields_to_headers_v3;
