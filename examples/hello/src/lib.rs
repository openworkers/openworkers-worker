//! The smallest worker that exercises the SDK: a fetch handler, a cron
//! handler, an outbound fetch and an environment variable.
//!
//! Build with: cargo build --target wasm32-wasip2 --release

use worker::*;

#[event(fetch)]
async fn fetch(mut req: Request, env: Env, ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;

    match url.path() {
        "/echo" => Response::from_bytes(req.bytes().await?),
        "/proxy" => {
            let upstream = Fetch::Url(Url::parse("https://upstream.example/data")?);
            let mut response = upstream.send().await?;

            Response::ok(response.text().await?)
        }
        "/wait-until" => {
            ctx.wait_until(async {
                console_log!("wait_until ran");
            });

            Response::ok("queued")
        }
        // The bridge a workers-rs application uses to hand a `!Send` future
        // to a framework that wants `Send`
        "/bridge" => {
            let (tx, rx) = futures_channel::oneshot::channel();

            worker::wasm_bindgen_futures::spawn_local(async move {
                let _ = tx.send("bridged".to_string());
            });

            Response::ok(rx.await.map_err(|e| Error::RustError(e.to_string()))?)
        }
        // Reads the body chunk by chunk; on the 0.3 world the whole body
        // never exists in guest memory
        "/stream-sum" => {
            use futures_util::StreamExt;

            let mut stream = req.stream()?;
            let mut total: u64 = 0;

            while let Some(chunk) = stream.next().await {
                total += chunk?.len() as u64;
            }

            Response::ok(total.to_string())
        }
        // Emits `mb` megabytes without ever holding more than a chunk on
        // the 0.3 world; the 0.2 world collects them
        "/stream-out" => {
            let mb: usize = url
                .query()
                .and_then(|q| q.strip_prefix("mb="))
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);

            let chunks =
                futures_util::stream::iter((0..mb * 16).map(|_| Ok(vec![b'x'; 64 * 1024])));

            Response::from_stream(chunks)
        }
        "/headers" => {
            let headers = Headers::new();
            headers.set("x-seen-host", url.host_str().unwrap_or_default())?;

            Ok(Response::ok("headers")?.with_headers(headers))
        }
        _ => {
            let greeting = env
                .var("GREETING")
                .map_or("Hello".to_string(), |v| v.to_string());

            console_log!("{} {}", req.method(), url.path());

            Response::ok(format!(
                "{greeting} from openworkers-worker!\nYou requested: {url}"
            ))
        }
    }
}

#[event(scheduled)]
async fn tick(event: ScheduledEvent, _env: Env, _ctx: ScheduleContext) {
    console_error_panic_hook::set_once();
    console_log!("scheduled at {} by '{}'", event.schedule(), event.cron());
}
