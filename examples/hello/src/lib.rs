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
        "/headers" => {
            let headers = Headers::new();
            headers.set("x-seen-host", url.host_str().unwrap_or_default())?;

            Ok(Response::ok("headers")?.with_headers(headers))
        }
        _ => {
            let greeting = env.var("GREETING").map_or("Hello".to_string(), |v| v.to_string());

            console_log!("{} {}", req.method(), url.path());

            Response::ok(format!("{greeting} from openworkers-worker!\nYou requested: {url}"))
        }
    }
}

#[event(scheduled)]
async fn tick(event: ScheduledEvent, _env: Env, _ctx: ScheduleContext) {
    console_error_panic_hook::set_once();
    console_log!("scheduled at {}", event.schedule());
}
