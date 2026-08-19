# openworkers-worker

A guest SDK for OpenWorkers, source-compatible with Cloudflare's
[`worker`](https://crates.io/crates/worker) crate 0.8.

An existing workers-rs application moves over by renaming one dependency:

```toml
# before
worker = { version = "0.8", features = ["d1"] }

# after
worker = { package = "openworkers-worker", version = "0.1", features = ["d1"] }
```

The same `use worker::*`, the same `#[event(fetch)]` and `#[event(scheduled)]`,
the same `Request`/`Response`/`Headers`/`Env`/`D1Database`. Underneath there is
no JavaScript, no wasm-bindgen and no V8: the crate targets `wasm32-wasip2` and
speaks `wasi:http/proxy@0.2.12` plus `openworkers:bindings@0.1.0` to a wasmtime
host.

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
    let db = env.d1("DB")?;
    let rows: Vec<Row> = db.prepare("SELECT * FROM targets").all().await?.results()?;

    Response::from_json(&rows)
}

#[event(scheduled)]
async fn tick(event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    console_log!("cron at {}", event.schedule());
}
```

A worker with only a fetch handler exports `wasi:http/incoming-handler` alone;
adding `#[event(scheduled)]` adds `openworkers:worker/scheduled`, which is how
the host tells a cron-capable worker from one that only serves HTTP.

## What is implemented

| Area | Status |
|---|---|
| `#[event(fetch)]`, `#[event(scheduled)]` | yes, `respond_with_errors` included |
| `Request`, `Response`, `ResponseBuilder`, `Headers`, `Method`, `Url` | yes |
| `http::Request`/`http::Response` conversions, `Body` | yes |
| `Env`, `Var`, `Secret` (from `wasi:cli/environment`) | yes |
| `Fetch` (outbound HTTP) | yes, through `wasi:http/outgoing-handler` |
| `console_log!` and friends, panic hook | yes, to stdout and stderr |
| D1 (`d1` feature): `prepare`/`bind`/`first`/`all`/`run`/`exec`/`batch`, `query!` | yes |
| KV: `get`/`put`/`delete`/`list` | yes |
| R2: `get`/`put`/`delete`/`head`/`list` | yes |
| `send_email` | types only; every send reports that the platform has no email operation |
| Queues, Durable Objects, WebSockets, Cache, Images | absent |

## Known differences from workers-rs

- **Bodies are buffered.** The host buffers request and response bodies at the
  boundary, so `Response::from_stream` collects and `ResponseBody` has no
  stream variant.
- **`ScheduledEvent::cron()` is empty.** The host's scheduled export carries
  only the trigger time.
- **`req.cf()` is always `None`.** There is no Cloudflare edge metadata.
- **`Bucket::head` errors on a missing key** rather than returning `Ok(None)`:
  the host's `head` cannot separate absence from failure. Use `get` when the
  two have to be told apart.
- **KV stores JSON documents.** A plain string is stored quoted and unquoted on
  the way back; `put_bytes` stores a JSON array of bytes.
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
- `macros/` - `#[event]` and friends
- `integration/` - runs the examples through `openworkers-runtime-wasm`
- `examples/hello` - the smallest worker
- `examples/health-shapes` - the shapes a real status page uses

```
cd examples/hello && cargo build --target wasm32-wasip2 --release
cd ../.. && cargo test -p openworkers-worker-integration
```

## Licence

MIT.
