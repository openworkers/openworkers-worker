//! The health-shapes example, served by the real host with an in-memory
//! stand-in for the runner's binding handlers.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use openworkers_core::DatabaseOp;
use openworkers_core::DatabaseResult;
use openworkers_core::Event;
use openworkers_core::HttpMethod;
use openworkers_core::HttpRequest;
use openworkers_core::HttpResponse;
use openworkers_core::KvOp;
use openworkers_core::KvResult;
use openworkers_core::LogLevel;
use openworkers_core::OpFuture;
use openworkers_core::OperationsHandler;
use openworkers_core::RequestBody;
use openworkers_core::ResponseBody;
use openworkers_core::Script;
use openworkers_core::SqlParam;
use openworkers_core::StorageOp;
use openworkers_core::StorageResult;
use openworkers_core::WorkerCode;
use openworkers_runtime_wasm::WasmWorker;

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

/// Stands in for the runner's binding handlers: it records the SQL it was
/// given and keeps KV and storage in memory so values can be read back.
#[derive(Default)]
struct BindingOps {
    queries: Mutex<Vec<(String, String, Vec<SqlParam>)>>,
    values: Mutex<HashMap<String, serde_json::Value>>,
    objects: Mutex<HashMap<String, Vec<u8>>>,
    logs: Mutex<Vec<String>>,
}

impl OperationsHandler for BindingOps {
    fn handle_binding_database(
        &self,
        binding: &str,
        op: DatabaseOp,
    ) -> OpFuture<'_, DatabaseResult> {
        let DatabaseOp::Query { sql, params } = op;

        self.queries
            .lock()
            .unwrap()
            .push((binding.to_string(), sql.clone(), params));

        let json = match sql.starts_with("SELECT") || sql.contains("RETURNING") {
            true => r#"[{"slug":"api","name":"API","url":"https://api.test","expects":200}]"#,
            false => r#"{"rowsAffected":3}"#,
        };

        Box::pin(async move { DatabaseResult::Rows(json.to_string()) })
    }

    fn handle_binding_kv(&self, _binding: &str, op: KvOp) -> OpFuture<'_, KvResult> {
        let mut values = self.values.lock().unwrap();

        let result = match op {
            KvOp::Get { key } => KvResult::Value(values.get(&key).cloned()),
            KvOp::Put { key, value, .. } => {
                values.insert(key, value);
                KvResult::Ok
            }
            KvOp::Delete { key } => {
                values.remove(&key);
                KvResult::Ok
            }
            KvOp::List { .. } => KvResult::Keys(values.keys().cloned().collect()),
        };

        Box::pin(async move { result })
    }

    fn handle_binding_storage(&self, _binding: &str, op: StorageOp) -> OpFuture<'_, StorageResult> {
        let mut objects = self.objects.lock().unwrap();

        let result = match op {
            StorageOp::Get { key } => StorageResult::Body(objects.get(&key).cloned()),
            StorageOp::Put { key, body } => {
                objects.insert(key, body);
                StorageResult::Body(None)
            }
            StorageOp::Delete { key } => {
                objects.remove(&key);
                StorageResult::Body(None)
            }
            StorageOp::Head { key } => match objects.get(&key) {
                Some(body) => StorageResult::Head {
                    size: body.len() as u64,
                    etag: Some("mock-etag".to_string()),
                },
                None => StorageResult::Error("Object not found".to_string()),
            },
            StorageOp::List { .. } => StorageResult::List {
                keys: objects.keys().cloned().collect(),
                truncated: false,
            },
            StorageOp::Fetch { .. } => StorageResult::Error("fetch is not part of the WIT".into()),
        };

        Box::pin(async move { result })
    }

    fn handle_log(&self, _level: LogLevel, message: String) {
        self.logs.lock().unwrap().push(message);
    }
}

fn script() -> Script {
    Script {
        code: WorkerCode::WebAssembly(load_component("health-shapes")),
        env: None,
        bindings: vec![],
    }
}

