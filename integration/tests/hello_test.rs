//! The hello example, served by the real host.

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
use openworkers_core::Script;
use openworkers_core::WorkerCode;
use openworkers_runtime_wasm::WasmWorker;

/// Loads the component an example crate built.
fn load_component(example: &str) -> Vec<u8> {
    let path = format!(
        "{}/../examples/{example}/target/wasm32-wasip2/release/{}.wasm",
        env!("CARGO_MANIFEST_DIR"),
        example.replace('-', "_")
    );

    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "could not read {path}: {e}\n\
             build it with: cd examples/{example} && cargo build --target wasm32-wasip2 --release"
        )
    })
}

fn script(example: &str, env: Option<HashMap<String, String>>) -> Script {
    Script {
        code: WorkerCode::WebAssembly(load_component(example)),
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

fn body_of(response: &openworkers_core::HttpResponse) -> String {
    match &response.body {
        ResponseBody::Bytes(bytes) => String::from_utf8_lossy(bytes).to_string(),
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

#[tokio::test]
async fn fetch_handler_answers_with_the_environment() {
    let env = HashMap::from([("GREETING".to_string(), "Bonjour".to_string())]);
    let mut worker = WasmWorker::new(script("hello", Some(env)), None, None)
        .await
        .expect("worker");

    let (event, rx) = Event::fetch(get("https://example.com/test"));
    worker.exec(event).await.expect("exec");

    let response = rx.await.expect("response");

    assert_eq!(response.status, 200);

    let body = body_of(&response);

    assert!(body.contains("Bonjour"), "greeting missing from {body}");
    assert!(body.contains("example.com/test"), "url missing from {body}");
}

#[tokio::test]
async fn request_body_round_trips() {
    let mut worker = WasmWorker::new(script("hello", None), None, None)
        .await
        .expect("worker");

    let request = HttpRequest {
        url: "https://example.com/echo".to_string(),
        method: HttpMethod::Post,
        headers: HashMap::new(),
        body: RequestBody::Bytes(bytes::Bytes::from_static(b"round trip")),
    };

    let (event, rx) = Event::fetch(request);
    worker.exec(event).await.expect("exec");

    let response = rx.await.expect("response");

    assert_eq!(response.status, 200);
    assert_eq!(body_of(&response), "round trip");
}

#[tokio::test]
async fn outbound_fetch_goes_through_the_host() {
    let ops = MockOps::new();
    let mut worker = WasmWorker::new_with_ops(script("hello", None), None, ops.clone())
        .await
        .expect("worker");

    let (event, rx) = Event::fetch(get("https://example.com/proxy"));
    worker.exec(event).await.expect("exec");

    let response = rx.await.expect("response");

    assert_eq!(response.status, 200);
    assert_eq!(body_of(&response), "mock upstream body");
    assert_eq!(
        ops.fetched.lock().unwrap().as_slice(),
        ["https://upstream.example/data"]
    );
}

#[tokio::test]
async fn response_headers_reach_the_host() {
    let mut worker = WasmWorker::new(script("hello", None), None, None)
        .await
        .expect("worker");

    let (event, rx) = Event::fetch(get("https://example.com/headers"));
    worker.exec(event).await.expect("exec");

    let response = rx.await.expect("response");

    assert!(
        response
            .headers
            .iter()
            .any(|(name, value)| name == "x-seen-host" && value == "example.com"),
        "header missing from {:?}",
        response.headers
    );
}

#[tokio::test]
async fn wait_until_runs_before_the_export_returns() {
    let ops = MockOps::new();
    let mut worker = WasmWorker::new_with_ops(script("hello", None), None, ops.clone())
        .await
        .expect("worker");

    let (event, rx) = Event::fetch(get("https://example.com/wait-until"));
    worker.exec(event).await.expect("exec");
    rx.await.expect("response");

    assert!(
        ops.logs
            .lock()
            .unwrap()
            .iter()
            .any(|(_, message)| message.contains("wait_until ran")),
        "wait_until never ran: {:?}",
        ops.logs.lock().unwrap()
    );
}

#[tokio::test]
async fn scheduled_handler_runs() {
    let ops = MockOps::new();
    let mut worker = WasmWorker::new_with_ops(script("hello", None), None, ops.clone())
        .await
        .expect("worker");

    let (event, rx) = Event::from_schedule("test-task".to_string(), 1_234_567_890);
    worker.exec(event).await.expect("exec");

    assert!(rx.await.expect("task result").success);
    assert!(
        ops.logs
            .lock()
            .unwrap()
            .iter()
            .any(|(_, message)| message.contains("scheduled at")),
        "cron log missing: {:?}",
        ops.logs.lock().unwrap()
    );
}

#[tokio::test]
async fn scheduled_handler_gets_the_cron() {
    let ops = MockOps::new();
    let mut worker = WasmWorker::new_with_ops(script("hello", None), None, ops.clone())
        .await
        .expect("worker");

    let source = openworkers_core::TaskSource::Schedule {
        time: 1_234_567_890,
        cron: Some("*/5 * * * *".to_string()),
    };
    let (event, rx) = Event::task("cron-task".to_string(), None, Some(source), 1);
    worker.exec(event).await.expect("exec");

    assert!(rx.await.expect("task result").success);
    assert!(
        ops.logs
            .lock()
            .unwrap()
            .iter()
            .any(|(_, message)| message.contains("scheduled at 1234567890 by '*/5 * * * *'")),
        "cron log missing: {:?}",
        ops.logs.lock().unwrap()
    );
}

#[tokio::test]
async fn a_oneshot_bridge_resumes_the_handler() {
    let mut worker = WasmWorker::new(script("hello", None), None, None)
        .await
        .expect("worker");

    let (event, rx) = Event::fetch(get("https://example.com/bridge"));
    worker.exec(event).await.expect("exec");

    let response = rx.await.expect("response");

    assert_eq!(response.status, 200);
    assert_eq!(body_of(&response), "bridged");
}
