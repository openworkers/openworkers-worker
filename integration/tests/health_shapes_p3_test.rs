//! The health-shapes example built for the WASI 0.3 world: the platform
//! bindings keep their sync ABI inside the async world, so the same database,
//! kv and storage round trips must come out unchanged.

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

/// The in-memory stand-ins the 0.2 shapes test uses, trimmed to what the
/// three round trips exercise.
#[derive(Default)]
struct BindingOps {
    queries: Mutex<Vec<(String, String, Vec<SqlParam>)>>,
    values: Mutex<HashMap<String, serde_json::Value>>,
    objects: Mutex<HashMap<String, Vec<u8>>>,
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
}

async fn serve(path: &str) -> (HttpResponse, Arc<BindingOps>) {
    let ops = Arc::new(BindingOps::default());

    let script = Script {
        code: WorkerCode::WebAssembly(load_p3_component("health-shapes")),
        env: None,
        bindings: vec![],
    };

    let mut worker = WasmWorker::new_with_ops(script, None, ops.clone())
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
async fn p3_database_query_deserializes_rows() {
    let (response, ops) = serve("/targets").await;

    assert_eq!(response.status, 200);
    assert!(
        body_text(&response).contains("\"slug\":\"api\""),
        "{}",
        body_text(&response)
    );

    let queries = ops.queries.lock().unwrap();
    assert_eq!(queries.len(), 1);
}

#[tokio::test]
async fn p3_kv_round_trips_a_string() {
    let (response, _) = serve("/kv").await;

    assert_eq!(response.status, 200);
    assert_eq!(body_text(&response), "hello");
}

#[tokio::test]
async fn p3_storage_round_trips_bytes() {
    let (response, _) = serve("/storage").await;

    assert_eq!(response.status, 200);
    assert_eq!(body_text(&response), "png bytes");
}
