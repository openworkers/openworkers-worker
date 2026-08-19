//! The hello example built for the WASI 0.3 world, served by the real host.
//! Same application code as hello_test; only the `p3` feature differs.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use openworkers_core::Event;
use openworkers_core::HttpMethod;
use openworkers_core::HttpRequest;
use openworkers_core::HttpResponse;
use openworkers_core::LogLevel;
use openworkers_core::OpFuture;
use openworkers_core::OperationsHandler;
use openworkers_core::RequestBody;
use openworkers_core::ResponseBody;
use openworkers_core::RuntimeLimits;
use openworkers_core::Script;
use openworkers_core::WorkerCode;
use openworkers_runtime_wasm::WasmWorker;

/// Loads the wasm the example built with `--features p3` into its own
/// target directory, so the 0.2 artifact stays untouched.
fn load_p3_component(example: &str) -> Vec<u8> {
    let path = format!(
        "{}/../examples/{example}/target-p3/wasm32-wasip2/release/{}.wasm",
        env!("CARGO_MANIFEST_DIR"),
        example.replace('-', "_")
    );

    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "could not read {path}: {e}\n\
             build it with: cd examples/{example} && \
             CARGO_TARGET_DIR=target-p3 cargo build --target wasm32-wasip2 --release --features p3"
        )
    })
}

fn script(env: Option<HashMap<String, String>>) -> Script {
    Script {
        code: WorkerCode::WebAssembly(load_p3_component("hello")),
        env,
        bindings: vec![],
    }
}

fn get(url: &str) -> HttpRequest {
    HttpRequest {
        url: url.to_string(),
        method: HttpMethod::Get,
        headers: HashMap::new(),
        body: RequestBody::None,
    }
}

fn post(url: &str, body: &[u8]) -> HttpRequest {
    HttpRequest {
        url: url.to_string(),
        method: HttpMethod::Post,
        headers: HashMap::new(),
        body: RequestBody::Bytes(bytes::Bytes::copy_from_slice(body)),
    }
}

fn body_of(response: &HttpResponse) -> String {
    match &response.body {
        ResponseBody::Bytes(bytes) => String::from_utf8_lossy(bytes).to_string(),
        ResponseBody::None => String::new(),
        other => panic!("expected a buffered body, got {other:?}"),
    }
}

struct MockOps {
    logs: Mutex<Vec<(LogLevel, String)>>,
    fetched: Mutex<Vec<String>>,
}

impl MockOps {
    fn new() -> Arc<Self> {
        Arc::new(MockOps {
            logs: Mutex::new(vec![]),
            fetched: Mutex::new(vec![]),
        })
    }
}

impl OperationsHandler for MockOps {
    fn handle_fetch(&self, request: HttpRequest) -> OpFuture<'_, Result<HttpResponse, String>> {
        self.fetched.lock().unwrap().push(request.url);

        Box::pin(async {
            Ok(HttpResponse {
                status: 200,
                headers: vec![],
                body: ResponseBody::Bytes(bytes::Bytes::from("mock upstream body")),
            })
        })
    }

    fn handle_log(&self, level: LogLevel, message: String) {
        self.logs.lock().unwrap().push((level, message));
    }
}

async fn serve(worker: &mut WasmWorker, request: HttpRequest) -> HttpResponse {
    let (event, rx) = Event::fetch(request);
    worker.exec(event).await.expect("Failed to execute event");
    rx.await.expect("Failed to receive response")
}

#[tokio::test]
async fn test_p3_hello_serves_the_default_route() {
    let env = HashMap::from([("GREETING".to_string(), "Bonjour".to_string())]);

    let mut worker = WasmWorker::new(script(Some(env)), None, None)
        .await
        .expect("Failed to create worker");

    let response = serve(&mut worker, get("https://example.com/")).await;

    assert_eq!(response.status, 200);
    assert!(body_of(&response).contains("Bonjour from openworkers-worker!"));
}

#[tokio::test]
async fn test_p3_echo_round_trips_a_body() {
    let mut worker = WasmWorker::new(script(None), None, None)
        .await
        .expect("Failed to create worker");

    let response = serve(&mut worker, post("https://example.com/echo", b"round trip")).await;

    assert_eq!(response.status, 200);
    assert_eq!(body_of(&response), "round trip");
}

