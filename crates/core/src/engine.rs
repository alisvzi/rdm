//! Download engine v3.
//!
//! * A central scheduler owns a list of byte ranges ("tasks"). Workers (connections)
//!   take ranges from it. When nothing is pending, an idle worker splits the range that
//!   still has the most bytes left, so one slow connection can never hold the file back.
//! * Every range response is validated strictly (206, Content-Range start/end/total,
//!   Content-Length) before a single byte of it is written.
//! * Progress is checkpointed about once a second (flush + fsync + state file). After a
//!   crash only bytes that were really flushed count as done.
//! * An optional controller adapts the number of active connections to the measured
//!   aggregate throughput and to throttling answers (429/503/connection failures).
//! * Everything the UI shows (connections, ranges, speeds, retries) is read from this
//!   live state, nothing is simulated.

use crate::limiter::Limiter;
use crate::probe::Probe;
use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use reqwest::{header, Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufWriter};
use tokio::time::timeout;

const MIB: u64 = 1024 * 1024;
/// A range is only split when at least two of these are left in it.
const MIN_SPLIT: u64 = 512 * 1024;
/// Flush + record progress after this many bytes per connection.
const COMMIT_EVERY: u64 = MIB;

const PHASE_PREPARING: u8 = 0;
const PHASE_CONNECTING: u8 = 1;
const PHASE_DOWNLOADING: u8 = 2;
const PHASE_VERIFYING: u8 = 3;
const PHASE_FINALIZING: u8 = 4;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/* ====================================================================== */
/* errors                                                                  */
/* ====================================================================== */

/// Returned (wrapped in anyhow) when a download was stopped on request.
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

/// The file on the server is no longer the one we started downloading.
#[derive(Debug)]
pub struct FileChanged(pub String);

impl fmt::Display for FileChanged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the file changed on the server: {}", self.0)
    }
}
impl std::error::Error for FileChanged {}

#[derive(Debug)]
pub struct HttpStatus(pub u16);

impl fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", http_text(self.0))
    }
}
impl std::error::Error for HttpStatus {}

#[derive(Debug)]
pub struct BadRange(pub String);

impl fmt::Display for BadRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid Range response: {}", self.0)
    }
}
impl std::error::Error for BadRange {}

fn http_text(code: u16) -> String {
    match code {
        401 => "HTTP 401 — authorization required".to_string(),
        403 => "HTTP 403 — access denied (the link or login session may have expired)".to_string(),
        404 => "HTTP 404 — file not found".to_string(),
        408 => "HTTP 408 — the server timed out waiting for the request".to_string(),
        410 => "HTTP 410 — the file is no longer available".to_string(),
        416 => "HTTP 416 — the requested range is not valid for this file".to_string(),
        429 => "HTTP 429 — too many requests (the server is throttling)".to_string(),
        500 => "HTTP 500 — server error".to_string(),
        502 => "HTTP 502 — bad gateway".to_string(),
        503 => "HTTP 503 — server temporarily unavailable".to_string(),
        504 => "HTTP 504 — gateway timeout".to_string(),
        c => format!("HTTP {c}"),
    }
}

/// Turns any engine error into a short human-readable sentence.
pub fn explain(e: &anyhow::Error) -> String {
    for cause in e.chain() {
        if let Some(h) = cause.downcast_ref::<HttpStatus>() {
            return http_text(h.0);
        }
        if let Some(c) = cause.downcast_ref::<FileChanged>() {
            return format!(
                "File changed on the server ({}). The partial download was discarded; restart the download.",
                c.0
            );
        }
        if let Some(b) = cause.downcast_ref::<BadRange>() {
            return format!("Invalid Range response: {}", b.0);
        }
        if let Some(r) = cause.downcast_ref::<reqwest::Error>() {
            if r.is_timeout() {
                return "Network timeout".to_string();
            }
            if r.is_connect() {
                return "Cannot connect to the server".to_string();
            }
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            if matches!(io.raw_os_error(), Some(112) | Some(28)) {
                return "Insufficient disk space".to_string();
            }
            let msg = match io.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    Some("Permission denied — the file or folder is protected or locked")
                }
                std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::BrokenPipe => Some("Connection reset"),
                std::io::ErrorKind::TimedOut => Some("Network timeout"),
                std::io::ErrorKind::NotFound => Some("File or folder not found"),
                std::io::ErrorKind::AlreadyExists => Some(
                    "A file with this name already exists; the finished file was not overwritten",
                ),
                _ => None,
            };
            if let Some(m) = msg {
                return m.to_string();
            }
        }
    }
    format!("{:#}", e)
}

/* ====================================================================== */
/* live state shared with the UI                                           */
/* ====================================================================== */

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConnStatus {
    Waiting,
    Connecting,
    Receiving,
    Retrying,
    Completed,
    Failed,
}

impl ConnStatus {
    fn name(self) -> &'static str {
        match self {
            ConnStatus::Waiting => "waiting",
            ConnStatus::Connecting => "connecting",
            ConnStatus::Receiving => "receiving",
            ConnStatus::Retrying => "retrying",
            ConnStatus::Completed => "completed",
            ConnStatus::Failed => "failed",
        }
    }
}

struct ConnInfo {
    status: ConnStatus,
    task: Option<usize>,
    last_start: u64,
    last_end: u64,
    total_bytes: u64,
    speed: f64,
    win_bytes: u64,
    win_t: Instant,
    t0: Instant,
    retries: u32,
    http_status: u16,
    started_at: u64,
    last_activity: u64,
    retry_in_ms: u64,
    segments_done: u32,
    error: Option<String>,
}

impl ConnInfo {
    fn new() -> Self {
        Self {
            status: ConnStatus::Waiting,
            task: None,
            last_start: 0,
            last_end: 0,
            total_bytes: 0,
            speed: 0.0,
            win_bytes: 0,
            win_t: Instant::now(),
            t0: Instant::now(),
            retries: 0,
            http_status: 0,
            started_at: 0,
            last_activity: 0,
            retry_in_ms: 0,
            segments_done: 0,
            error: None,
        }
    }

    fn touch(&mut self) {
        self.last_activity = now_unix();
        if self.started_at == 0 {
            self.started_at = self.last_activity;
        }
    }

