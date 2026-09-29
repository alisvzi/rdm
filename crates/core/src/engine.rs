use crate::limiter::Limiter;
use crate::probe::Probe;
use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use reqwest::{header, Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufWriter};
use tokio::time::timeout;

/// Returned (wrapped in anyhow) when a download was stopped on request.
/// Check with `err.downcast_ref::<Stopped>().is_some()`.
#[derive(Debug)]
pub struct Stopped;

impl fmt::Display for Stopped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "download stopped")
    }
}
impl std::error::Error for Stopped {}

fn stopped() -> anyhow::Error {
    anyhow::Error::new(Stopped)
}

/// Live counters that a UI can poll at any time.
#[derive(Default)]
pub struct Progress {
    downloaded: AtomicU64,
    session: AtomicU64,
    total: AtomicU64,
}

impl Progress {
    /// Bytes on disk so far, including what earlier runs already finished.
    pub fn downloaded(&self) -> u64 {
        self.downloaded.load(Ordering::Relaxed)
    }
    /// Bytes received during this run only (used for speed calculation).
    pub fn session(&self) -> u64 {
        self.session.load(Ordering::Relaxed)
    }
    /// 0 when the size is unknown.
    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }
    fn add(&self, n: u64) {
        self.downloaded.fetch_add(n, Ordering::Relaxed);
        self.session.fetch_add(n, Ordering::Relaxed);
    }
    fn init(&self, downloaded: u64, total: u64) {
        self.downloaded.store(downloaded, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
    }
}

pub struct Job {
    pub client: Client,
    /// Result of `probe::probe` for this URL.
    pub probe: Probe,
    /// Final file path. Data is written to `<out>.part` and renamed when complete.
    pub out: PathBuf,
    pub connections: usize,
    pub retries: u32,
    pub headers: header::HeaderMap,
    pub limiter: Arc<Limiter>,
    pub progress: Arc<Progress>,
    /// Set to true to stop (pause) the download. `run` then returns `Stopped`.
    pub stop: Arc<AtomicBool>,
}

/// Persisted next to the .part file so a download can continue after a crash,
/// a lost connection, or closing the program.
#[derive(Serialize, Deserialize)]
struct State {
    total: u64,
    chunk_size: u64,
    validator: Option<String>,
    done: Vec<bool>,
}

enum ChunkError {
    /// Network-level problem: worth retrying.
    Retry(anyhow::Error),
    /// Disk error, stop request, or a server that will never work.
    Fatal(anyhow::Error),
}

struct Shared {
    client: Client,
    url: String,
    headers: header::HeaderMap,
    part_path: PathBuf,
    state_path: PathBuf,
    state: Mutex<State>,
    next_chunk: AtomicUsize,
    limiter: Arc<Limiter>,
    progress: Arc<Progress>,
    stop: Arc<AtomicBool>,
    failed: AtomicBool,
    retries: u32,
    total: u64,
    chunk_size: u64,
    n_chunks: usize,
}

