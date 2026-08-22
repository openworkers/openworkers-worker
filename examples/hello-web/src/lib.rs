//! A worker that serves one HTML page, as a face for the platform's
//! Rust support.
//!
//! Build with: cargo build --target wasm32-wasip2 --release

use worker::*;

const PAGE: &str = include_str!("page.html");

/// The path lands in HTML, and the client chooses it
fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[event(fetch)]
async fn fetch(req: Request, _env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;

    let page = PAGE
        .replace("{method}", &req.method().to_string())
        .replace("{path}", &escape_html(url.path()));

    let headers = Headers::new();
    headers.set("content-type", "text/html; charset=utf-8")?;

    Ok(Response::ok(page)?.with_headers(headers))
}
