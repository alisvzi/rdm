//! Tiny local HTTP server that lets the browser extension hand links to RDM.
//!
//! Safety rules (a web page must never be able to start downloads):
//!  * listens on 127.0.0.1 only
//!  * `Host` must be 127.0.0.1/localhost (blocks DNS-rebinding)
//!  * if the request has an `Origin`, it must be a browser-extension origin
//!  * every request needs the secret `X-RDM-Token` shown in the app settings

use rdm_core::manager::Manager as Downloads;
use serde::Deserialize;
use serde_json::json;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager as _};
use tiny_http::{Header, Method, Request, Response, Server};

pub const PORT: u16 = 46873;

/// Tauri-managed state: did the server manage to start?
pub struct ApiStatus(pub AtomicBool);

type Resp = Response<std::io::Cursor<Vec<u8>>>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddBody {
    url: String,
    referrer: Option<String>,
    cookies: Option<String>,
    user_agent: Option<String>,
    #[serde(default = "yes")]
    start_now: bool,
}

fn yes() -> bool {
    true
}

pub fn start(app: AppHandle, dl: Downloads) {
    std::thread::spawn(move || {
        let server = match Server::http(("127.0.0.1", PORT)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("browser integration disabled: cannot listen on port {PORT}: {e}");
                return;
            }
        };
        app.state::<ApiStatus>().0.store(true, Ordering::Relaxed);
        for req in server.incoming_requests() {
            let dl = dl.clone();
            let app = app.clone();
            std::thread::spawn(move || handle(req, dl, app));
        }
    });
}

fn header_value(req: &Request, name: &str) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| {
            let field = h.field.as_str().as_ref();
            field.eq_ignore_ascii_case(name)
        })
        .map(|h| h.value.to_string())
}

fn reply(status: u16, body: Option<serde_json::Value>, origin: Option<&str>) -> Resp {
    let text = body.map(|b| b.to_string()).unwrap_or_default();
    let mut r = Response::from_string(text).with_status_code(status);
    let headers: [(&str, String); 5] = [
        ("Content-Type", "application/json".into()),
        ("Access-Control-Allow-Origin", origin.unwrap_or("*").into()),
        (
            "Access-Control-Allow-Headers",
            "Content-Type, X-RDM-Token".into(),
        ),
        ("Access-Control-Allow-Methods", "GET, POST, OPTIONS".into()),
        ("Vary", "Origin".into()),
    ];
    for (k, v) in headers {
        if let Ok(h) = Header::from_bytes(k.as_bytes(), v.as_bytes()) {
            r = r.with_header(h);
        }
    }
    r
}

fn origin_ok(o: &str) -> bool {
    o.starts_with("chrome-extension://")
        || o.starts_with("moz-extension://")
        || o.starts_with("safari-web-extension://")
}

fn host_ok(h: Option<&str>) -> bool {
    match h {
        Some(h) => {
            let host = h.split(':').next().unwrap_or("");
            host == "127.0.0.1" || host == "localhost"
        }
        None => false,
    }
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn handle(mut req: Request, dl: Downloads, app: AppHandle) {
    let origin = header_value(&req, "Origin");
    let o = origin.as_deref();

    // CORS preflight carries no secrets.
    if *req.method() == Method::Options {
        let _ = req.respond(reply(204, None, o));
        return;
    }

    if !host_ok(header_value(&req, "Host").as_deref()) {
        let _ = req.respond(reply(
            403,
            Some(json!({"ok": false, "error": "bad host"})),
            o,
        ));
        return;
    }
    if let Some(origin) = o {
        if !origin_ok(origin) {
            let _ = req.respond(reply(
                403,
                Some(json!({"ok": false, "error": "forbidden origin"})),
                o,
            ));
            return;
        }
    }

    let token = header_value(&req, "X-RDM-Token").unwrap_or_default();
    let expected = dl.settings().api_token;
    if expected.is_empty() || !constant_eq(token.as_bytes(), expected.as_bytes()) {
        let _ = req.respond(reply(
            401,
            Some(json!({"ok": false, "error": "wrong token"})),
            o,
        ));
        return;
    }

    let path = req.url().split('?').next().unwrap_or("").to_string();
    let method = req.method().clone();

    let response = match (method, path.as_str()) {
        (Method::Get, "/ping") => reply(
            200,
            Some(json!({"ok": true, "app": "rdm", "version": env!("CARGO_PKG_VERSION")})),
            o,
        ),

        (Method::Post, "/add") => {
            let mut body = String::new();
            let read_ok = req
                .as_reader()
                .take(1 << 20)
                .read_to_string(&mut body)
                .is_ok();
            match (read_ok, serde_json::from_str::<AddBody>(&body)) {
                (true, Ok(b)) => {
                    let mut headers: Vec<(String, String)> = Vec::new();
                    if let Some(c) = b.cookies.filter(|c| !c.is_empty()) {
                        headers.push(("Cookie".to_string(), c));
                    }
                    if let Some(r) = b.referrer.filter(|r| r.starts_with("http")) {
                        headers.push(("Referer".to_string(), r));
                    }
                    if let Some(u) = b.user_agent.filter(|u| !u.is_empty()) {
                        headers.push(("User-Agent".to_string(), u));
                    }
                    let res = tauri::async_runtime::block_on(dl.add_with(
                        &b.url,
                        None,
                        b.start_now,
                        headers,
                    ));
                    match res {
                        Ok(id) => {
                            let _ = app.emit("downloads", dl.snapshot());
                            if let Some(p) = dl.path_of(id) {
                                let name = std::path::Path::new(&p)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                let _ = app.emit("added", name);
                            }
                            reply(200, Some(json!({"ok": true, "id": id})), o)
                        }
                        Err(e) => reply(
                            400,
                            Some(json!({"ok": false, "error": format!("{:#}", e)})),
                            o,
                        ),
                    }
                }
                _ => reply(400, Some(json!({"ok": false, "error": "bad request"})), o),
            }
        }

        _ => reply(404, Some(json!({"ok": false, "error": "not found"})), o),
    };
    let _ = req.respond(response);
}
