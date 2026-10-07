//! A task handler: it gets every task, cron included, and answers what it
//! received. A payload of `"fail"` fails the task.
//!
//! Build with: cargo build --target wasm32-wasip2 --release

use serde_json::json;
use serde_json::Value;
use worker::*;

#[event(task)]
async fn task(event: TaskEvent, _env: Env, _ctx: TaskContext) -> Result<Value> {
    let payload: Option<Value> = event.payload()?;

    if payload == Some(json!("fail")) {
        return Err(Error::RustError("asked to fail".to_string()));
    }

    let cron = match event.source() {
        Some(TaskSource::Schedule { cron, .. }) => cron.clone(),
        _ => None,
    };

    Ok(json!({
        "id": event.id(),
        "attempt": event.attempt(),
        "payload": payload,
        "cron": cron,
    }))
}
