//! The task example, served by the real host: `#[event(task)]` gets the
//! payload and the source, and its value or its error is the task result.

use openworkers_core::Event;
use openworkers_core::Script;
use openworkers_core::TaskResult;
use openworkers_core::TaskSource;
use openworkers_core::WorkerCode;
use openworkers_runtime_wasm::WasmWorker;
use serde_json::json;

/// Built by `cd examples/task && cargo build --target wasm32-wasip2 --release`
fn task_example() -> Script {
    let path = format!(
        "{}/../examples/task/target/wasm32-wasip2/release/task.wasm",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "could not read {path}: {e}\n\
             build it with: cd examples/task && cargo build --target wasm32-wasip2 --release"
        )
    });

    Script {
        code: WorkerCode::WebAssembly(bytes),
        env: None,
        bindings: vec![],
    }
}

async fn run(event: Event, rx: tokio::sync::oneshot::Receiver<TaskResult>) -> TaskResult {
    let mut worker = WasmWorker::new(task_example(), None, None)
        .await
        .expect("worker");
    worker.exec(event).await.expect("exec");

    rx.await.expect("task result")
}

#[tokio::test]
async fn an_invoked_task_gets_its_payload() {
    let (event, rx) = Event::invoke(
        "invoke-1".to_string(),
        Some(json!({ "quantity": 3 })),
        Some("cli".to_string()),
    );
    let result = run(event, rx).await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        result.data,
        Some(json!({ "id": "invoke-1", "attempt": 1, "payload": { "quantity": 3 }, "cron": null }))
    );
}

#[tokio::test]
async fn a_cron_task_gets_its_cron() {
    let source = TaskSource::Schedule {
        time: 1_700_000_000_000,
        cron: Some("0 * * * *".to_string()),
    };
    let (event, rx) = Event::task("cron-1".to_string(), None, Some(source), 2);
    let result = run(event, rx).await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        result.data,
        Some(json!({ "id": "cron-1", "attempt": 2, "payload": null, "cron": "0 * * * *" }))
    );
}

#[tokio::test]
async fn an_error_from_the_handler_fails_the_task() {
    let (event, rx) = Event::invoke("invoke-2".to_string(), Some(json!("fail")), None);
    let result = run(event, rx).await;

    assert!(!result.success);
    assert_eq!(result.error.as_deref(), Some("asked to fail"));
}
