//! The shapes a real workers-rs status page uses, in one worker: two D1
//! bindings read through prepare/bind/all/first/run, KV, R2, an email
//! announcement and a cron sweep.
//!
//! Every call site here is copied from a workers-rs application unchanged.
//!
//! Build with: cargo build --target wasm32-wasip2 --release

use std::cell::RefCell;
use std::rc::Rc;

use serde::Deserialize;
use serde::Serialize;
use worker::wasm_bindgen::JsValue;
use worker::*;

/// A service to watch, as the configuration database describes it.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Target {
    pub slug: String,
    pub name: String,
    pub url: String,
    pub expects: i64,
}

thread_local! {
    static DB: RefCell<Option<Rc<D1Database>>> = const { RefCell::new(None) };
}

fn install(env: &Env) -> Result<()> {
    let db = env.d1("DB")?;

    DB.with(|cell| *cell.borrow_mut() = Some(Rc::new(db)));

    Ok(())
}

fn db() -> Rc<D1Database> {
    DB.with(|cell| cell.borrow().clone().expect("DB not installed"))
}

fn s(text: &str) -> JsValue {
    JsValue::from_str(text)
}

fn n(value: i64) -> JsValue {
    JsValue::from_f64(value as f64)
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    install(&env)?;

    let url = req.url()?;

    match url.path() {
        "/targets" => {
            let targets: Vec<Target> = db()
                .prepare("SELECT slug, name, url, expects FROM targets ORDER BY name")
                .all()
                .await?
                .results()?;

            Response::from_json(&targets)
        }
        "/record" => {
            let args = vec![s("api"), s("2026-08-19T10:00:00Z"), n(200), n(12), n(1)];
            let result = db()
                .prepare(
                    "INSERT INTO checks (slug, at, status, latency_ms, ok) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .bind(&args)?
                .run()
                .await?;

            let changes = result.meta()?.and_then(|meta| meta.changes).unwrap_or(0);

            Response::ok(format!("recorded {changes}"))
        }
        "/first" => {
            let name: Option<String> = db()
                .prepare("SELECT name FROM targets WHERE slug = ?1")
                .bind(&[s("api")])?
                .first(Some("name"))
                .await?;

            Response::ok(name.unwrap_or_else(|| "none".to_string()))
        }
        "/kv" => {
            let store = env.kv("CACHE")?;

            store.put("greeting", "hello")?.execute().await?;

            let value = store.get("greeting").text().await?;

            Response::ok(value.unwrap_or_default())
        }
        "/r2" => {
            let bucket = env.bucket("PHOTOS")?;

            bucket.put("logo.png", b"png bytes".to_vec()).execute().await?;

            let object = bucket.get("logo.png").execute().await?;
            let Some(object) = object else {
                return Response::error("missing", 404);
            };
            let Some(body) = object.body() else {
                return Response::error("no body", 500);
            };

            Response::from_bytes(body.bytes().await?)
        }
        "/email" => {
            let binding = env.send_email("EMAIL")?;
            let sender = EmailAddress::new("Supervision", "alerts@example.test");
            let message = SendEmailBuilder::builder_with_email_address_and_str(
                &sender,
                "ops@example.test",
                "api is down",
            )
            .text("api (https://api.example.test) did not answer.\n")
            .build();

            match binding.send_with_builder(&message).await {
                Ok(_) => Response::ok("sent"),
                Err(e) => Response::ok(format!("email refused: {e}")),
            }
        }
        _ => Response::ok("health-shapes"),
    }
}

/// One sweep per cron tick, as a status page would run it.
#[event(scheduled)]
async fn tick(event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    console_error_panic_hook::set_once();

    if let Err(e) = install(&env) {
        console_log!("cron {}: {e}", event.schedule());
        return;
    }

    let pruned = db()
        .prepare("DELETE FROM checks WHERE at < datetime('now', ?1)")
        .bind(&[s("-30 days")]);

    match pruned {
        Err(e) => console_log!("cron: {e}"),
        Ok(statement) => match statement.run().await {
            Ok(result) => console_log!(
                "cron: pruned {}",
                result.meta().ok().flatten().and_then(|meta| meta.changes).unwrap_or(0)
            ),
            Err(e) => console_log!("cron: prune failed: {e}"),
        },
    }
}
