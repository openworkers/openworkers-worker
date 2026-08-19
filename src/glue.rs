//! What `#[event]` expands into: the bridge between a WASI export and an
//! async handler.

use std::future::Future;

use url::Url;

use crate::fetch::fields_to_headers;
use crate::fetch::read_body;
use crate::fetch::write_body;
use crate::rt;
use crate::wit::wasi::http::types::Fields;
use crate::wit::wasi::http::types::IncomingRequest;
use crate::wit::wasi::http::types::OutgoingBody;
use crate::wit::wasi::http::types::OutgoingResponse;
use crate::wit::wasi::http::types::ResponseOutparam;
use crate::wit::wasi::http::types::Scheme;
use crate::Context;
use crate::Env;
use crate::Error;
use crate::FromRequest;
use crate::IntoResponse;
use crate::Method;
use crate::Request;
use crate::Response;
use crate::Result;
use crate::ScheduleContext;
use crate::ScheduledEvent;

/// Drives a `#[event(fetch)]` handler for one request.
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

    Ok(Request::from_parts(method, url, headers, Some(bytes)))
}

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