impl Shared {
    fn should_stop(&self) -> bool {
        self.stop.load(Ordering::Relaxed) || self.failed.load(Ordering::Relaxed)
    }
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

pub fn part_path(out: &Path) -> PathBuf {
    with_suffix(out, ".part")
}

pub fn state_path(out: &Path) -> PathBuf {
    with_suffix(out, ".part.json")
}

/// Deletes the temporary files of an unfinished download.
pub fn remove_partial(out: &Path) {
    let _ = std::fs::remove_file(part_path(out));
    let _ = std::fs::remove_file(state_path(out));
    let _ = std::fs::remove_file(with_suffix(&state_path(out), ".tmp"));
}

/// How many bytes an unfinished download already has safely on disk.
pub fn saved_progress(out: &Path) -> Option<u64> {
    let st = load_state(&state_path(out))?;
    if st.chunk_size == 0 || !part_path(out).exists() {
        return None;
    }
    Some(
        st.done
            .iter()
            .enumerate()
            .filter(|(_, d)| **d)
            .map(|(i, _)| chunk_len(i, st.chunk_size, st.total))
            .sum(),
    )
}

fn chunk_len(i: usize, chunk_size: u64, total: u64) -> u64 {
    let start = i as u64 * chunk_size;
    (start + chunk_size).min(total).saturating_sub(start)
}

fn load_state(path: &Path) -> Option<State> {
    let data = std::fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

fn save_state(path: &Path, st: &State) -> Result<()> {
    let tmp = with_suffix(path, ".tmp");
    std::fs::write(&tmp, serde_json::to_vec(st)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub async fn run(job: Job) -> Result<()> {
    let Job {
        client,
        probe,
        out,
        connections,
        retries,
        headers,
        limiter,
        progress,
        stop,
    } = job;

    if stop.load(Ordering::Relaxed) {
        return Err(stopped());
    }

    let part = part_path(&out);
    let spath = state_path(&out);
    let url = probe.url.to_string();

    // Server without range support (or unknown size): one connection, no resume.
    let total = match (probe.ranges, probe.total) {
        (true, Some(t)) => t,
        _ => {
            return single(
                &client, &url, &headers, probe.total, &part, &out, &limiter, &progress, &stop,
            )
            .await
        }
    };

    // Resume if the saved state matches this exact file.
    let resumed = load_state(&spath).filter(|s| {
        s.chunk_size > 0
            && s.total == total
            && s.validator == probe.validator
            && s.done.len() as u64 == total.div_ceil(s.chunk_size)
            && std::fs::metadata(&part).map(|m| m.len() == total).unwrap_or(false)
    });

    let state = match resumed {
        Some(s) => s,
        None => {
            let chunk_size = (total / 64).clamp(512 * 1024, 8 * 1024 * 1024);
            let n = total.div_ceil(chunk_size) as usize;
            let f = std::fs::File::create(&part)
                .with_context(|| format!("cannot create {}", part.display()))?;
            f.set_len(total).context("cannot allocate disk space")?;
            State {
                total,
                chunk_size,
                validator: probe.validator.clone(),
                done: vec![false; n],
            }
        }
    };

    let chunk_size = state.chunk_size;
    let n_chunks = state.done.len();
    let done_bytes: u64 = state
        .done
        .iter()
        .enumerate()
        .filter(|(_, d)| **d)
        .map(|(i, _)| chunk_len(i, chunk_size, total))
        .sum();
    let remaining = state.done.iter().filter(|d| !**d).count();
    progress.init(done_bytes, total);

    let shared = Arc::new(Shared {
        client,
        url,
        headers,
        part_path: part.clone(),
        state_path: spath.clone(),
        state: Mutex::new(state),
        next_chunk: AtomicUsize::new(0),
        limiter,
        progress,
        stop: stop.clone(),
        failed: AtomicBool::new(false),
        retries,
        total,
        chunk_size,
        n_chunks,
    });

    let workers = connections.clamp(1, 64).min(remaining.max(1));
    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let sh = shared.clone();
        handles.push(tokio::spawn(async move { worker(sh).await }));
    }

    let mut real_err: Option<anyhow::Error> = None;
    let mut was_stopped = false;
    for h in handles {
        match h.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if e.downcast_ref::<Stopped>().is_some() {
                    was_stopped = true;
                } else if real_err.is_none() {
                    real_err = Some(e);
                }
            }
            Err(e) => {
                if real_err.is_none() {
                    real_err = Some(anyhow!("worker crashed: {e}"));
                }
            }
        }
    }

    let all_done = shared.state.lock().unwrap().done.iter().all(|d| *d);
    if !all_done {
        if let Some(e) = real_err {
            return Err(e.context("download failed"));
        }
        if was_stopped || stop.load(Ordering::Relaxed) {
            return Err(stopped());
        }
        bail!("download incomplete");
    }

    std::fs::rename(&part, &out)
        .with_context(|| format!("cannot move finished file to {}", out.display()))?;
    let _ = std::fs::remove_file(&spath);
    Ok(())
}

/// Each worker keeps taking the next unfinished chunk from a shared counter.
/// Fast connections naturally end up downloading more chunks than slow ones.
async fn worker(sh: Arc<Shared>) -> Result<()> {
    loop {
        if sh.should_stop() {
            return Err(stopped());
        }
        let i = sh.next_chunk.fetch_add(1, Ordering::Relaxed);
        if i >= sh.n_chunks {
            return Ok(());
        }
        let already_done = { sh.state.lock().unwrap().done[i] };
        if already_done {
            continue;
        }
        if let Err(e) = download_chunk(&sh, i).await {
            if e.downcast_ref::<Stopped>().is_none() {
                sh.failed.store(true, Ordering::Relaxed);
            }
            return Err(e);
        }
    }
}

async fn download_chunk(sh: &Shared, i: usize) -> Result<()> {
    let start = i as u64 * sh.chunk_size;
    let end = (start + sh.chunk_size).min(sh.total) - 1; // inclusive
    let mut pos = start;
    let mut attempt = 0u32;

    while pos <= end {
        if sh.should_stop() {
            return Err(stopped());
        }
        let before = pos;
        match fetch_range(sh, &mut pos, end).await {
            Ok(()) => {}
            Err(ChunkError::Fatal(e)) => return Err(e),
            Err(ChunkError::Retry(e)) => {
                if pos > before {
                    attempt = 0; // we made progress, so start counting again
                }
                attempt += 1;
                if attempt > sh.retries {
                    return Err(e.context(format!(
                        "chunk {} failed after {} retries",
                        i, sh.retries
                    )));
                }
                let delay = 300u64 * (1u64 << attempt.min(6));
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
        }
    }

    let mut st = sh.state.lock().unwrap();
    st.done[i] = true;
    save_state(&sh.state_path, &st)
}

/// Downloads bytes [*pos ..= end] and writes them straight into the .part file.
/// *pos is advanced only for bytes that were handed to the writer, so a retry
/// continues exactly where the connection broke.
async fn fetch_range(sh: &Shared, pos: &mut u64, end: u64) -> Result<(), ChunkError> {
    let resp = sh
        .client
        .get(&sh.url)
        .headers(sh.headers.clone())
        .header(header::RANGE, format!("bytes={}-{}", *pos, end))
        .send()
        .await
        .map_err(|e| ChunkError::Retry(e.into()))?;

    let status = resp.status();
    if status != StatusCode::PARTIAL_CONTENT {
        let err = anyhow!("unexpected HTTP status {} for a range request", status);
        return Err(if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            ChunkError::Retry(err)
        } else {
            ChunkError::Fatal(err)
        });
    }

    let mut file = OpenOptions::new()
        .write(true)
        .open(&sh.part_path)
        .await
        .map_err(|e| ChunkError::Fatal(e.into()))?;
    file.seek(SeekFrom::Start(*pos))
        .await
        .map_err(|e| ChunkError::Fatal(e.into()))?;
    let mut w = BufWriter::with_capacity(256 * 1024, file);

    let mut stream = resp.bytes_stream();
    let mut net_err: Option<anyhow::Error> = None;
    let mut was_stopped = false;

    while *pos <= end {
        if sh.should_stop() {
            was_stopped = true;
            break;
        }
        // Wake up every 500 ms even without data so a pause request is noticed quickly.
        let next = match timeout(Duration::from_millis(500), stream.next()).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        match next {
            Some(Ok(bytes)) => {
                let remaining = end - *pos + 1;
                let take = (bytes.len() as u64).min(remaining) as usize;
                sh.limiter.acquire(take).await;
                w.write_all(&bytes[..take])
                    .await
                    .map_err(|e| ChunkError::Fatal(e.into()))?;
                *pos += take as u64;
                sh.progress.add(take as u64);
            }
            Some(Err(e)) => {
                net_err = Some(e.into());
                break;
            }
            None => {
                net_err = Some(anyhow!("connection closed before the chunk was complete"));
                break;
            }
        }
    }

    // Always flush what we received so far, even if the connection failed.
    w.flush().await.map_err(|e| ChunkError::Fatal(e.into()))?;

    if was_stopped {
        return Err(ChunkError::Fatal(stopped()));
    }
    match net_err {
        Some(e) => Err(ChunkError::Retry(e)),
        None => Ok(()),
    }
}

#[allow(clippy::too_many_arguments)]
async fn single(
    client: &Client,
    url: &str,
    headers: &header::HeaderMap,
    total: Option<u64>,
    part: &Path,
    out: &Path,
    limiter: &Limiter,
    progress: &Progress,
    stop: &AtomicBool,
) -> Result<()> {
    let resp = client
        .get(url)
        .headers(headers.clone())
        .send()
        .await
        .context("could not connect to the server")?;
    if !resp.status().is_success() {
        bail!("server responded with HTTP {}", resp.status());
    }

    let total = total.or(resp.content_length());
    progress.init(0, total.unwrap_or(0));

    let file = tokio::fs::File::create(part).await?;
    let mut w = BufWriter::with_capacity(1 << 20, file);
    let mut stream = resp.bytes_stream();

    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = w.flush().await;
            return Err(stopped());
        }
        let next = match timeout(Duration::from_millis(500), stream.next()).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        match next {
            Some(item) => {
                let bytes = item.context("download interrupted")?;
                limiter.acquire(bytes.len()).await;
                w.write_all(&bytes).await?;
                progress.add(bytes.len() as u64);
            }
            None => break,
        }
    }
    w.flush().await?;
    std::fs::rename(part, out)?;
    Ok(())
}