async fn serve(path: &str) -> (HttpResponse, Arc<BindingOps>) {
    let ops = Arc::new(BindingOps::default());
    let mut worker = WasmWorker::new_with_ops(script(), None, ops.clone())
        .await
        .expect("worker");

    let request = HttpRequest {
        url: format!("https://example.com{path}"),
        method: HttpMethod::Get,
        headers: HashMap::new(),
        body: RequestBody::None,
    };

    let (event, rx) = Event::fetch(request);
    worker.exec(event).await.expect("exec");

    (rx.await.expect("response"), ops)
}

fn body_text(response: &HttpResponse) -> String {
    let ResponseBody::Bytes(body) = &response.body else {
        panic!("expected a buffered body");
    };

    String::from_utf8_lossy(body).into_owned()
}

#[tokio::test]
async fn database_query_deserializes_rows() {
    let (response, ops) = serve("/targets").await;

    assert_eq!(response.status, 200);
    assert!(
        body_text(&response).contains("\"slug\":\"api\""),
        "{}",
        body_text(&response)
    );

    let queries = ops.queries.lock().unwrap();

    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].0, "DB");
}

#[tokio::test]
async fn database_query_sends_typed_parameters() {
    let (response, ops) = serve("/record").await;

    assert_eq!(response.status, 200);
    assert_eq!(body_text(&response), "recorded 1");

    let queries = ops.queries.lock().unwrap();
    let (_, sql, params) = &queries[0];

    assert!(sql.starts_with("INSERT INTO checks"));
    assert_eq!(params.len(), 5);
    assert!(
        format!("{params:?}").contains("Int(200)"),
        "a whole number should bind as an integer: {params:?}"
    );
}

#[tokio::test]
async fn database_query_reads_one_column() {
    let (response, _) = serve("/first").await;

    assert_eq!(response.status, 200);
    assert_eq!(body_text(&response), "API");
}

#[tokio::test]
async fn kv_round_trips_a_string() {
    let (response, _) = serve("/kv").await;

    assert_eq!(response.status, 200);
    assert_eq!(body_text(&response), "hello");
}

#[tokio::test]
async fn storage_round_trips_bytes() {
    let (response, _) = serve("/storage").await;

    assert_eq!(response.status, 200);
    assert_eq!(body_text(&response), "png bytes");
}

#[tokio::test]
async fn kv_lists_what_delete_left() {
    let (response, _) = serve("/kv-list").await;

    assert_eq!(response.status, 200);
    assert_eq!(body_text(&response), "greeting");
}

#[tokio::test]
async fn storage_head_and_list_report_the_object() {
    let (response, _) = serve("/storage-head").await;

    assert_eq!(response.status, 200);
    assert_eq!(body_text(&response), "9 mock-etag logo.png false");
}

#[tokio::test]
async fn database_query_binds_an_array_as_a_list() {
    let (response, ops) = serve("/any").await;

    assert_eq!(response.status, 200);
    assert_eq!(body_text(&response), "1");

    let queries = ops.queries.lock().unwrap();
    let (_, _, params) = &queries[0];

    assert_eq!(params.len(), 1);
    assert!(
        matches!(&params[0], SqlParam::Array(values) if values.len() == 2),
        "an array should bind as one list parameter: {params:?}"
    );
}

#[tokio::test]
async fn send_email_reports_that_it_is_unsupported() {
    let (response, _) = serve("/email").await;

    assert_eq!(response.status, 200);
    assert_eq!(
        body_text(&response),
        "email refused: send_email is not supported by this platform yet"
    );
}

#[tokio::test]
async fn scheduled_handler_runs_a_statement() {
    let ops = Arc::new(BindingOps::default());
    let mut worker = WasmWorker::new_with_ops(script(), None, ops.clone())
        .await
        .expect("worker");

    let (event, rx) = Event::from_schedule("prune".to_string(), 1_234_567_890);
    worker.exec(event).await.expect("exec");

    assert!(rx.await.expect("task result").success);

    let queries = ops.queries.lock().unwrap();

    assert_eq!(queries.len(), 1);
    assert!(queries[0].1.starts_with("DELETE FROM checks"));
    assert!(
        ops.logs
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains("pruned 1")),
        "cron log missing: {:?}",
        ops.logs.lock().unwrap()
    );
}
