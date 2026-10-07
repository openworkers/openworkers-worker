//! The shapes a status page uses, in one worker: the database read and
//! written through `query`, KV, storage, an email announcement and a cron
//! sweep. The bindings have the names and the methods of `env.DB`, `env.KV`
//! and `env.STORAGE` in a JavaScript worker; the SQL is PostgreSQL.
//!
//! Build with: cargo build --target wasm32-wasip2 --release

use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use worker::*;

/// A service to watch, as the configuration database describes it.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Target {
    pub slug: String,
    pub name: String,
    pub url: String,
    pub expects: i64,
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;

    match url.path() {
        "/targets" => {
            let targets: Vec<Target> = env
                .database("DB")?
                .query(
                    "SELECT slug, name, url, expects FROM targets ORDER BY name",
                    &[],
                )
                .await?;

            Response::from_json(&targets)
        }
        "/record" => {
            let params = [
                json!("api"),
                json!("2026-08-19T10:00:00Z"),
                json!(200),
                json!(12),
                json!(true),
            ];
            let rows: Vec<Value> = env
                .database("DB")?
                .query(
                    "INSERT INTO checks (slug, at, status, latency_ms, ok) \
                     VALUES ($1, $2, $3, $4, $5) RETURNING slug",
                    &params,
                )
                .await?;

            Response::ok(format!("recorded {}", rows.len()))
        }
        "/first" => {
            let rows: Vec<Value> = env
                .database("DB")?
                .query("SELECT name FROM targets WHERE slug = $1", &[json!("api")])
                .await?;
            let name = rows
                .first()
                .and_then(|row| row["name"].as_str())
                .unwrap_or("none");

            Response::ok(name)
        }
        "/kv" => {
            let kv = env.kv("CACHE")?;

            kv.put("greeting", "hello", None).await?;

            let value: Option<String> = kv.get("greeting").await?;

            Response::ok(value.unwrap_or_default())
        }
        "/storage" => {
            let storage = env.storage("PHOTOS")?;

            storage.put("logo.png", b"png bytes".to_vec()).await?;

            match storage.get("logo.png").await? {
                Some(body) => Response::from_bytes(body),
                None => Response::error("missing", 404),
            }
        }
        "/kv-list" => {
            let kv = env.kv("CACHE")?;

            kv.put("greeting", "hello", Some(60)).await?;
            kv.put("farewell", "bye", None).await?;
            kv.delete("farewell").await?;

            Response::ok(kv.list(Some("gr"), Some(10)).await?.join(","))
        }
        "/storage-head" => {
            let storage = env.storage("PHOTOS")?;

            storage.put("logo.png", b"png bytes".to_vec()).await?;

            let head = storage.head("logo.png").await?;
            let listing = storage.list(None, Some(10)).await?;

            Response::ok(format!(
                "{} {} {} {}",
                head.size,
                head.etag.unwrap_or_default(),
                listing.keys.join(","),
                listing.truncated
            ))
        }
        "/any" => {
            let targets: Vec<Target> = env
                .database("DB")?
                .query(
                    "SELECT slug, name, url, expects FROM targets WHERE slug = ANY($1)",
                    &[json!(["api", "web"])],
                )
                .await?;

            Response::ok(format!("{}", targets.len()))
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
async fn tick(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    console_error_panic_hook::set_once();

    let database = match env.database("DB") {
        Ok(database) => database,
        Err(e) => return console_log!("cron: {e}"),
    };

    let pruned: Result<Vec<Value>> = database
        .query(
            "DELETE FROM checks WHERE at < now() - $1::interval RETURNING slug",
            &[json!("30 days")],
        )
        .await;

    match pruned {
        Ok(rows) => console_log!("cron: pruned {}", rows.len()),
        Err(e) => console_log!("cron: prune failed: {e}"),
    }
}