/// The oneshot bridge a workers-rs application uses for `Send` futures:
/// a spawned task completes the channel the handler is awaiting. On the 0.3
/// world wit-bindgen's executor does the wakeup.
#[tokio::test]
async fn test_p3_bridge_resumes_the_handler() {
    let mut worker = WasmWorker::new(script(None), None, None)
        .await
        .expect("Failed to create worker");

    let response = serve(&mut worker, get("https://example.com/bridge")).await;

    assert_eq!(response.status, 200);
    assert_eq!(body_of(&response), "bridged");
}

/// `wait_until` work runs after the response and the host waits for it:
/// the log line it emits is there when exec returns.
#[tokio::test]
async fn test_p3_wait_until_runs_after_the_response() {
    let ops = MockOps::new();

    let mut worker = WasmWorker::new_with_ops(script(None), None, ops.clone())
        .await
        .expect("Failed to create worker");

    let response = serve(&mut worker, get("https://example.com/wait-until")).await;

    assert_eq!(body_of(&response), "queued");

    let logs = ops.logs.lock().unwrap();
    assert!(
        logs.iter()
            .any(|(_, message)| message.contains("wait_until ran")),
        "the wait_until future should have completed, got: {logs:?}"
    );
}

#[tokio::test]
async fn test_p3_outbound_fetch_flows_through_ops() {
    let ops = MockOps::new();

    let mut worker = WasmWorker::new_with_ops(script(None), None, ops.clone())
        .await
        .expect("Failed to create worker");

    let response = serve(&mut worker, get("https://example.com/proxy")).await;

    assert_eq!(response.status, 200);
    assert_eq!(body_of(&response), "mock upstream body");

    let fetched = ops.fetched.lock().unwrap();
    assert_eq!(fetched.as_slice(), ["https://upstream.example/data"]);
}

#[tokio::test]
async fn test_p3_response_headers_come_through() {
    let mut worker = WasmWorker::new(script(None), None, None)
        .await
        .expect("Failed to create worker");

    let response = serve(&mut worker, get("https://example.com/headers")).await;

    assert!(
        response
            .headers
            .iter()
            .any(|(name, value)| name == "x-seen-host" && value == "example.com"),
        "expected the x-seen-host header, got {:?}",
        response.headers
    );
}

/// 32 MiB of request body flow through a guest capped at 8 MiB, because the
/// SDK reads `req.stream()` chunk by chunk off the wire. The same route on
/// the 0.2 build buffers and dies (hello_test covers the buffered read).
#[tokio::test]
async fn test_p3_streams_a_large_request_under_a_tight_cap() {
    let limits = RuntimeLimits {
        heap_max_mb: 8,
        max_cpu_time_ms: 0,
        ..Default::default()
    };

    let mut worker = WasmWorker::new(script(None), Some(limits), None)
        .await
        .expect("Failed to create worker");

    let payload = vec![b'a'; 32 * 1024 * 1024];
    let response = serve(
        &mut worker,
        post("https://example.com/stream-sum", &payload),
    )
    .await;

    assert_eq!(response.status, 200);
    assert_eq!(body_of(&response), (32 * 1024 * 1024).to_string());
}

/// The same cap, the other direction: `Response::from_stream` hands chunks
/// to the host as they are produced.
#[tokio::test]
async fn test_p3_streams_a_large_response_under_a_tight_cap() {
    let limits = RuntimeLimits {
        heap_max_mb: 8,
        max_cpu_time_ms: 0,
        ..Default::default()
    };

    let mut worker = WasmWorker::new(script(None), Some(limits), None)
        .await
        .expect("Failed to create worker");

    let response = serve(&mut worker, get("https://example.com/stream-out?mb=32")).await;

    assert_eq!(response.status, 200);

    let ResponseBody::Bytes(body) = &response.body else {
        panic!("expected a buffered body");
    };

    assert_eq!(body.len(), 32 * 1024 * 1024);
}

/// The p3 build still exports the 0.2 scheduled handler; both generations
/// live in one component.
#[tokio::test]
async fn test_p3_scheduled_still_runs() {
    let ops = MockOps::new();

    let mut worker = WasmWorker::new_with_ops(script(None), None, ops.clone())
        .await
        .expect("Failed to create worker");

    let (event, rx) = Event::from_schedule("test-task".to_string(), 1_234_567_890);
    worker.exec(event).await.expect("Failed to execute event");
    rx.await.expect("Failed to receive task result");

    let logs = ops.logs.lock().unwrap();
    assert!(
        logs.iter()
            .any(|(_, message)| message.contains("scheduled at")),
        "the scheduled handler should have logged, got: {logs:?}"
    );
}
