//! Integration tests: the real engine downloads from a small mock HTTP server that can
//! misbehave in the ways real servers do (429 with Retry-After, 500, no Range support,
//! a file that changes in the middle, a wrong Content-Range).
//!
//! Run with `cargo test -p rdm-core`.

use rdm_core::engine::{self, BadRange, FileChanged, Job, Progress};
use rdm_core::limiter::Limiter;
use rdm_core::probe;
use rdm_core::reqwest::header::HeaderMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const SIZE: usize = 3 * 1024 * 1024 + 123;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    /// The 2nd..4th request get 429 with Retry-After: 1.
    Throttle429,
    /// The 2nd..4th request get 500.
    Server500,
    /// Range headers are ignored, the answer is always 200.
    NoRange,
    /// From the 4th request on the ETag changes (the file was replaced on the server).
    EtagChanges,
    /// Ranged answers carry a Content-Range that starts one byte too late.
    WrongContentRange,
}

fn make_data() -> Vec<u8> {
    (0..SIZE).map(|i| ((i * 7 + i / 251) % 256) as u8).collect()
}

struct Server {
    url: String,
}

async fn start(mode: Mode) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let data = Arc::new(make_data());
    let hits = Arc::new(AtomicUsize::new(0));
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else {
                return;
            };
            let (data, hits) = (data.clone(), hits.clone());
            tokio::spawn(async move {
                let _ = handle(sock, mode, data, hits).await;
            });
        }
    });
    Server {
        url: format!("http://127.0.0.1:{port}/file.bin"),
    }
}

async fn handle(
    mut sock: TcpStream,
    mode: Mode,
    data: Arc<Vec<u8>>,
    hits: Arc<AtomicUsize>,
) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = sock.read(&mut tmp).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() > 16 * 1024 {
            return Ok(());
        }
    }
    let text = String::from_utf8_lossy(&buf).to_string();
    let header = |name: &str| -> Option<String> {
        text.lines().skip(1).find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case(name)
                .then(|| v.trim().to_string())
        })
    };
    let n = hits.fetch_add(1, Ordering::SeqCst);
    let range = header("range").and_then(|r| {
        let r = r.strip_prefix("bytes=")?.to_string();
        let (a, b) = r.split_once('-')?;
        let a: usize = a.parse().ok()?;
        let b: usize = if b.is_empty() {
            SIZE - 1
        } else {
            b.parse().ok()?
        };
        Some((a, b.min(SIZE - 1)))
    });
    let etag = if mode == Mode::EtagChanges && n >= 3 {
        "\"v2\""
    } else {
        "\"v1\""
    };
    let if_range_ok = header("if-range").map_or(true, |v| v == etag);

    // Misbehaviour first.
    if n >= 1 && n < 4 && range.is_some() {
        if mode == Mode::Throttle429 {
            let head = "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            return sock.write_all(head.as_bytes()).await;
        }
        if mode == Mode::Server500 {
            let head = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            return sock.write_all(head.as_bytes()).await;
        }
    }

    let ranges_ok = mode != Mode::NoRange;
    let accept = if ranges_ok {
        "Accept-Ranges: bytes\r\n"
    } else {
        ""
    };
    match range {
        Some((a, b)) if ranges_ok && if_range_ok && a <= b => {
            let shown_start = if mode == Mode::WrongContentRange && n >= 1 {
                a + 1
            } else {
                a
            };
            let body = &data[a..=b];
            let head = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Type: application/octet-stream\r\n{accept}ETag: {etag}\r\nContent-Range: bytes {shown_start}-{b}/{SIZE}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            sock.write_all(head.as_bytes()).await?;
            sock.write_all(body).await
        }
        _ => {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\n{accept}ETag: {etag}\r\nContent-Length: {SIZE}\r\nConnection: close\r\n\r\n"
            );
            sock.write_all(head.as_bytes()).await?;
            sock.write_all(&data).await
        }
    }
}

struct Outcome {
    result: anyhow::Result<()>,
    out: PathBuf,
    dir: PathBuf,
    secs: f64,
}

async fn download(name: &str, mode: Mode, connections: usize, retries: u32) -> Outcome {
    let server = start(mode).await;
    let dir = std::env::temp_dir().join(format!("rdm_it_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("file.bin");

    let client = probe::build_client(probe::DEFAULT_UA).unwrap();
    let info = probe::probe(&client, &server.url, &HeaderMap::new())
        .await
        .unwrap();
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        engine::run(Job {
            client,
            probe: info,
            out: out.clone(),
            connections,
            retries,
            headers: HeaderMap::new(),
            limiter: Arc::new(Limiter::new(0)),
            item_limiter: Arc::new(Limiter::new(0)),
            use_if_range: true,
            host_cap: 0,
            progress: Arc::new(Progress::default()),
            stop: Arc::new(AtomicBool::new(false)),
        }),
    )
    .await
    .expect("download did not finish in time");
    Outcome {
        result,
        out,
        dir,
        secs: started.elapsed().as_secs_f64(),
    }
}

fn assert_complete(o: &Outcome) {
    assert!(o.result.is_ok(), "download failed: {:?}", o.result);
    assert_eq!(
        std::fs::read(&o.out).unwrap(),
        make_data(),
        "file content differs"
    );
    assert!(
        !engine::part_path(&o.out).exists(),
        "temporary file left behind"
    );
    assert!(
        !engine::state_path(&o.out).exists(),
        "state file left behind"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn normal_ranged_download_is_byte_exact() {
    let o = download("normal", Mode::Normal, 4, 3).await;
    assert_complete(&o);
    let _ = std::fs::remove_dir_all(&o.dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auto_connections_download_is_byte_exact() {
    let o = download("auto", Mode::Normal, 0, 3).await;
    assert_complete(&o);
    let _ = std::fs::remove_dir_all(&o.dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_429_waits_for_retry_after_and_recovers() {
    let o = download("t429", Mode::Throttle429, 4, 8).await;
    assert_complete(&o);
    assert!(
        o.secs >= 0.9,
        "Retry-After was ignored (finished in {:.2}s)",
        o.secs
    );
    let _ = std::fs::remove_dir_all(&o.dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_500_is_retried() {
    let o = download("t500", Mode::Server500, 4, 8).await;
    assert_complete(&o);
    let _ = std::fs::remove_dir_all(&o.dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn server_without_range_support_still_downloads() {
    let o = download("norange", Mode::NoRange, 4, 3).await;
    assert_complete(&o);
    let _ = std::fs::remove_dir_all(&o.dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn file_changed_on_server_is_detected_and_not_merged() {
    let o = download("etag", Mode::EtagChanges, 4, 3).await;
    let err = o
        .result
        .as_ref()
        .expect_err("a changed file must not be accepted");
    assert!(
        err.chain()
            .any(|c| c.downcast_ref::<FileChanged>().is_some()),
        "unexpected error: {err:#}"
    );
    assert!(!o.out.exists(), "a mixed file was published");
    let _ = std::fs::remove_dir_all(&o.dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_content_range_is_rejected() {
    let o = download("badrange", Mode::WrongContentRange, 2, 1).await;
    let err = o
        .result
        .as_ref()
        .expect_err("a wrong Content-Range must not be accepted");
    assert!(
        err.chain().any(|c| c.downcast_ref::<BadRange>().is_some()),
        "unexpected error: {err:#}"
    );
    assert!(!o.out.exists(), "a corrupt file was published");
    let _ = std::fs::remove_dir_all(&o.dir);
}
