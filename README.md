# openworkers-worker

A guest SDK for OpenWorkers. Requests, responses and handlers follow
Cloudflare's [`worker`](https://crates.io/crates/worker) crate 0.8; the
bindings follow OpenWorkers' JavaScript runtime.

```toml
worker = { package = "openworkers-worker", version = "0.2" }
```

The same `use worker::*`, the same `#[event(fetch)]` and `#[event(scheduled)]`,
the same `Request`/`Response`/`Headers`/`Env` as workers-rs. The bindings are
the ones a JavaScript worker sees on `env`: `env.database("DB")` is `env.DB`,
with its `query`, and the same holds for `kv` and `storage`. Underneath there
is no JavaScript, no wasm-bindgen and no V8: the crate targets `wasm32-wasip2`
and speaks `wasi:http/proxy@0.2.0` plus `openworkers:bindings@0.1.0` to a
wasmtime host.

```
cargo build --target wasm32-wasip2 --release
```

The result is a component. Nothing else is needed: no `worker-build`, no
`wrangler`, no JS shim.

## What a worker looks like

```rust
use worker::*;

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let rows: Vec<Row> = env
        .database("DB")?
        .query("SELECT * FROM targets WHERE active = $1", &[json!(true)])
        .await?;

    Response::from_json(&rows)
}

#[event(scheduled)]
async fn tick(event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    console_log!("cron at {}", event.schedule());
}
```

A worker with only a fetch handler exports `wasi:http/incoming-handler` alone;
adding `#[event(scheduled)]` adds `openworkers:worker/scheduled` and
`openworkers:worker/task`, which is how the host tells a cron-capable worker
from one that only serves HTTP. A host that has the task export calls it and
sends the cron expression; an older host calls `scheduled` with the time alone.

## Tasks

`#[event(task)]` is OpenWorkers' own: it gets every task the platform runs,
cron, chained, sent from a worker or invoked, with its payload. What it returns
is the task result, as JSON; an error fails the task.

```rust
#[event(task)]
async fn task(event: TaskEvent, _env: Env, _ctx: TaskContext) -> Result<Option<u32>> {
    let order: Option<Order> = event.payload()?;

    Ok(order.map(|order| order.quantity * 2))
}
```

A worker has `#[event(task)]` or `#[event(scheduled)]`, not both: each one
emits the task export. A cron tick reaches a task handler with
`TaskSource::Schedule { time, cron }`.

## Migrating a workers-rs application

Rename the dependency, then move the bindings to the OpenWorkers ones:

- D1 becomes `env.database("DB")?.query(sql, params)`, and its SQL becomes
  PostgreSQL: `$1` placeholders, `now() - $1::interval` for
  `datetime('now', ?1)`. A statement that has to report what it changed
  says `RETURNING`.
- `env.kv("KV")` keeps its name; `get`, `put`, `delete` and `list` take their
  arguments directly instead of through builders.
- R2 becomes `env.storage("STORAGE")`, with `get`, `put`, `head`, `list` and
  `delete`.

What else has to go is anything that talks to JavaScript directly, because
wasm-bindgen's imports have no host on `wasm32-wasip2` and panic with
*"cannot call wasm-bindgen imported functions on non-wasm targets"* the first
time they run:

1. Drop the `console_error_panic_hook` dependency and add
   `use worker::console_error_panic_hook;`. The call site stays as it is.
2. Drop the `wasm-bindgen-futures` dependency and reach `spawn_local` through
   `worker::wasm_bindgen_futures::spawn_local`.
3. Drop `wasmbind` from `chrono`'s features, and `wasm-bindgen` from `time`'s.
   Both read the clock through `std` once the feature is off.
4. Replace any other `js_sys` / `wasm_bindgen` use. `worker::js_sys::Date` and
   `worker::wasm_bindgen::JsValue` cover what a clock call site needs;
   `Reflect`, `Promise`, `Uint8Array` and the rest have no counterpart.

## What is implemented

| Area | Status |
|---|---|
| `#[event(fetch)]`, `#[event(scheduled)]` | yes, `respond_with_errors` included |
| `#[event(task)]`: payload, source, attempt, JSON result | yes, OpenWorkers only |
| `Request`, `Response`, `ResponseBuilder`, `Headers`, `Method`, `Url` | yes |
| `http::Request`/`http::Response` conversions, `Body` | yes |
| `Env`, `Var`, `Secret` (from `wasi:cli/environment`) | yes |
| `Fetch` (outbound HTTP) | yes, through `wasi:http/outgoing-handler` |
| `console_log!` and friends, panic hook | yes, to stdout and stderr |
| `env.database`: `query`, PostgreSQL | yes |
| `env.kv`: `get`/`put`/`delete`/`list`, JSON values | yes |
| `env.storage`: `get`/`put`/`head`/`list`/`delete` | yes; no `fetch`, the host has no such call |
| D1, R2, Cloudflare's KV builders | absent; see the bindings above |
| `send_email` | types only; every send reports that the platform has no email operation |
| Queues, Durable Objects, WebSockets, Cache, Images | absent |

## Known differences from workers-rs

- **Bodies are buffered.** The host buffers request and response bodies at the
  boundary, so `Response::from_stream` collects and `ResponseBody` has no
  stream variant.
- **`ScheduledEvent::cron()` is empty on runtime-wasm before 0.15.3.** Those
  hosts call the scheduled export, which carries only the trigger time.
- **`req.cf()` is always `None`.** There is no Cloudflare edge metadata.
- **`storage.head` errors on a missing key** rather than returning `Ok(None)`:
  the host's `head` cannot separate absence from failure. Use `get` when the
  two have to be told apart.
- **`worker::wasm_bindgen::JsValue` is a plain tagged value**, not a JS handle.
  `from_str`, `from_f64`, `from_bool`, `NULL` and the `From` impls work; the
  rest of wasm-bindgen does not exist.
- **`worker::js_sys`** carries `Date::now()` only.
- **`#[event(start)]`, `#[event(queue)]`, `#[event(email)]` and
  `#[durable_object]`** are compile errors rather than silent no-ops.

## Async

WASI 0.2 exports are synchronous, so an async handler is driven to completion
inside `handle` by a poll loop over a no-op waker (`src/rt.rs`). Every host
call at 0.2 has a blocking form, so a guest future never parks waiting on the
host and no reactor is needed; `futures::executor::block_on` would park the
thread instead, and a wasm32-wasip2 guest has no second thread to unpark it.
`ctx.wait_until` and `wasm_bindgen_futures::spawn_local` queue tasks that the
same loop drains before the export returns.

## Layout

- `.` - the SDK
- `wit/` - verbatim copy of the runtime's contract; the SDK generates against
  its `fetch-worker`, `scheduled-only` and `task-only` worlds
- `macros/` - `#[event]` and friends
- `integration/` - runs the examples through `openworkers-runtime-wasm`
- `examples/hello` - the smallest worker
- `examples/health-shapes` - the shapes a real status page uses
- `examples/task` - a task handler that answers what it received

```
cd examples/hello && cargo build --target wasm32-wasip2 --release
cd ../.. && cargo test -p openworkers-worker-integration
```

## Licence

MIT.
