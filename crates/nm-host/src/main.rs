//! `rdm-nm.exe` — the Native Messaging host of RDM.
//!
//! The browser starts this program and talks to it over stdin/stdout. It forwards each
//! request to the RDM desktop app on 127.0.0.1 using the API token from the app's own
//! settings file, and starts the app first if it is not running. It never listens on a port.
//!
//! Requests : {"type":"ping"} | {"type":"add","payload":{...}} | {"type":"add-batch","payload":{...}}
//! Answers  : {"ok":bool,"status":<http status>,"data":<json from RDM>}  or  {"ok":false,"error":"..."}

mod frame;

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const PORT: u16 = 46873;

fn data_dir() -> Option<PathBuf> {
    if let Some(a) = std::env::var_os("APPDATA") {
        return Some(PathBuf::from(a).join("com.rdm.downloadmanager"));
    }
    std::env::var_os("HOME").map(|h| {
        PathBuf::from(h)
            .join(".local")
            .join("share")
            .join("com.rdm.downloadmanager")
    })
}

fn read_token() -> Option<String> {
    let text = std::fs::read_to_string(data_dir()?.join("settings.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get("apiToken")?
        .as_str()
        .map(str::to_string)
        .filter(|t| t.len() >= 16)
}

/// One HTTP request to the local app. Returns (status, body).
fn http(
    method: &str,
    path: &str,
    token: &str,
    body: Option<&[u8]>,
) -> std::io::Result<(u16, Vec<u8>)> {
    let addr: SocketAddr = ([127, 0, 0, 1], PORT).into();
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    s.set_read_timeout(Some(Duration::from_secs(120)))?;
    s.set_write_timeout(Some(Duration::from_secs(10)))?;
    let body = body.unwrap_or(&[]);
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nX-RDM-Token: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes())?;
    s.write_all(body)?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw)?;
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "bad answer from RDM")
        })?;
    let status: u16 = String::from_utf8_lossy(&raw[..split])
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    Ok((status, raw[split + 4..].to_vec()))
}

/// Starts rdm-gui.exe (next to this program) and waits until it answers.
fn launch_app(token: &str) -> bool {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    else {
        return false;
    };
    let exe = dir.join(if cfg!(windows) {
        "rdm-gui.exe"
    } else {
        "rdm-gui"
    });
    if !exe.exists() || std::process::Command::new(&exe).spawn().is_err() {
        return false;
    }
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(15) {
        std::thread::sleep(Duration::from_millis(400));
        if matches!(http("GET", "/ping", token, None), Ok((200, _))) {
            return true;
        }
    }
    false
}

fn handle(msg: &[u8]) -> Value {
    let Ok(v) = serde_json::from_slice::<Value>(msg) else {
        return json!({"ok": false, "error": "invalid JSON"});
    };
    let (method, path) = match v.get("type").and_then(Value::as_str) {
        Some("ping") => ("GET", "/ping"),
        Some("add") => ("POST", "/add"),
        Some("add-batch") => ("POST", "/add-batch"),
        _ => return json!({"ok": false, "error": "unknown request type"}),
    };
    let Some(token) = read_token() else {
        return json!({"ok": false, "error": "RDM has never been started (no settings file yet)"});
    };
    let body = v.get("payload").map(|p| p.to_string());
    let send = || http(method, path, &token, body.as_deref().map(str::as_bytes));

    let first = send();
    let res = match first {
        Ok(r) => Ok(r),
        Err(_) if method == "POST" && launch_app(&token) => send(),
        Err(e) => Err(e),
    };
    match res {
        Ok((status, data)) => json!({
            "ok": (200..300).contains(&status),
            "status": status,
            "data": serde_json::from_slice::<Value>(&data).unwrap_or(Value::Null),
        }),
        Err(_) => json!({"ok": false, "error": "RDM is not running and could not be started"}),
    }
}

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let (mut input, mut output) = (stdin.lock(), stdout.lock());
    loop {
        match frame::read_message(&mut input) {
            Ok(Some(msg)) => {
                let answer = handle(&msg).to_string();
                if frame::write_message(&mut output, answer.as_bytes()).is_err() {
                    return;
                }
            }
            Ok(None) | Err(_) => return,
        }
    }
}