    fn note_bytes(&mut self, n: u64) {
        self.total_bytes += n;
        self.win_bytes += n;
        self.touch();
        let dt = self.win_t.elapsed().as_secs_f64();
        if dt >= 0.5 {
            let inst = self.win_bytes as f64 / dt;
            self.speed = if self.speed <= 0.0 {
                inst
            } else {
                self.speed * 0.5 + inst * 0.5
            };
            self.win_bytes = 0;
            self.win_t = Instant::now();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TaskStatus {
    Pending,
    Active,
    Completed,
}

/// One byte range of the file: `start..=end` (inclusive).
struct Task {
    start: u64,
    end: u64,
    /// Next byte that has not been handed to a writer yet.
    pos: u64,
    /// Every byte before this one is flushed to the file.
    committed: u64,
    status: TaskStatus,
    attempts: u32,
    conn: Option<usize>,
    /// The last attempt on this range failed for good (until it is picked up again).
    failed: bool,
}

impl Task {
    fn pending(start: u64, end: u64) -> Self {
        Self {
            start,
            end,
            pos: start,
            committed: start,
            status: TaskStatus::Pending,
            attempts: 0,
            conn: None,
            failed: false,
        }
    }
    fn completed(start: u64, end: u64) -> Self {
        Self {
            start,
            end,
            pos: end + 1,
            committed: end + 1,
            status: TaskStatus::Completed,
            attempts: 0,
            conn: None,
            failed: false,
        }
    }
}

/// Sorts, clamps and merges byte ranges (inclusive ends).
fn merge_ranges(mut v: Vec<(u64, u64)>, total: u64) -> Vec<(u64, u64)> {
    v.retain(|(s, e)| s <= e && *s < total);
    v.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::new();
    for (s, e) in v {
        let e = e.min(total.saturating_sub(1));
        match out.last_mut() {
            Some(last) if s <= last.1.saturating_add(1) => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

/// The central range scheduler plus the per-connection live info.
pub struct Sched {
    tasks: Vec<Task>,
    conns: Vec<ConnInfo>,
    total: u64,
}

impl Sched {
    fn new(total: u64, done: &[(u64, u64)], conns: usize) -> Self {
        let mut tasks = Vec::new();
        let mut cursor = 0u64;
        for &(s, e) in done {
            if s > cursor {
                tasks.push(Task::pending(cursor, s - 1));
            }
            tasks.push(Task::completed(s, e));
            cursor = e + 1;
        }
        if cursor < total {
            tasks.push(Task::pending(cursor, total - 1));
        }
        Self {
            tasks,
            conns: (0..conns.max(1)).map(|_| ConnInfo::new()).collect(),
            total,
        }
    }

    fn all_done(&self) -> bool {
        self.tasks.iter().all(|t| t.status == TaskStatus::Completed)
    }

    /// Gives `conn` something to download: a pending range, or half of the biggest
    /// range that is still being downloaded by someone else.
    fn acquire(&mut self, conn: usize) -> Option<usize> {
        let pending = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.status == TaskStatus::Pending && t.pos <= t.end)
            .min_by_key(|(_, t)| t.pos)
            .map(|(i, _)| i);

        let id = if let Some(i) = pending {
            let t = &mut self.tasks[i];
            t.status = TaskStatus::Active;
            t.conn = Some(conn);
            t.failed = false;
            i
        } else {
            let victim = self
                .tasks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.status == TaskStatus::Active)
                .map(|(i, t)| (i, t.end + 1 - t.pos))
                .filter(|(_, rem)| *rem >= 2 * MIN_SPLIT)
                .max_by_key(|(_, rem)| *rem);
            let (vi, rem) = victim?;
            let mid = self.tasks[vi].pos + rem / 2;
            let old_end = self.tasks[vi].end;
            self.tasks[vi].end = mid - 1;
            self.tasks.push(Task {
                start: mid,
                end: old_end,
                pos: mid,
                committed: mid,
                status: TaskStatus::Active,
                attempts: 0,
                conn: Some(conn),
                failed: false,
            });
            self.tasks.len() - 1
        };

        let (start, end) = (self.tasks[id].start, self.tasks[id].end);
        let c = &mut self.conns[conn];
        c.task = Some(id);
        c.last_start = start;
        c.last_end = end;
        Some(id)
    }

    /// Reserves up to `want` bytes at the current position of range `tid`.
    /// Returns (offset, length, range finished). `None` means the range is exhausted
    /// (another connection took over its tail).
    fn reserve(&mut self, tid: usize, conn: usize, want: u64) -> Option<(u64, u64, bool)> {
        let t = &mut self.tasks[tid];
        if t.pos > t.end {
            return None;
        }
        let len = want.min(t.end + 1 - t.pos);
        let pos = t.pos;
        t.pos += len;
        let finished = t.pos > t.end;
        self.conns[conn].note_bytes(len);
        Some((pos, len, finished))
    }

    fn commit(&mut self, tid: usize, upto: u64) {
        let t = &mut self.tasks[tid];
        if upto > t.committed {
            t.committed = upto.min(t.end + 1);
        }
    }

    fn finish(&mut self, tid: usize) {
        let t = &mut self.tasks[tid];
        t.status = TaskStatus::Completed;
        t.committed = t.end + 1;
        t.pos = t.end + 1;
        let conn = t.conn.take();
        if let Some(c) = conn {
            let ci = &mut self.conns[c];
            ci.task = None;
            ci.segments_done += 1;
            ci.status = ConnStatus::Completed;
            ci.error = None;
        }
    }

    /// Gives the range back so another connection (or this one, later) can continue it.
    fn release(&mut self, tid: usize, failed: bool) {
        let t = &mut self.tasks[tid];
        // Only flushed bytes are trusted; anything after `committed` is downloaded again.
        t.pos = t.committed.max(t.start);
        if t.pos > t.end {
            t.status = TaskStatus::Completed;
        } else {
            t.status = TaskStatus::Pending;
            t.failed = failed;
        }
        let conn = t.conn.take();
        if let Some(c) = conn {
            self.conns[c].task = None;
        }
    }

    /// Flushed byte ranges (what a restart may trust).
    fn done_ranges(&self) -> Vec<(u64, u64)> {
        let v: Vec<(u64, u64)> = self
            .tasks
            .iter()
            .filter_map(|t| match t.status {
                TaskStatus::Completed => Some((t.start, t.end)),
                _ if t.committed > t.start => Some((t.start, t.committed - 1)),
                _ => None,
            })
            .collect();
        merge_ranges(v, self.total)
    }

    /// Completion (0..=100) of up to `buckets` equal slices of the file, from the real ranges.
    fn map(&self, buckets: usize) -> Vec<u8> {
        if self.total == 0 {
            return Vec::new();
        }
        let b = (buckets as u64).min(self.total).max(1);
        (0..b)
            .map(|k| {
                let a = k * self.total / b;
                let z = ((k + 1) * self.total / b).max(a + 1);
                let mut covered = 0u64;
                for t in &self.tasks {
                    let hi = if t.status == TaskStatus::Completed {
                        t.end + 1
                    } else {
                        t.pos
                    };
                    if hi > t.start {
                        let s = t.start.max(a);
                        let e = hi.min(z);
                        if e > s {
                            covered += e - s;
                        }
                    }
                }
                ((covered * 100) / (z - a)).min(100) as u8
            })
            .collect()
    }
}

/// One connection as the UI sees it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    pub id: usize,
    pub status: String,
    pub start: u64,
    pub end: u64,
    /// Next byte this connection will fetch inside its range.
    pub pos: u64,
    /// Bytes of the current (or last) range that are already received.
    pub downloaded: u64,
    /// Everything this connection ever received.
    pub total_bytes: u64,
    pub speed: u64,
    pub avg_speed: u64,
    pub retries: u32,
    pub http_status: u16,
    pub started_at: u64,
    pub last_activity: u64,
    pub retry_in_ms: u64,
    pub segments_done: u32,
    pub error: Option<String>,
}

/// A consistent copy of the engine's live state for the UI.
#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EngineSnapshot {
    pub phase: String,
    pub connections: Vec<ConnectionView>,
    /// Real per-slice completion of the file (0..=100 each).
    pub map: Vec<u8>,
    pub segments_total: u32,
    pub segments_completed: u32,
    pub segments_active: u32,
    pub segments_failed: u32,
    pub segments_pending: u32,
    pub active_connections: u32,
    pub target_connections: u32,
    pub max_connections: u32,
    pub retries: u32,
    pub retry_limit: u32,
    pub http_status: u16,
    pub last_activity: u64,
}

/// Live counters that a UI can poll at any time.
#[derive(Default)]
pub struct Progress {
    downloaded: AtomicU64,
    session: AtomicU64,
    total: AtomicU64,
    retries: AtomicU32,
    retry_limit: AtomicU32,
    http_status: AtomicU32,
    phase: AtomicU8,
    target_conns: AtomicUsize,
    max_conns: AtomicUsize,
    /// Lowest connection count the server forced us down to (0 = never throttled).
    throttled: AtomicUsize,
    note: Mutex<String>,
    sched: OnceLock<Arc<Mutex<Sched>>>,
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
    /// Retries so far in this run.
    pub fn retries(&self) -> u32 {
        self.retries.load(Ordering::Relaxed)
    }
    /// Connection count the server pushed us down to with 429/503/refusals; 0 = none.
    pub fn throttled_cap(&self) -> usize {
        self.throttled.load(Ordering::Relaxed)
    }
    /// Explanation shown when a resume was not possible.
    pub fn note(&self) -> String {
        self.note.lock().unwrap().clone()
    }

    fn add(&self, n: u64) {
        self.downloaded.fetch_add(n, Ordering::Relaxed);
        self.session.fetch_add(n, Ordering::Relaxed);
    }

    /// Used by the HLS downloader: bytes written so far, total size not known.
    pub(crate) fn add_bytes(&self, n: u64) {
        self.add(n);
    }
    pub(crate) fn begin_unknown_size(&self) {
        self.init(0, 0);
        self.set_phase(PHASE_DOWNLOADING);
    }
    pub(crate) fn count_retry(&self) {
        self.retries.fetch_add(1, Ordering::Relaxed);
    }
    fn init(&self, downloaded: u64, total: u64) {
        self.downloaded.store(downloaded, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
    }
    fn set_phase(&self, p: u8) {
        self.phase.store(p, Ordering::Relaxed);
    }
    fn advance_phase(&self, from: u8, to: u8) {
        let _ = self
            .phase
            .compare_exchange(from, to, Ordering::Relaxed, Ordering::Relaxed);
    }
    fn set_note(&self, text: &str) {
        *self.note.lock().unwrap() = text.to_string();
    }

    /// Copies the live state: connections, ranges, speeds, retries, phase.
    pub fn snapshot(&self, map_buckets: usize) -> EngineSnapshot {
        let raw_phase = self.phase.load(Ordering::Relaxed);
        let mut snap = EngineSnapshot {
            retries: self.retries.load(Ordering::Relaxed),
            retry_limit: self.retry_limit.load(Ordering::Relaxed),
            http_status: self.http_status.load(Ordering::Relaxed) as u16,
            target_connections: self.target_conns.load(Ordering::Relaxed) as u32,
            max_connections: self.max_conns.load(Ordering::Relaxed) as u32,
            ..EngineSnapshot::default()
        };

        let mut any_receiving = false;
        let mut any_retrying = false;
        let mut any_connecting = false;

        if let Some(sh) = self.sched.get() {
            let s = sh.lock().unwrap();
            snap.map = s.map(map_buckets);
            snap.segments_total = s.tasks.len() as u32;
            for t in &s.tasks {
                match t.status {
                    TaskStatus::Completed => snap.segments_completed += 1,
                    TaskStatus::Active => snap.segments_active += 1,
                    TaskStatus::Pending if t.failed => snap.segments_failed += 1,
                    TaskStatus::Pending => snap.segments_pending += 1,
                }
            }
            for (i, c) in s.conns.iter().enumerate() {
                let (start, end, done) = match c.task {
                    Some(t) => {
                        let t = &s.tasks[t];
                        (t.start, t.end, t.pos - t.start)
                    }
                    None => {
                        let len = if c.last_end >= c.last_start && c.status == ConnStatus::Completed
                        {
                            c.last_end + 1 - c.last_start
                        } else {
                            0
                        };
                        (c.last_start, c.last_end, len)
                    }
                };
                let receiving = c.status == ConnStatus::Receiving;
                any_receiving |= receiving;
                any_retrying |= c.status == ConnStatus::Retrying;
                any_connecting |= c.status == ConnStatus::Connecting;
                if matches!(
                    c.status,
                    ConnStatus::Receiving | ConnStatus::Connecting | ConnStatus::Retrying
                ) {
                    snap.active_connections += 1;
                }
                let fresh = c.win_t.elapsed().as_secs_f64() < 2.0;
                let avg = c.total_bytes as f64 / c.t0.elapsed().as_secs_f64().max(0.001);
                snap.last_activity = snap.last_activity.max(c.last_activity);
                snap.connections.push(ConnectionView {
                    id: i + 1,
                    status: c.status.name().to_string(),
                    start,
                    end,
                    pos: start + done,
                    downloaded: done,
                    total_bytes: c.total_bytes,
                    speed: if receiving && fresh {
                        c.speed as u64
                    } else {
                        0
                    },
                    avg_speed: avg as u64,
                    retries: c.retries,
                    http_status: c.http_status,
                    started_at: c.started_at,
                    last_activity: c.last_activity,
                    retry_in_ms: c.retry_in_ms,
                    segments_done: c.segments_done,
                    error: c.error.clone(),
                });
            }
        }

        snap.phase = match raw_phase {
            PHASE_PREPARING => "preparing",
            PHASE_CONNECTING => "connecting",
            PHASE_VERIFYING => "verifying",
            PHASE_FINALIZING => "finalizing",
            _ => {
                if any_receiving {
                    "downloading"
                } else if any_retrying {
                    "retrying"
                } else if any_connecting {
                    "connecting"
                } else {
                    "downloading"
                }
            }
        }
        .to_string();
        snap
    }
}

/* ====================================================================== */
/* job + resume state                                                      */
/* ====================================================================== */

pub struct Job {
    pub client: Client,
    /// Result of `probe::probe` for this URL.
    pub probe: Probe,
    /// Final file path. Data is written to `<out>.part` and renamed when complete.
    pub out: PathBuf,
    /// 0 = automatic (the engine adapts the number of connections), otherwise 1..=64.
    pub connections: usize,
    pub retries: u32,
    pub headers: header::HeaderMap,
    /// Global speed limiter shared by all downloads.
    pub limiter: Arc<Limiter>,
    /// Speed limiter of this download only.
    pub item_limiter: Arc<Limiter>,
    /// Send If-Range with the validator so a changed file is detected on every request.
    pub use_if_range: bool,
    /// Upper limit of connections remembered for this host (0 = none). Only used in auto mode.
    pub host_cap: usize,
    pub progress: Arc<Progress>,
    /// Set to true to stop (pause) the download. `run` then returns `Stopped`.
    pub stop: Arc<AtomicBool>,
}

/// Persisted next to the .part file. Only ranges that were flushed to disk are listed.
#[derive(Serialize, Deserialize)]
struct State {
    #[serde(default)]
    version: u32,
    total: u64,
    validator: Option<String>,
    ranges: Vec<(u64, u64)>,
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
    // The temporary file must exist and have the full size; otherwise the saved ranges
    // cannot be trusted (crash, manual deletion, disk problem) and the engine restarts.
    let len = std::fs::metadata(part_path(out)).ok()?.len();
    if len != st.total {
        return None;
    }
    Some(st.ranges.iter().map(|(s, e)| e - s + 1).sum())
}

fn load_state(path: &Path) -> Option<State> {
    let data = std::fs::read(path).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&data).ok()?;
    if v.get("ranges").is_some() {
        return serde_json::from_value(v).ok();
    }
    // Older versions stored fixed-size chunks: convert them to ranges.
    let total = v.get("total")?.as_u64()?;
    let chunk = v.get("chunk_size")?.as_u64()?;
    if chunk == 0 {
        return None;
    }
    let done = v.get("done")?.as_array()?;
    let validator = v
        .get("validator")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let mut ranges = Vec::new();
    for (i, d) in done.iter().enumerate() {
        if d.as_bool() == Some(true) {
            let s = i as u64 * chunk;
            let e = (s + chunk).min(total).saturating_sub(1);
            if e >= s {
                ranges.push((s, e));
            }
        }
    }
    Some(State {
        version: 2,
        total,
        validator,
        ranges: merge_ranges(ranges, total),
    })
}

fn save_state(path: &Path, st: &State) -> Result<()> {
    let tmp = with_suffix(path, ".tmp");
    std::fs::write(&tmp, serde_json::to_vec(st)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Crash-safe checkpoint: force the data to disk first, then record which ranges are valid.
fn checkpoint(
    part: &Path,
    spath: &Path,
    total: u64,
    validator: &Option<String>,
    ranges: Vec<(u64, u64)>,
) {
    if let Ok(f) = std::fs::OpenOptions::new().write(true).open(part) {
        let _ = f.sync_data();
    }
    let st = State {
        version: 2,
        total,
        validator: validator.clone(),
        ranges,
    };
    let _ = save_state(spath, &st);
}

/* ====================================================================== */
/* shared state of one run                                                 */
/* ====================================================================== */

enum ChunkError {
    /// Network-level problem or a bad answer: worth retrying.
    Retry {
        err: anyhow::Error,
        wait: Option<Duration>,
        /// Counts as pressure for the adaptive connection controller.
        pressure: bool,
    },
    /// Disk error, stop request, or a server that will never work.
    Fatal(anyhow::Error),
}

impl ChunkError {
    fn retry(err: anyhow::Error) -> Self {
        ChunkError::Retry {
            err,
            wait: None,
            pressure: true,
        }
    }
}

fn bad_range(msg: &str) -> ChunkError {
    ChunkError::Retry {
        err: anyhow::Error::new(BadRange(msg.to_string())),
        wait: None,
        pressure: false,
    }
}

fn fatal_io(e: std::io::Error) -> ChunkError {
    ChunkError::Fatal(anyhow::Error::new(e))
}

fn retry_after(headers: &header::HeaderMap) -> Option<Duration> {
    headers
        .get(header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|s| Duration::from_secs(s.min(120)))
}

/// Decides what a non-206 answer to a Range request means.
fn classify_status(status: StatusCode, headers: &header::HeaderMap) -> ChunkError {
    let code = status.as_u16();
    match code {
        200 => ChunkError::Fatal(anyhow::Error::new(FileChanged(
            "the server ignored the Range request, or the file was replaced".to_string(),
        ))),
        429 | 503 => ChunkError::Retry {
            err: anyhow::Error::new(HttpStatus(code)),
            wait: retry_after(headers),
            pressure: true,
        },
        408 | 500 | 502 | 504 => ChunkError::retry(anyhow::Error::new(HttpStatus(code))),
        c if c >= 500 => ChunkError::retry(anyhow::Error::new(HttpStatus(c))),
        c => ChunkError::Fatal(anyhow::Error::new(HttpStatus(c))),
    }
}

fn backoff(attempt: u32) -> Duration {
    let exp = attempt.clamp(1, 7) - 1;
    let base = (500u64 << exp).min(30_000);
    let jitter = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
        % (base / 4 + 1);
    Duration::from_millis(base + jitter)
}

fn parse_content_range(s: &str) -> Option<(u64, u64, Option<u64>)> {
    let rest = s.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (a, b) = range.trim().split_once('-')?;
    let start = a.trim().parse().ok()?;
    let end = b.trim().parse().ok()?;
    let total = match total.trim() {
        "*" => None,
        t => Some(t.parse().ok()?),
    };
    Some((start, end, total))
}

struct Shared {
    client: Client,
    url: String,
    headers: header::HeaderMap,
    part_path: PathBuf,
    state_path: PathBuf,
    sched: Arc<Mutex<Sched>>,
    limiter: Arc<Limiter>,
    item_limiter: Arc<Limiter>,
    progress: Arc<Progress>,
    stop: Arc<AtomicBool>,
    failed: AtomicBool,
    retries: u32,
    total: u64,
    validator: Option<String>,
    if_range: Option<String>,
    /// How many workers may be active right now (the controller changes this).
    target: AtomicUsize,
    /// Throttling / connection-failure events since the controller last looked.
    pressure: AtomicU32,
}

impl Shared {
    fn should_stop(&self) -> bool {
        self.stop.load(Ordering::Relaxed) || self.failed.load(Ordering::Relaxed)
    }
}

/* ====================================================================== */
/* run                                                                     */
/* ====================================================================== */

pub async fn run(job: Job) -> Result<()> {
    let Job {
        client,
        probe,
        out,
        connections,
        retries,
        headers,
        limiter,
        item_limiter,
        use_if_range,
        host_cap,
        progress,
        stop,
    } = job;

    if stop.load(Ordering::Relaxed) {
        return Err(stopped());
    }
    progress.set_phase(PHASE_CONNECTING);
    progress.retry_limit.store(retries, Ordering::Relaxed);

    let part = part_path(&out);
    let spath = state_path(&out);
    let url = probe.url.to_string();

    // Server without range support (or unknown size): one connection, no resume.
    let total = match (probe.ranges, probe.total) {
        (true, Some(t)) => t,
        _ => {
            progress.set_note("The server does not support resuming (no Range support), so this download uses a single connection and cannot be resumed.");
            return single(
                &client,
                &url,
                &headers,
                probe.total,
                &part,
                &out,
                &limiter,
                &item_limiter,
                &progress,
                &stop,
            )
            .await;
        }
    };

    // Resume only when the saved state describes exactly this file.
    let saved = load_state(&spath);
    let mut note = String::new();
    let done_ranges: Vec<(u64, u64)> = match saved {
        Some(s) => {
            let part_ok = std::fs::metadata(&part)
                .map(|m| m.len() == total)
                .unwrap_or(false);
            if s.total != total {
                note = "Resume is not possible: the file size on the server changed. The download restarted from the beginning.".to_string();
                Vec::new()
            } else if s.validator != probe.validator {
                note = "Resume is not possible: the file's ETag/Last-Modified changed on the server. The download restarted from the beginning.".to_string();
                Vec::new()
            } else if !part_ok {
                note = "Resume is not possible: the partial file is missing or has the wrong size. The download restarted from the beginning.".to_string();
                Vec::new()
            } else {
                merge_ranges(s.ranges, total)
            }
        }
        None => Vec::new(),
    };
    if !note.is_empty() {
        progress.set_note(&note);
    }

    if done_ranges.is_empty() {
        if let Some(free) = crate::fsutil::free_space(&part) {
            if free < total.saturating_add(16 << 20) {
                bail!(
                    "not enough disk space: {} MiB needed, {} MiB free",
                    total >> 20,
                    free >> 20
                );
            }
        }
        let f = std::fs::File::create(&part)
            .with_context(|| format!("cannot create {}", part.display()))?;
        f.set_len(total).context("cannot allocate disk space")?;
    }

    let auto = connections == 0;
    let max_conns = if auto {
        if host_cap > 0 {
            host_cap.clamp(1, 16)
        } else {
            16
        }
    } else {
        connections.clamp(1, 64)
    };
    let start_target = if auto { 4.min(max_conns) } else { max_conns };

    let done_bytes: u64 = done_ranges.iter().map(|(s, e)| e - s + 1).sum();
    progress.init(done_bytes, total);
    progress.max_conns.store(max_conns, Ordering::Relaxed);
    progress.target_conns.store(start_target, Ordering::Relaxed);

    let sched = Arc::new(Mutex::new(Sched::new(total, &done_ranges, max_conns)));
    let _ = progress.sched.set(sched.clone());

    let if_range = if use_if_range {
        match (&probe.etag, &probe.last_modified) {
            (Some(e), _) if !e.starts_with("W/") => Some(e.clone()),
            (_, Some(lm)) => Some(lm.clone()),
            _ => None,
        }
    } else {
        None
    };

    let shared = Arc::new(Shared {
        client,
        url,
        headers,
        part_path: part.clone(),
        state_path: spath.clone(),
        sched,
        limiter,
        item_limiter,
        progress: progress.clone(),
        stop: stop.clone(),
        failed: AtomicBool::new(false),
        retries,
        total,
        validator: probe.validator.clone(),
        if_range,
        target: AtomicUsize::new(start_target),
        pressure: AtomicU32::new(0),
    });

    let mut handles = Vec::with_capacity(max_conns);
    if !shared.sched.lock().unwrap().all_done() {
        for idx in 0..max_conns {
            let sh = shared.clone();
            handles.push(tokio::spawn(async move { worker(sh, idx).await }));
        }
    }
    let persist_handle = tokio::spawn(persist_loop(shared.clone()));
    let controller_handle = if auto {
        Some(tokio::spawn(controller(shared.clone(), max_conns)))
    } else {
        None
    };

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
    persist_handle.abort();
    if let Some(h) = controller_handle {
        h.abort();
    }

    let (all_done, ranges) = {
        let s = shared.sched.lock().unwrap();
        (s.all_done(), s.done_ranges())
    };

    if !all_done {
        // Leave a valid checkpoint behind so the next start continues from here.
        checkpoint(&part, &spath, total, &shared.validator, ranges);
        if let Some(e) = real_err {
            if e.downcast_ref::<FileChanged>().is_some() {
                remove_partial(&out);
                return Err(e.context("start the download again"));
            }
            return Err(e.context("download failed"));
        }
        if was_stopped || stop.load(Ordering::Relaxed) {
            return Err(stopped());
        }
        bail!("download incomplete");
    }

    progress.set_phase(PHASE_VERIFYING);
    // Integrity: the finished file must have exactly the size the server announced.
    match std::fs::metadata(&part) {
        Ok(m) if m.len() == total => {}
        Ok(m) => bail!(
            "file size mismatch: expected {} bytes but found {}",
            total,
            m.len()
        ),
        Err(e) => bail!("cannot read the finished file: {e}"),
    }

    progress.set_phase(PHASE_FINALIZING);
    if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&part) {
        let _ = f.sync_all();
    }
    crate::fsutil::safe_rename(&part, &out)
        .with_context(|| format!("cannot move the finished file to {}", out.display()))?;
    let _ = std::fs::remove_file(&spath);
    Ok(())
}

/// Writes a checkpoint about once a second while the download runs.
async fn persist_loop(sh: Arc<Shared>) {
    let mut last: Vec<(u64, u64)> = Vec::new();
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let ranges = sh.sched.lock().unwrap().done_ranges();
        if ranges != last {
            let part = sh.part_path.clone();
            let spath = sh.state_path.clone();
            let validator = sh.validator.clone();
            let total = sh.total;
            let r = ranges.clone();
            let _ = tokio::task::spawn_blocking(move || {
                checkpoint(&part, &spath, total, &validator, r);
            })
            .await;
            last = ranges;
        }
    }
}

/// Adapts the number of active connections to the measured throughput.
async fn controller(sh: Arc<Shared>, max: usize) {
    const LADDER: [usize; 9] = [1, 2, 4, 6, 8, 12, 16, 24, 32];
    fn up(t: usize, cap: usize) -> usize {
        LADDER
            .iter()
            .copied()
            .find(|&x| x > t && x <= cap)
            .unwrap_or(t)
    }
    fn down(t: usize) -> usize {
        LADDER.iter().rev().copied().find(|&x| x < t).unwrap_or(1)
    }

    let mut target = sh.target.load(Ordering::Relaxed);
    let mut cap = max;
    let mut last_bytes = sh.progress.session();
    let mut prev_rate = 0f64;
    let mut last_was_up = false;
    let mut cooldown = 1u32;

    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;
        if sh.should_stop() {
            return;
        }
        let cur = sh.progress.session();
        let rate = cur.saturating_sub(last_bytes) as f64 / 3.0;
        last_bytes = cur;
        let pressure = sh.pressure.swap(0, Ordering::Relaxed);

        if pressure > 0 {
            // The server is throttling or refusing connections: back off and stay lower.
            let lower = down(target);
            cap = lower;
            target = lower;
            cooldown = 3;
            last_was_up = false;
            let known = sh.progress.throttled.load(Ordering::Relaxed);
            if known == 0 || lower < known {
                sh.progress.throttled.store(lower, Ordering::Relaxed);
            }
        } else if cooldown > 0 {
            cooldown -= 1;
        } else if last_was_up && rate < prev_rate * 0.9 {
            // The last increase made the aggregate speed worse: go back and stop growing.
            let lower = down(target);
            cap = lower;
            target = lower;
            cooldown = 3;
            last_was_up = false;
        } else if last_was_up && rate < prev_rate * 1.05 {
            // No real gain from the last increase: this is enough.
            cap = target;
            last_was_up = false;
        } else if target < cap && rate > 0.0 {
            let next = up(target, cap);
            if next > target {
                prev_rate = rate;
                target = next;
                last_was_up = true;
            }
        } else {
            last_was_up = false;
        }

        sh.target.store(target, Ordering::Relaxed);
        sh.progress.target_conns.store(target, Ordering::Relaxed);
    }
}

fn set_conn(sh: &Shared, idx: usize, f: impl FnOnce(&mut ConnInfo)) {
    let mut s = sh.sched.lock().unwrap();
    f(&mut s.conns[idx]);
}

async fn worker(sh: Arc<Shared>, idx: usize) -> Result<()> {
    loop {
        if sh.should_stop() {
            return Err(stopped());
        }
        if sh.sched.lock().unwrap().all_done() {
            return Ok(());
        }
        if idx >= sh.target.load(Ordering::Relaxed) {
            // Parked by the controller: hold no connection while waiting.
            set_conn(&sh, idx, |c| c.status = ConnStatus::Waiting);
            tokio::time::sleep(Duration::from_millis(300)).await;
            continue;
        }
        let tid = { sh.sched.lock().unwrap().acquire(idx) };
        match tid {
            None => {
                set_conn(&sh, idx, |c| {
                    if c.status != ConnStatus::Failed {
                        c.status = ConnStatus::Waiting;
                    }
                });
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Some(tid) => {
                if let Err(e) = run_task(&sh, idx, tid).await {
                    if e.downcast_ref::<Stopped>().is_none() {
                        sh.failed.store(true, Ordering::Relaxed);
                    }
                    return Err(e);
                }
            }
        }
    }
}

enum Outcome {
    Done,
    Parked,
}

/// Downloads one range, retrying with backoff according to the error rules.
async fn run_task(sh: &Shared, idx: usize, tid: usize) -> Result<()> {
    let mut attempt = 0u32;
    loop {
        if sh.should_stop() {
            sh.sched.lock().unwrap().release(tid, false);
            return Err(stopped());
        }
        let before = sh.sched.lock().unwrap().tasks[tid].pos;
        match fetch_into(sh, idx, tid).await {
            Ok(Outcome::Done) => {
                sh.sched.lock().unwrap().finish(tid);
                return Ok(());
            }
            Ok(Outcome::Parked) => {
                let mut s = sh.sched.lock().unwrap();
                s.release(tid, false);
                s.conns[idx].status = ConnStatus::Waiting;
                return Ok(());
            }
            Err(ChunkError::Fatal(e)) => {
                let was_stop = e.downcast_ref::<Stopped>().is_some();
                let msg = explain(&e);
                let mut s = sh.sched.lock().unwrap();
                s.release(tid, !was_stop);
                if !was_stop {
                    let c = &mut s.conns[idx];
                    c.status = ConnStatus::Failed;
                    c.error = Some(msg);
                }
                return Err(e);
            }
            Err(ChunkError::Retry {
                err,
                wait,
                pressure,
            }) => {
                let progressed = sh.sched.lock().unwrap().tasks[tid].pos > before;
                if progressed {
                    attempt = 0;
                }
                attempt += 1;
                sh.progress.retries.fetch_add(1, Ordering::Relaxed);
                if pressure {
                    sh.pressure.fetch_add(1, Ordering::Relaxed);
                }
                let msg = explain(&err);
                if attempt > sh.retries {
                    let mut s = sh.sched.lock().unwrap();
                    s.release(tid, true);
                    let c = &mut s.conns[idx];
                    c.status = ConnStatus::Failed;
                    c.error = Some(msg);
                    drop(s);
                    return Err(err.context(format!("giving up after {} retries", sh.retries)));
                }
                let delay = wait.unwrap_or_else(|| backoff(attempt));
                {
                    let mut s = sh.sched.lock().unwrap();
                    s.tasks[tid].attempts += 1;
                    let c = &mut s.conns[idx];
                    c.status = ConnStatus::Retrying;
                    c.retries = attempt;
                    c.error = Some(msg);
                    c.touch();
                }
                let mut left = delay;
                while !left.is_zero() {
                    if sh.should_stop() {
                        sh.sched.lock().unwrap().release(tid, false);
                        return Err(stopped());
                    }
                    let slice = left.min(Duration::from_millis(200));
                    set_conn(sh, idx, |c| c.retry_in_ms = left.as_millis() as u64);
                    tokio::time::sleep(slice).await;
                    left -= slice;
                }
                set_conn(sh, idx, |c| c.retry_in_ms = 0);
            }
        }
    }
}

/// One HTTP request for the rest of range `tid`. Data is written only after the
/// response header has been validated completely.
async fn fetch_into(sh: &Shared, idx: usize, tid: usize) -> Result<Outcome, ChunkError> {
    let (task_start, start_pos, end) = {
        let s = sh.sched.lock().unwrap();
        let t = &s.tasks[tid];
        (t.start, t.pos, t.end)
    };
    if start_pos > end {
        return Ok(Outcome::Done);
    }

    set_conn(sh, idx, |c| {
        c.status = ConnStatus::Connecting;
        c.last_start = task_start;
        c.last_end = end;
        c.touch();
    });

    let mut req = sh
        .client
        .get(&sh.url)
        .headers(sh.headers.clone())
        .header(header::RANGE, format!("bytes={}-{}", start_pos, end));
    if let Some(v) = &sh.if_range {
        req = req.header(header::IF_RANGE, v.as_str());
    }
    let resp = req
        .send()
        .await
        .map_err(|e| ChunkError::retry(anyhow::Error::new(e)))?;

    let status = resp.status();
    sh.progress
        .http_status
        .store(status.as_u16() as u32, Ordering::Relaxed);
    set_conn(sh, idx, |c| c.http_status = status.as_u16());
    if status != StatusCode::PARTIAL_CONTENT {
        return Err(classify_status(status, resp.headers()));
    }

    // ---- strict validation of the answer ----
    let cr = resp
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let Some(cr) = cr else {
        return Err(bad_range("the server did not send a Content-Range header"));
    };
    let Some((cr_start, cr_end, cr_total)) = parse_content_range(&cr) else {
        return Err(bad_range(&format!("unreadable Content-Range: {cr}")));
    };
    if let Some(t) = cr_total {
        if t != sh.total {
            return Err(ChunkError::Fatal(anyhow::Error::new(FileChanged(format!(
                "its size changed from {} to {} bytes",
                sh.total, t
            )))));
        }
    }
    if cr_start != start_pos {
        return Err(bad_range(&format!(
            "expected the range to start at {start_pos} but the server sent {cr_start}"
        )));
    }
    if cr_end < cr_start || cr_end >= sh.total || cr_end > end {
        return Err(bad_range(&format!(
            "the returned range {cr_start}-{cr_end} does not fit the request {start_pos}-{end}"
        )));
    }
    if let Some(len) = resp.content_length() {
        if len != cr_end - cr_start + 1 {
            return Err(bad_range(
                "Content-Length does not match the returned range",
            ));
        }
    }

    let mut file = OpenOptions::new()
        .write(true)
        .open(&sh.part_path)
        .await
        .map_err(fatal_io)?;
    file.seek(SeekFrom::Start(start_pos))
        .await
        .map_err(fatal_io)?;
    let mut w = BufWriter::with_capacity(256 * 1024, file);

    set_conn(sh, idx, |c| {
        c.status = ConnStatus::Receiving;
        c.error = None;
        c.win_t = Instant::now();
        c.win_bytes = 0;
    });
    sh.progress
        .advance_phase(PHASE_CONNECTING, PHASE_DOWNLOADING);

    let mut stream = resp.bytes_stream();
    let mut net_err: Option<anyhow::Error> = None;
    let mut was_stopped = false;
    let mut parked = false;
    let mut cur_pos = start_pos;
    let mut since_commit = 0u64;

    loop {
        if sh.should_stop() {
            was_stopped = true;
            break;
        }
        if idx >= sh.target.load(Ordering::Relaxed) {
            parked = true;
            break;
        }
        // Wake up every 500 ms even without data so stop requests are noticed quickly.
        let next = match timeout(Duration::from_millis(500), stream.next()).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        match next {
            Some(Ok(bytes)) => {
                sh.item_limiter.acquire(bytes.len()).await;
                sh.limiter.acquire(bytes.len()).await;
                let reserved = {
                    let mut s = sh.sched.lock().unwrap();
                    s.reserve(tid, idx, bytes.len() as u64)
                };
                // None: another connection took over the rest of this range.
                let Some((_, len, finished)) = reserved else {
                    break;
                };
                w.write_all(&bytes[..len as usize])
                    .await
                    .map_err(fatal_io)?;
                cur_pos += len;
                since_commit += len;
                sh.progress.add(len);
                if since_commit >= COMMIT_EVERY {
                    w.flush().await.map_err(fatal_io)?;
                    sh.sched.lock().unwrap().commit(tid, cur_pos);
                    since_commit = 0;
                }
                if finished || len < bytes.len() as u64 {
                    break;
                }
            }
            Some(Err(e)) => {
                net_err = Some(anyhow::Error::new(e));
                break;
            }
            None => break,
        }
    }

    // Always flush what we received so far, even if the connection failed.
    w.flush().await.map_err(fatal_io)?;
    sh.sched.lock().unwrap().commit(tid, cur_pos);

    if was_stopped {
        return Err(ChunkError::Fatal(stopped()));
    }
    let finished = {
        let s = sh.sched.lock().unwrap();
        let t = &s.tasks[tid];
        t.pos > t.end
    };
    if finished {
        return Ok(Outcome::Done);
    }
    if parked {
        return Ok(Outcome::Parked);
    }
    Err(ChunkError::retry(net_err.unwrap_or_else(|| {
        anyhow!("the connection closed before the range was complete")
    })))
}

/// Single connection fallback (no Range support or unknown size).
#[allow(clippy::too_many_arguments)]
async fn single(
    client: &Client,
    url: &str,
    headers: &header::HeaderMap,
    total: Option<u64>,
    part: &Path,
    out: &Path,
    limiter: &Limiter,
    item_limiter: &Limiter,
    progress: &Arc<Progress>,
    stop: &AtomicBool,
) -> Result<()> {
    let resp = client
        .get(url)
        .headers(headers.clone())
        .send()
        .await
        .context("could not connect to the server")?;
    let status = resp.status();
    progress
        .http_status
        .store(status.as_u16() as u32, Ordering::Relaxed);
    if !status.is_success() {
        return Err(anyhow::Error::new(HttpStatus(status.as_u16())));
    }

    let total = total.or(resp.content_length());
    progress.init(0, total.unwrap_or(0));

    let size = total.unwrap_or(0);
    let sched = Arc::new(Mutex::new(Sched::new(size, &[], 1)));
    {
        let mut s = sched.lock().unwrap();
        if !s.tasks.is_empty() {
            s.tasks[0].status = TaskStatus::Active;
            s.tasks[0].conn = Some(0);
            s.conns[0].task = Some(0);
            s.conns[0].last_end = size - 1;
        }
        s.conns[0].status = ConnStatus::Receiving;
        s.conns[0].http_status = status.as_u16();
        s.conns[0].touch();
    }
    let _ = progress.sched.set(sched.clone());
    progress.max_conns.store(1, Ordering::Relaxed);
    progress.target_conns.store(1, Ordering::Relaxed);
    progress.set_phase(PHASE_DOWNLOADING);

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
                item_limiter.acquire(bytes.len()).await;
                limiter.acquire(bytes.len()).await;
                w.write_all(&bytes).await?;
                progress.add(bytes.len() as u64);
                let mut s = sched.lock().unwrap();
                s.conns[0].note_bytes(bytes.len() as u64);
                if let Some(t) = s.tasks.first_mut() {
                    t.pos = (t.pos + bytes.len() as u64).min(t.end + 1);
                    t.committed = t.pos;
                }
            }
            None => break,
        }
    }
    progress.set_phase(PHASE_FINALIZING);
    w.flush().await?;
    {
        let mut s = sched.lock().unwrap();
        s.conns[0].status = ConnStatus::Completed;
        if !s.tasks.is_empty() {
            s.tasks[0].status = TaskStatus::Completed;
            s.tasks[0].pos = s.tasks[0].end + 1;
        }
    }
    crate::fsutil::safe_rename(part, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_range() {
        assert_eq!(
            parse_content_range("bytes 0-99/1000"),
            Some((0, 99, Some(1000)))
        );
        assert_eq!(parse_content_range("bytes 5-9/*"), Some((5, 9, None)));
        assert!(parse_content_range("items 0-1/2").is_none());
        assert!(parse_content_range("bytes a-b/2").is_none());
    }

    #[test]
    fn merges_overlapping_ranges() {
        let v = merge_ranges(vec![(10, 19), (0, 9), (30, 39), (35, 50)], 45);
        assert_eq!(v, vec![(0, 19), (30, 44)]);
    }

    #[test]
    fn fresh_scheduler_has_one_pending_range() {
        let s = Sched::new(1000, &[], 4);
        assert_eq!(s.tasks.len(), 1);
        assert!(!s.all_done());
    }

    #[test]
    fn resume_builds_ranges_around_finished_parts() {
        let s = Sched::new(100, &[(10, 19), (50, 59)], 2);
        let spans: Vec<(u64, u64, bool)> = s
            .tasks
            .iter()
            .map(|t| (t.start, t.end, t.status == TaskStatus::Completed))
            .collect();
        assert_eq!(
            spans,
            vec![
                (0, 9, false),
                (10, 19, true),
                (20, 49, false),
                (50, 59, true),
                (60, 99, false)
            ]
        );
    }

    #[test]
    fn idle_worker_splits_the_biggest_active_range() {
        let mut s = Sched::new(10 * MIB, &[], 2);
        let a = s.acquire(0).unwrap();
        let b = s.acquire(1).unwrap();
        assert_ne!(a, b);
        assert_eq!(s.tasks.len(), 2);
        assert_eq!(s.tasks[a].start, 0);
        assert_eq!(s.tasks[a].end + 1, s.tasks[b].start);
        assert_eq!(s.tasks[b].end, 10 * MIB - 1);
    }

    #[test]
    fn small_ranges_are_not_split() {
        let mut s = Sched::new(MIN_SPLIT, &[], 2);
        assert!(s.acquire(0).is_some());
        assert!(s.acquire(1).is_none());
    }

    #[test]
    fn reserve_stops_at_the_end_of_the_range() {
        let mut s = Sched::new(100, &[], 1);
        let t = s.acquire(0).unwrap();
        assert_eq!(s.reserve(t, 0, 60), Some((0, 60, false)));
        assert_eq!(s.reserve(t, 0, 60), Some((60, 40, true)));
        assert_eq!(s.reserve(t, 0, 10), None);
    }

    #[test]
    fn release_goes_back_to_the_last_flushed_byte() {
        let mut s = Sched::new(100, &[], 1);
        let t = s.acquire(0).unwrap();
        s.reserve(t, 0, 50);
        s.commit(t, 30);
        s.release(t, true);
        assert_eq!(s.tasks[t].pos, 30);
        assert_eq!(s.tasks[t].status, TaskStatus::Pending);
        assert!(s.tasks[t].failed);
        assert_eq!(s.done_ranges(), vec![(0, 29)]);
    }

    #[test]
    fn map_reflects_real_progress() {
        let mut s = Sched::new(100, &[], 1);
        let t = s.acquire(0).unwrap();
        s.reserve(t, 0, 50);
        let m = s.map(2);
        assert_eq!(m, vec![100, 0]);
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        assert!(backoff(1) >= Duration::from_millis(500));
        assert!(backoff(3) >= Duration::from_millis(2000));
        assert!(backoff(50) <= Duration::from_millis(40_000));
    }

    #[test]
    fn old_state_files_are_converted() {
        let dir = std::env::temp_dir().join(format!("rdm-test-{}", now_unix()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.part.json");
        std::fs::write(
            &p,
            br#"{"total":1000,"chunk_size":400,"validator":"x","done":[true,false,true]}"#,
        )
        .unwrap();
        let st = load_state(&p).unwrap();
        assert_eq!(st.ranges, vec![(0, 399), (800, 999)]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn errors_are_explained_in_plain_words() {
        let e = anyhow::Error::new(HttpStatus(503));
        assert!(explain(&e).contains("temporarily unavailable"));
        let e = anyhow::Error::new(BadRange("x".to_string()));
        assert!(explain(&e).contains("Invalid Range"));
    }
}
