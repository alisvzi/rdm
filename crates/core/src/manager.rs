//! Download manager: keeps the list of downloads, runs a limited number of them at
//! the same time (queue), supports pause/resume/restart/remove, and saves everything to
//! disk so the list survives a restart.
//!
//! Methods that can start a download (`add`, `resume`, `resume_all`, `restart`,
//! `set_settings`) must be called from inside a Tokio runtime, because they spawn tasks.

use crate::engine::{self, EngineSnapshot, Job, Progress, Stopped};
use crate::hls;
use crate::integrity::{self, Algo};
use crate::limiter::Limiter;
use crate::limitrules;
use crate::probe;
use crate::queues::{self, Queue, Slot, MAIN_QUEUE};
use crate::secure;
use crate::state;
use anyhow::{anyhow, bail, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Downloading,
    Paused,
    Completed,
    Error,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub download_dir: String,
    /// Parallel connections per download. 0 = automatic (adapts to the server).
    pub connections: usize,
    /// How many downloads run at the same time; the rest wait in the queue.
    pub max_concurrent: usize,
    /// Global speed limit in KiB/s. 0 = unlimited.
    pub speed_limit_kbps: u64,
    pub retries: u32,
    /// Put files into sub-folders (Video, Music, ...) inside the download folder.
    pub categorize: bool,
    /// Closing the window hides the app in the system tray instead of quitting.
    pub close_to_tray: bool,
    /// Scheduler: start/stop the queue at fixed times of day ("HH:MM", empty = off).
    pub schedule_enabled: bool,
    pub schedule_start: String,
    pub schedule_stop: String,
    /// What to do when a scheduled run finishes: "none" | "shutdown" | "hibernate".
    pub after_finish: String,
    /// Secret shared with the browser extension. Generated automatically.
    pub api_token: String,
    /// Folder chosen last time in the confirmation dialog ("" = use the category folders).
    pub last_dir: String,
    /// Show an IDM-style confirmation window before every browser download.
    pub confirm_downloads: bool,
    /// Optional virus scanner run on every finished file ("" = off).
    pub scanner_path: String,
    /// Scanner arguments split on spaces; `{file}` is replaced by the file path
    /// (the path is never split, so spaces in it are safe). Added at the end if missing.
    pub scanner_args: String,
    /// Exit codes (comma separated) that mean "threat found"; 0 = clean; others = scanner failed.
    pub scanner_threat_codes: String,
    /// "system" (default) | "none" | "manual"
    pub proxy_mode: String,
    /// http://, https://, socks5:// or socks5h:// address (manual mode).
    pub proxy_url: String,
    pub proxy_user: String,
    /// Kept in memory; saved separately, encrypted. Never sent to the UI.
    #[serde(skip_serializing)]
    pub proxy_password: String,
    pub proxy_bypass: String,
    /// "http1" (default; best for many parallel connections) | "auto" (allow HTTP/2).
    pub http_mode: String,
    /// Speed schedule, one rule per line: `22:00-06:00 5000` (KiB/s, 0 = unlimited).
    pub limit_rules: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: String::new(),
            connections: 0,
            max_concurrent: 3,
            speed_limit_kbps: 0,
            retries: 8,
            categorize: true,
            close_to_tray: true,
            schedule_enabled: false,
            schedule_start: String::new(),
            schedule_stop: String::new(),
            after_finish: "none".to_string(),
            api_token: String::new(),
            last_dir: String::new(),
            confirm_downloads: true,
            scanner_path: String::new(),
            scanner_args: "{file}".to_string(),
            scanner_threat_codes: "1,2".to_string(),
            proxy_mode: "system".to_string(),
            proxy_url: String::new(),
            proxy_user: String::new(),
            proxy_password: String::new(),
            proxy_bypass: String::new(),
            http_mode: "http1".to_string(),
            limit_rules: String::new(),
        }
    }
}

impl Settings {
    fn sanitize(&mut self, fallback_dir: &Path) {
        self.connections = self.connections.min(64);
        self.max_concurrent = self.max_concurrent.clamp(1, 16);
        self.retries = self.retries.min(50);
        if self.download_dir.trim().is_empty() {
            self.download_dir = fallback_dir.to_string_lossy().into_owned();
        }
        if !valid_hm(&self.schedule_start) {
            self.schedule_start.clear();
        }
        if !valid_hm(&self.schedule_stop) {
            self.schedule_stop.clear();
        }
        if !matches!(self.after_finish.as_str(), "shutdown" | "hibernate") {
            self.after_finish = "none".to_string();
        }
        if !matches!(self.proxy_mode.as_str(), "none" | "manual") {
            self.proxy_mode = "system".to_string();
        }
        if self.http_mode != "auto" {
            self.http_mode = "http1".to_string();
        }
        self.proxy_url = self.proxy_url.trim().to_string();
        self.proxy_user = self.proxy_user.trim().to_string();
        self.proxy_bypass = self.proxy_bypass.trim().to_string();
        self.scanner_path = self.scanner_path.trim().trim_matches('"').to_string();
        self.scanner_args = self.scanner_args.trim().to_string();
        if self.scanner_args.is_empty() {
            self.scanner_args = "{file}".to_string();
        }
        self.scanner_threat_codes = self
            .scanner_threat_codes
            .split(',')
            .filter_map(|c| c.trim().parse::<i32>().ok())
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(",");
        if self.api_token.len() < 16 {
            self.api_token = new_token();
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: u64,
    pub url: String,
    pub filename: String,
    pub save_path: String,
    pub category: String,
    pub total: Option<u64>,
    pub downloaded: u64,
    pub status: Status,
    pub error: Option<String>,
    pub resumable: bool,
    pub added: u64,
    /// Extra request headers (cookies, referer, user-agent) captured from the browser.
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub accept_ranges: Option<String>,
    #[serde(default)]
    pub http_status: u16,
    /// Retries over the whole life of this download.
    #[serde(default)]
    pub retries: u32,
    #[serde(default)]
    pub started_at: Option<u64>,
    #[serde(default)]
    pub completed_at: Option<u64>,
    /// Seconds this download was actually running (all sessions together).
    #[serde(default)]
    pub active_secs: u64,
    /// Speed limit of this download only, in KiB/s (0 = none).
    #[serde(default)]
    pub speed_limit_kbps: u64,
    /// What to do when this download completes: "" / "none" | "open_file" | "open_folder"
    /// | "shutdown" | "hibernate".
    #[serde(default)]
    pub on_complete: String,
    /// Request headers encrypted with DPAPI. Only exists in downloads.json, never in memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secure_headers: Option<String>,
    /// "" / "not_verified" | "verifying" | "verified" | "mismatch" | "computed" | "failed"
    #[serde(default)]
    pub integrity: String,
    /// Checksum the file should have, "sha256:<hex>" (from the server or typed by the user).
    #[serde(default)]
    pub expected_hash: Option<String>,
    /// "server" | "user" | "".
    #[serde(default)]
    pub integrity_source: String,
    /// Checksum calculated from the finished file.
    #[serde(default)]
    pub actual_hash: Option<String>,
    /// "" | "scanning" | "clean" | "threat" | "failed"
    #[serde(default)]
    pub scan: String,
    #[serde(default)]
    pub scan_note: Option<String>,
    /// Queue this download belongs to.
    #[serde(default = "main_queue")]
    pub queue_id: u32,
    /// -1 low, 0 normal, 1 high. Higher priority starts first inside its queue.
    #[serde(default)]
    pub priority: i32,
}

fn main_queue() -> u32 {
    MAIN_QUEUE
}

/// Applies a status change only when the state machine allows it.
fn set_status(it: &mut Item, to: Status) -> bool {
    if state::transition_allowed(it.status, to) {
        it.status = to;
        true
    } else {
        false
    }
}

/// Connection limit remembered for one host after it throttled us.
#[derive(Clone, Serialize, Deserialize)]
struct HostCap {
    cap: usize,
    at: u64,
}

const HOST_CAP_TTL_SECS: u64 = 24 * 3600;

/// What the UI receives: the stored item plus live values.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    #[serde(flatten)]
    pub item: Item,
    /// Bytes per second (smoothed).
    pub speed: u64,
    /// Seconds left, when it can be estimated.
    pub eta: Option<u64>,
    /// A pause/remove was requested and the download is shutting down.
    pub stopping: bool,
    /// Completion (0..=100) of up to 64 slices of the file, for the "download map".
    pub segments: Vec<u8>,
    /// What the engine is doing right now ("" when not running).
    pub phase: String,
    pub active_connections: u32,
    pub max_connections: u32,
}

/// Everything the details window shows, read from the live engine state.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadDetails {
    #[serde(flatten)]
    pub view: ItemView,
    /// Connections, ranges, retries... (only while the download is running).
    pub engine: Option<EngineSnapshot>,
    /// Average speed of the current run, bytes per second.
    pub avg_speed: u64,
    /// Time this download was running (earlier sessions + the current one).
    pub elapsed_secs: u64,
    /// Why a resume was not possible, if that happened.
    pub resume_note: String,
    /// "not_verified" until a checksum check exists for this download.
    pub integrity: String,
    pub queue: String,
}

/// Result of checking a link before the user confirms the download.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub url: String,
    pub filename: String,
    pub total: Option<u64>,
    pub resumable: bool,
    pub category: String,
    /// Where the file would be saved right now (last chosen folder or the category folder).
    pub folder: String,
    /// The automatic folder for this kind of file.
    pub category_folder: String,
}

#[derive(Clone, Copy)]
enum Reason {
    Pause,
    Remove { delete_file: bool },
}

struct Running {
    progress: Arc<Progress>,
    stop: Arc<AtomicBool>,
    reason: Reason,
    /// Speed limiter of this download.
    limiter: Arc<Limiter>,
    queue_id: u32,
    last_t: Instant,
    last_bytes: u64,
    speed: f64,
    started: Instant,
}

struct Live {
    downloaded: u64,
    speed: u64,
    stopping: bool,
    segments: Vec<u8>,
    phase: String,
    active: u32,
    max: u32,
}

struct Inner {
    client: Mutex<reqwest::Client>,
    queues: Mutex<Vec<Queue>>,
    queue_limiters: Mutex<HashMap<u32, Arc<Limiter>>>,
    host_caps: Mutex<HashMap<String, HostCap>>,
    probe_cache: Mutex<HashMap<String, (Instant, probe::Probe)>>,
    /// Local minute of the day reported by the app (for the speed schedule); u32::MAX = unknown.
    rate_minute: std::sync::atomic::AtomicU32,
    data_dir: PathBuf,
    items: Mutex<Vec<Item>>,
    running: Mutex<HashMap<u64, Running>>,
    settings: Mutex<Settings>,
    limiter: Arc<Limiter>,
    next_id: AtomicU64,
    persist_lock: Mutex<()>,
}

#[derive(Clone)]
pub struct Manager {
    inner: Arc<Inner>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn valid_hm(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    let mut it = s.split(':');
    match (it.next(), it.next(), it.next()) {
        (Some(h), Some(m), None) => {
            h.len() == 2
                && m.len() == 2
                && h.parse::<u32>().map_or(false, |h| h < 24)
                && m.parse::<u32>().map_or(false, |m| m < 60)
        }
        _ => false,
    }
}

fn valid_action(s: &str) -> bool {
    matches!(
        s,
        "" | "none" | "open_file" | "open_folder" | "shutdown" | "hibernate"
    )
}

fn new_token() -> String {
    let mut buf = [0u8; 16];
    if getrandom::getrandom(&mut buf).is_err() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        buf = n.to_le_bytes();
    }
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Builds a header map from stored pairs, silently skipping invalid ones.
pub fn headers_from(pairs: &[(String, String)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (k, v) in pairs {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            map.insert(name, value);
        }
    }
    map
}

fn proxy_of(s: &Settings) -> probe::ProxyConfig {
    probe::ProxyConfig {
        mode: s.proxy_mode.clone(),
        url: s.proxy_url.clone(),
        user: s.proxy_user.clone(),
        password: s.proxy_password.clone(),
        bypass: s.proxy_bypass.clone(),
        http_auto: s.http_mode == "auto",
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let data = std::fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

fn write_atomic(path: &Path, data: &[u8]) {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    if std::fs::write(&tmp, data).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

pub fn category_of(filename: &str) -> &'static str {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" => "Compressed",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "epub" | "odt"
        | "csv" | "rtf" => "Documents",
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma" | "opus" => "Music",
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "ts" | "mpg" | "mpeg" => {
            "Video"
        }
        "exe" | "msi" | "msix" | "apk" | "dmg" | "iso" | "deb" | "appimage" => "Programs",
        _ => "General",
    }
}

/// Smooths the speed of one running download and copies its live state.
fn update_live(r: &mut Running, now: Instant) -> Live {
    let dt = now.duration_since(r.last_t).as_secs_f64();
    if dt >= 0.3 {
        let cur = r.progress.session();
        let inst = cur.saturating_sub(r.last_bytes) as f64 / dt;
        r.speed = if r.speed <= 0.0 {
            inst
        } else {
            r.speed * 0.6 + inst * 0.4
        };
        r.last_bytes = cur;
        r.last_t = now;
    }
    let snap = r.progress.snapshot(64);
    Live {
        downloaded: r.progress.downloaded(),
        speed: r.speed as u64,
        stopping: r.stop.load(Ordering::Relaxed),
        segments: snap.map,
        phase: snap.phase,
        active: snap.active_connections,
        max: snap.max_connections,
    }
}

fn make_view(it: &Item, live: Option<&Live>) -> ItemView {
    let mut item = it.clone();
    item.headers.clear(); // never send cookies to the UI
    item.secure_headers = None;
    let mut speed = 0u64;
    let mut stopping = false;
    let mut segments = Vec::new();
    let mut phase = String::new();
    let mut active = 0u32;
    let mut max = 0u32;
    if let Some(l) = live {
        item.downloaded = l.downloaded;
        speed = l.speed;
        stopping = l.stopping;
        segments = l.segments.clone();
        phase = l.phase.clone();
        active = l.active;
        max = l.max;
    }
    if let Some(t) = item.total {
        item.downloaded = item.downloaded.min(t);
    }
    let eta = match (item.status, item.total) {
        (Status::Downloading, Some(t)) if speed > 0 => {
            Some(t.saturating_sub(item.downloaded) / speed)
        }
        _ => None,
    };
    ItemView {
        item,
        speed,
        eta,
        stopping,
        segments,
        phase,
        active_connections: active,
        max_connections: max,
    }
}

impl Inner {
    fn persist_items(&self) {
        let _guard = self.persist_lock.lock().unwrap();
        // Cookies and other headers are encrypted (DPAPI) before they touch the disk.
        // If encryption is not possible they are simply not saved.
        let list: Vec<Item> = {
            let items = self.items.lock().unwrap();
            items
                .iter()
                .map(|it| {
                    let mut c = it.clone();
                    c.secure_headers = if c.headers.is_empty() {
                        None
                    } else {
                        serde_json::to_string(&c.headers)
                            .ok()
                            .and_then(|j| secure::protect(&j))
                    };
                    c.headers.clear();
                    c
                })
                .collect()
        };
        let data = serde_json::to_vec_pretty(&list);
        if let Ok(data) = data {
            write_atomic(&self.data_dir.join("downloads.json"), &data);
        }
    }

    fn persist_settings(&self) {
        let _guard = self.persist_lock.lock().unwrap();
        let data = {
            let s = self.settings.lock().unwrap();
            serde_json::to_vec_pretty(&*s)
        };
        if let Ok(data) = data {
            write_atomic(&self.data_dir.join("settings.json"), &data);
        }
    }

    fn update_item(&self, id: u64, f: impl FnOnce(&mut Item)) {
        let mut items = self.items.lock().unwrap();
        if let Some(it) = items.iter_mut().find(|i| i.id == id) {
            f(it);
        }
    }

    /// Starts queued downloads until `max_concurrent` are running.
    /// Probes a link, reusing an answer from the last minute. Adding a download used to cost
    /// three identical requests (check, add, start); now it costs one.
    async fn probe_cached(&self, url: &str, pairs: &[(String, String)]) -> Result<probe::Probe> {
        let ttl = std::time::Duration::from_secs(60);
        let key = format!(
            "{url}\n{}",
            pairs
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        if let Some((t, p)) = self.probe_cache.lock().unwrap().get(&key) {
            if t.elapsed() < ttl {
                return Ok(p.clone());
            }
        }
        let p = probe::probe(&self.client(), url, &headers_from(pairs)).await?;
        let mut cache = self.probe_cache.lock().unwrap();
        if cache.len() > 64 {
            cache.retain(|_, (t, _)| t.elapsed() < ttl);
        }
        cache.insert(key, (Instant::now(), p.clone()));
        Ok(p)
    }

    /// Sets the global speed limit: the speed schedule if a rule matches now, else the
    /// normal limit from the settings. Returns the limit in KiB/s.
    fn apply_rate(&self) -> u64 {
        let s = self.settings.lock().unwrap();
        let minute = self.rate_minute.load(Ordering::Relaxed);
        let rules = limitrules::parse(&s.limit_rules).unwrap_or_default();
        let scheduled = if minute < 1440 {
            limitrules::effective(&rules, minute)
        } else {
            None
        };
        let kbps = scheduled.unwrap_or(s.speed_limit_kbps);
        self.limiter.set_rate(kbps.saturating_mul(1024));
        kbps
    }

    fn host_of(url: &str) -> Option<String> {
        reqwest::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
    }

    /// Connection limit remembered for this URL's host (0 = none or expired).
    fn host_cap_for(&self, url: &str) -> usize {
        let Some(host) = Self::host_of(url) else {
            return 0;
        };
        let now = now_secs();
        self.host_caps
            .lock()
            .unwrap()
            .get(&host)
            .filter(|c| now.saturating_sub(c.at) < HOST_CAP_TTL_SECS)
            .map_or(0, |c| c.cap)
    }

    /// Remembers that a host throttled us down to `cap` connections.
    fn remember_throttle(&self, url: &str, cap: usize) {
        if cap == 0 {
            return;
        }
        let Some(host) = Self::host_of(url) else {
            return;
        };
        {
            let mut caps = self.host_caps.lock().unwrap();
            let now = now_secs();
            let keep = caps
                .get(&host)
                .filter(|c| now.saturating_sub(c.at) < HOST_CAP_TTL_SECS)
                .map_or(cap, |c| c.cap.min(cap));
            caps.insert(host, HostCap { cap: keep, at: now });
        }
        self.persist_host_caps();
    }

    fn client(&self) -> reqwest::Client {
        self.client.lock().unwrap().clone()
    }

    fn persist_queues(&self) {
        let _guard = self.persist_lock.lock().unwrap();
        let data = serde_json::to_vec_pretty(&*self.queues.lock().unwrap());
        if let Ok(data) = data {
            write_atomic(&self.data_dir.join("queues.json"), &data);
        }
    }

    fn persist_host_caps(&self) {
        let _guard = self.persist_lock.lock().unwrap();
        let data = serde_json::to_vec_pretty(&*self.host_caps.lock().unwrap());
        if let Ok(data) = data {
            write_atomic(&self.data_dir.join("hosts.json"), &data);
        }
    }

    /// The speed limiter shared by every download of one queue.
    fn queue_limiter(&self, qid: u32) -> Arc<Limiter> {
        let kbps = self
            .queues
            .lock()
            .unwrap()
            .iter()
            .find(|q| q.id == qid)
            .map_or(0, |q| q.speed_limit_kbps);
        self.queue_limiters
            .lock()
            .unwrap()
            .entry(qid)
            .or_insert_with(|| Arc::new(Limiter::new(kbps.saturating_mul(1024))))
            .clone()
    }

    /// Starts waiting downloads: at most `max_concurrent` in total, and each queue within its
    /// own limit. Queues with a higher priority go first; disabled queues start nothing.
    fn schedule(self: &Arc<Self>) {
        loop {
            let global_max = self.settings.lock().unwrap().max_concurrent.max(1);
            let running: Vec<u32> = self
                .running
                .lock()
                .unwrap()
                .values()
                .map(|r| r.queue_id)
                .collect();
            if running.len() >= global_max {
                break;
            }
            let slots: Vec<Slot> = self
                .queues
                .lock()
                .unwrap()
                .iter()
                .map(|q| Slot {
                    id: q.id,
                    priority: q.priority,
                    max: q.max_concurrent,
                    enabled: q.enabled,
                })
                .collect();
            let mut counts: Vec<(u32, usize)> = Vec::new();
            for q in &running {
                match counts.iter_mut().find(|(id, _)| id == q) {
                    Some(c) => c.1 += 1,
                    None => counts.push((*q, 1)),
                }
            }
            let waiting: Vec<u32> = self
                .items
                .lock()
                .unwrap()
                .iter()
                .filter(|i| i.status == Status::Queued)
                .map(|i| i.queue_id)
                .collect();
            let Some(qid) = queues::pick_queue(&slots, &counts, &waiting) else {
                break;
            };
            let (id, kbps) = {
                let mut items = self.items.lock().unwrap();
                // Highest priority first; equal priority keeps the list order.
                let mut best: Option<usize> = None;
                for (n, i) in items.iter().enumerate() {
                    if i.status == Status::Queued
                        && i.queue_id == qid
                        && best.map_or(true, |b| i.priority > items[b].priority)
                    {
                        best = Some(n);
                    }
                }
                match best.map(|n| &mut items[n]) {
                    Some(it) => {
                        set_status(it, Status::Downloading);
                        it.error = None;
                        (it.id, it.speed_limit_kbps)
                    }
                    None => break,
                }
            };

            let progress = Arc::new(Progress::default());
            let stop = Arc::new(AtomicBool::new(false));
            let item_limiter = Arc::new(Limiter::with_parent(
                kbps.saturating_mul(1024),
                self.queue_limiter(qid),
            ));
            self.running.lock().unwrap().insert(
                id,
                Running {
                    progress: progress.clone(),
                    stop: stop.clone(),
                    reason: Reason::Pause,
                    limiter: item_limiter.clone(),
                    queue_id: qid,
                    last_t: Instant::now(),
                    last_bytes: 0,
                    speed: 0.0,
                    started: Instant::now(),
                },
            );
            let inner = self.clone();
            tokio::spawn(
                async move { run_download(inner, id, progress, stop, item_limiter).await },
            );
        }
        self.persist_items();
    }
}

async fn run_download(
    inner: Arc<Inner>,
    id: u64,
    progress: Arc<Progress>,
    stop: Arc<AtomicBool>,
    item_limiter: Arc<Limiter>,
) {
    let found = {
        let items = inner.items.lock().unwrap();
        items.iter().find(|i| i.id == id).map(|i| {
            (
                i.url.clone(),
                PathBuf::from(&i.save_path),
                i.headers.clone(),
            )
        })
    };
    let Some((url, out, item_headers)) = found else {
        inner.running.lock().unwrap().remove(&id);
        inner.schedule();
        return;
    };
    let settings = inner.settings.lock().unwrap().clone();
    let started = Instant::now();
    inner.update_item(id, |it| {
        if it.started_at.is_none() {
            it.started_at = Some(now_secs());
        }
    });

    let result: Result<()> = async {
        let headers = headers_from(&item_headers);
        // Probing again on every start refreshes redirects and the resume validator.
        let info = inner.probe_cached(&url, &item_headers).await?;
        if hls::is_dash(info.url.as_str(), info.content_type.as_deref()) {
            bail!("DASH (.mpd) streams are not supported yet; only plain HLS (.m3u8) is");
        }
        if hls::is_hls(info.url.as_str(), info.content_type.as_deref()) {
            inner.update_item(id, |it| {
                it.total = None;
                it.resumable = false;
                it.http_status = info.http_status;
                it.content_type = info.content_type.clone();
            });
            return hls::run(hls::HlsJob {
                client: inner.client(),
                url: info.url.to_string(),
                out: out.clone(),
                headers,
                retries: settings.retries,
                limiter: inner.limiter.clone(),
                item_limiter: item_limiter.clone(),
                progress: progress.clone(),
                stop: stop.clone(),
            })
            .await;
        }
        inner.update_item(id, |it| {
            it.total = info.total;
            it.resumable = info.ranges;
            it.http_status = info.http_status;
            it.content_type = info.content_type.clone();
            it.etag = info.etag.clone();
            it.last_modified = info.last_modified.clone();
            it.accept_ranges = info.accept_ranges.clone();
            it.server = info.server.clone();
            if it.expected_hash.is_none() || it.integrity_source == "server" {
                it.expected_hash = info.digest.clone();
                it.integrity_source = if info.digest.is_some() { "server" } else { "" }.to_string();
            }
        });
        engine::run(Job {
            client: inner.client(),
            probe: info,
            out: out.clone(),
            connections: settings.connections,
            retries: settings.retries,
            headers,
            limiter: inner.limiter.clone(),
            item_limiter: item_limiter.clone(),
            use_if_range: true,
            host_cap: inner.host_cap_for(&url),
            progress: progress.clone(),
            stop: stop.clone(),
        })
        .await
    }
    .await;

    let reason = inner.running.lock().unwrap().remove(&id).map(|r| r.reason);
    let downloaded = progress.downloaded();
    let elapsed = started.elapsed().as_secs();
    let retries = progress.retries();

    if let Some(Reason::Remove { delete_file }) = reason {
        engine::remove_partial(&out);
        if delete_file && result.is_ok() {
            let _ = std::fs::remove_file(&out);
        }
        inner.schedule();
        return;
    }

    inner.update_item(id, |it| {
        it.active_secs += elapsed;
        it.retries += retries;
    });

    inner.remember_throttle(&url, progress.throttled_cap());
    let result_ok = result.is_ok();
    let scanning = !settings.scanner_path.is_empty();
    match result {
        Ok(()) => inner.update_item(id, |it| {
            set_status(it, Status::Completed);
            it.error = None;
            it.total = it.total.or(Some(downloaded));
            it.downloaded = it.total.unwrap_or(downloaded);
            it.completed_at = Some(now_secs());
            it.actual_hash = None;
            it.scan = if scanning {
                "scanning".to_string()
            } else {
                String::new()
            };
            it.scan_note = None;
            it.integrity = if it.expected_hash.is_some() {
                "verifying".to_string()
            } else {
                "not_verified".to_string()
            };
        }),
        Err(e) if e.downcast_ref::<Stopped>().is_some() => inner.update_item(id, |it| {
            set_status(it, Status::Paused);
            it.downloaded = downloaded;
        }),
        Err(e) => {
            let msg = engine::explain(&e);
            inner.update_item(id, |it| {
                set_status(it, Status::Error);
                it.error = Some(msg);
                it.downloaded = downloaded;
            });
        }
    }
    if result_ok {
        let inner2 = inner.clone();
        tokio::spawn(async move { post_process(inner2, id).await });
    }
    inner.schedule();
}

fn threat_codes(s: &str) -> Vec<i32> {
    s.split(',').filter_map(|c| c.trim().parse().ok()).collect()
}

/// Runs after a download finished: checksum check (if a checksum is known), then the scanner.
async fn post_process(inner: Arc<Inner>, id: u64) {
    let has_expected = {
        let items = inner.items.lock().unwrap();
        items
            .iter()
            .find(|i| i.id == id)
            .map_or(false, |i| i.expected_hash.is_some())
    };
    if has_expected {
        verify_item(&inner, id).await;
    }
    scan_item(&inner, id).await;
}

/// Calculates the checksum of the finished file and compares it with the expected one.
async fn verify_item(inner: &Arc<Inner>, id: u64) {
    let found = {
        let items = inner.items.lock().unwrap();
        items
            .iter()
            .find(|i| i.id == id)
            .map(|i| (PathBuf::from(&i.save_path), i.expected_hash.clone()))
    };
    let Some((path, expected)) = found else {
        return;
    };
    let expected = expected.and_then(|e| integrity::split_stored(&e));
    let algo = expected.as_ref().map_or(Algo::Sha256, |(a, _)| *a);
    inner.update_item(id, |it| it.integrity = "verifying".to_string());
    inner.persist_items();

    let res = tokio::task::spawn_blocking(move || {
        integrity::hash_file(&path, algo, &AtomicBool::new(false), &AtomicU64::new(0))
    })
    .await;
    let outcome = match res {
        Ok(Ok(hex)) => Ok(hex),
        Ok(Err(e)) => Err(e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    inner.update_item(id, |it| match &outcome {
        Ok(hex) => {
            it.actual_hash = Some(integrity::format_hash(algo, hex));
            it.integrity = match &expected {
                Some((_, want)) if want.eq_ignore_ascii_case(hex) => "verified",
                Some(_) => "mismatch",
                None => "computed",
            }
            .to_string();
        }
        Err(_) => it.integrity = "failed".to_string(),
    });
    inner.persist_items();
}

/// Runs the configured scanner (if any) on the finished file.
async fn scan_item(inner: &Arc<Inner>, id: u64) {
    let settings = inner.settings.lock().unwrap().clone();
    if settings.scanner_path.is_empty() {
        return;
    }
    let path = {
        let items = inner.items.lock().unwrap();
        match items.iter().find(|i| i.id == id) {
            Some(i) => i.save_path.clone(),
            None => return,
        }
    };
    inner.update_item(id, |it| {
        it.scan = "scanning".to_string();
        it.scan_note = None;
    });
    inner.persist_items();

    let mut args: Vec<String> = settings
        .scanner_args
        .split_whitespace()
        .map(|a| a.replace("{file}", &path))
        .collect();
    if !settings.scanner_args.contains("{file}") {
        args.push(path.clone());
    }
    let mut cmd = tokio::process::Command::new(&settings.scanner_path);
    cmd.args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // no console window

    let run = async {
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("cannot start the scanner: {e}"))?;
        child.wait().await.map_err(|e| e.to_string())
    };
    let outcome = match tokio::time::timeout(std::time::Duration::from_secs(600), run).await {
        Ok(Ok(status)) => Ok(status.code()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("the scanner did not finish within 10 minutes".to_string()),
    };
    let threats = threat_codes(&settings.scanner_threat_codes);
    inner.update_item(id, |it| match &outcome {
        Ok(Some(0)) => it.scan = "clean".to_string(),
        Ok(Some(c)) if threats.contains(c) => {
            it.scan = "threat".to_string();
            it.scan_note = Some(format!("scanner exit code {c}"));
        }
        Ok(code) => {
            it.scan = "failed".to_string();
            it.scan_note = Some(format!("scanner exit code {code:?}"));
        }
        Err(e) => {
            it.scan = "failed".to_string();
            it.scan_note = Some(e.clone());
        }
    });
    inner.persist_items();
}

impl Manager {
    pub fn new(data_dir: PathBuf, default_download_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let mut settings: Settings = read_json(&data_dir.join("settings.json")).unwrap_or_default();
        settings.sanitize(&default_download_dir);
        // The proxy password lives in its own encrypted file, not in settings.json.
        if let Ok(blob) = std::fs::read_to_string(data_dir.join("proxy.secret")) {
            if let Some(p) = secure::unprotect(blob.trim()) {
                settings.proxy_password = p;
            }
        }
        // A broken proxy setting must never stop the app from starting.
        let client = probe::build_client_with(probe::DEFAULT_UA, &proxy_of(&settings))
            .or_else(|_| probe::build_client(probe::DEFAULT_UA))?;

        // Queues: the first start after an update turns the old global scheduler into "Main".
        let mut queue_list: Vec<Queue> =
            read_json(&data_dir.join("queues.json")).unwrap_or_default();
        if queue_list.is_empty() {
            queue_list.push(Queue {
                max_concurrent: settings.max_concurrent,
                schedule_enabled: settings.schedule_enabled,
                schedule_start: settings.schedule_start.clone(),
                schedule_stop: settings.schedule_stop.clone(),
                after_finish: settings.after_finish.clone(),
                only_if_all_succeeded: true,
                ..Queue::default()
            });
        }
        for q in queue_list.iter_mut() {
            q.sanitize();
        }
        if !queue_list.iter().any(|q| q.id == MAIN_QUEUE) {
            queue_list.insert(0, Queue::default());
        }
        let host_caps: HashMap<String, HostCap> =
            read_json(&data_dir.join("hosts.json")).unwrap_or_default();

        let mut items: Vec<Item> = read_json(&data_dir.join("downloads.json")).unwrap_or_default();
        for it in items.iter_mut() {
            if let Some(blob) = it.secure_headers.take() {
                if let Some(h) = secure::unprotect(&blob)
                    .and_then(|j| serde_json::from_str::<Vec<(String, String)>>(&j).ok())
                {
                    it.headers = h;
                }
            }
            if !queue_list.iter().any(|q| q.id == it.queue_id) {
                it.queue_id = MAIN_QUEUE;
            }
            if it.integrity == "verifying" {
                it.integrity = "not_verified".to_string();
            }
            if it.scan == "scanning" {
                it.scan.clear();
            }
            // The app was closed while these were active: let the user resume them.
            if matches!(it.status, Status::Downloading | Status::Queued) {
                set_status(it, Status::Paused);
            }
            if it.status != Status::Completed {
                it.downloaded = engine::saved_progress(Path::new(&it.save_path)).unwrap_or(0);
            }
        }
        let next_id = items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        let limiter = Arc::new(Limiter::new(settings.speed_limit_kbps.saturating_mul(1024)));

        let inner = Arc::new(Inner {
            client: Mutex::new(client),
            queues: Mutex::new(queue_list),
            queue_limiters: Mutex::new(HashMap::new()),
            host_caps: Mutex::new(host_caps),
            probe_cache: Mutex::new(HashMap::new()),
            rate_minute: std::sync::atomic::AtomicU32::new(u32::MAX),
            data_dir,
            items: Mutex::new(items),
            running: Mutex::new(HashMap::new()),
            settings: Mutex::new(settings),
            limiter,
            next_id: AtomicU64::new(next_id),
            persist_lock: Mutex::new(()),
        });
        inner.persist_settings();
        inner.persist_queues();
        inner.persist_items();
        Ok(Self { inner })
    }

    pub fn settings(&self) -> Settings {
        self.inner.settings.lock().unwrap().clone()
    }

    pub fn set_settings(&self, mut s: Settings) -> Result<()> {
        let (fallback, token, last_dir, old_proxy) = {
            let cur = self.inner.settings.lock().unwrap();
            (
                cur.download_dir.clone(),
                cur.api_token.clone(),
                cur.last_dir.clone(),
                proxy_of(&cur),
            )
        };
        // The UI never receives the proxy password, so an empty value means "unchanged".
        if s.proxy_password.is_empty() && s.proxy_user == old_proxy.user {
            s.proxy_password = old_proxy.password.clone();
        }
        s.api_token = token; // the UI can never change the pairing token
        s.last_dir = last_dir; // only changed through the download dialogs
        s.sanitize(Path::new(&fallback));
        limitrules::parse(&s.limit_rules).map_err(|e| anyhow!("speed schedule: {e}"))?;
        let new_proxy = proxy_of(&s);
        let proxy_changed = format!("{new_proxy:?}") != format!("{old_proxy:?}");
        if proxy_changed {
            // Build first: a bad proxy setting is rejected and nothing changes.
            let c = probe::build_client_with(probe::DEFAULT_UA, &new_proxy)?;
            *self.inner.client.lock().unwrap() = c;
            let secret = if new_proxy.password.is_empty() {
                Some(String::new())
            } else {
                secure::protect(&new_proxy.password)
            };
            match secret {
                Some(v) => write_atomic(&self.inner.data_dir.join("proxy.secret"), v.as_bytes()),
                None => bail!("the proxy password cannot be stored securely on this system"),
            }
        }
        std::fs::create_dir_all(&s.download_dir)
            .map_err(|e| anyhow!("cannot create folder {}: {e}", s.download_dir))?;
        *self.inner.settings.lock().unwrap() = s;
        self.inner.apply_rate();
        self.inner.persist_settings();
        // max_concurrent may have grown: start more queued downloads.
        self.inner.schedule();
        Ok(())
    }

    fn unique_path(&self, path: PathBuf) -> PathBuf {
        let items = self.inner.items.lock().unwrap();
        let taken = |p: &Path| {
            p.exists()
                || engine::part_path(p).exists()
                || items.iter().any(|i| Path::new(&i.save_path) == p)
        };
        if !taken(&path) {
            return path;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "download".to_string());
        let ext = path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let mut n = 1u32;
        loop {
            let candidate = dir.join(format!("{stem} ({n}){ext}"));
            if !taken(&candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Checks the link, decides the file name and folder, and adds it to the list.
    /// With `start = false` the item is added as paused.
    pub async fn add(&self, url: &str, dir: Option<&str>, start: bool) -> Result<u64> {
        self.add_with(url, dir, start, Vec::new()).await
    }

    /// Like `add`, with extra request headers (cookies, referer, ...) captured from a browser.
    pub async fn add_with(
        &self,
        url: &str,
        dir: Option<&str>,
        start: bool,
        headers: Vec<(String, String)>,
    ) -> Result<u64> {
        self.add_full(url, dir, None, start, headers).await
    }

    /// The general form: optional folder and file name chosen by the user.
    /// A chosen folder becomes the default for the next download.
    pub async fn add_full(
        &self,
        url: &str,
        dir: Option<&str>,
        filename: Option<&str>,
        start: bool,
        headers: Vec<(String, String)>,
    ) -> Result<u64> {
        let url = url.trim();
        let parsed = reqwest::Url::parse(url).map_err(|e| anyhow!("invalid URL: {e}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            bail!("only http:// and https:// links are supported");
        }

        let info = self.inner.probe_cached(url, &headers).await?;
        let settings = self.settings();
        let chosen_name = filename
            .map(probe::sanitize_filename)
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| info.filename.clone());
        let is_hls = hls::is_hls(info.url.as_str(), info.content_type.as_deref());
        let chosen_name = if is_hls {
            hls::output_name(&chosen_name)
        } else {
            chosen_name
        };
        let custom = dir.map(str::trim).filter(|d| !d.is_empty());
        let category = category_of(&chosen_name);
        let category_folder = self.category_folder(&settings, category);
        let folder = match custom {
            Some(d) => PathBuf::from(d),
            None if !settings.last_dir.trim().is_empty() => PathBuf::from(settings.last_dir.trim()),
            None => category_folder.clone(),
        };
        std::fs::create_dir_all(&folder)
            .map_err(|e| anyhow!("cannot create folder {}: {e}", folder.display()))?;
        if custom.is_some() {
            self.remember_dir(&folder, &category_folder);
        }

        let path = self.unique_path(folder.join(&chosen_name));
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(chosen_name.clone());
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let item = Item {
            id,
            url: url.to_string(),
            filename,
            save_path: path.to_string_lossy().into_owned(),
            category: category.to_string(),
            total: if is_hls { None } else { info.total },
            downloaded: 0,
            status: if start {
                Status::Queued
            } else {
                Status::Paused
            },
            error: None,
            resumable: info.ranges && !is_hls,
            added: now_secs(),
            headers,
            content_type: info.content_type.clone(),
            etag: info.etag.clone(),
            last_modified: info.last_modified.clone(),
            server: info.server.clone(),
            accept_ranges: info.accept_ranges.clone(),
            http_status: info.http_status,
            retries: 0,
            started_at: None,
            completed_at: None,
            active_secs: 0,
            speed_limit_kbps: 0,
            on_complete: String::new(),
            secure_headers: None,
            integrity: String::new(),
            expected_hash: None,
            integrity_source: String::new(),
            actual_hash: None,
            scan: String::new(),
            scan_note: None,
            queue_id: MAIN_QUEUE,
            priority: 0,
        };
        self.inner.items.lock().unwrap().push(item);
        self.inner.persist_items();
        if start {
            self.inner.schedule();
        }
        Ok(id)
    }

    fn category_folder(&self, settings: &Settings, category: &str) -> PathBuf {
        if settings.categorize {
            Path::new(&settings.download_dir).join(category)
        } else {
            PathBuf::from(&settings.download_dir)
        }
    }

    /// Remembers the folder the user picked, so the next download starts there.
    /// Picking the automatic category folder itself clears the memory again.
    fn remember_dir(&self, chosen: &Path, category_folder: &Path) {
        fn norm(p: &Path) -> String {
            p.to_string_lossy()
                .trim_end_matches(|c: char| c == '\\' || c == '/')
                .to_lowercase()
        }
        let same = norm(chosen) == norm(category_folder);
        {
            let mut s = self.inner.settings.lock().unwrap();
            s.last_dir = if same {
                String::new()
            } else {
                chosen.to_string_lossy().into_owned()
            };
        }
        self.inner.persist_settings();
    }

    pub fn clear_last_dir(&self) {
        self.inner.settings.lock().unwrap().last_dir.clear();
        self.inner.persist_settings();
    }

    pub fn set_confirm_downloads(&self, value: bool) {
        self.inner.settings.lock().unwrap().confirm_downloads = value;
        self.inner.persist_settings();
    }

    /// Checks a link (name, size, resume support) and suggests where to save it,
    /// without adding anything to the list yet.
    pub async fn prepare(&self, url: &str, headers: &[(String, String)]) -> Result<Prepared> {
        let url = url.trim();
        let parsed = reqwest::Url::parse(url).map_err(|e| anyhow!("invalid URL: {e}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            bail!("only http:// and https:// links are supported");
        }
        let info = self.inner.probe_cached(url, headers).await?;
        let settings = self.settings();
        let is_hls = hls::is_hls(info.url.as_str(), info.content_type.as_deref());
        let fname = if is_hls {
            hls::output_name(&info.filename)
        } else {
            info.filename.clone()
        };
        let category = category_of(&fname);
        let category_folder = self.category_folder(&settings, category);
        let folder = if settings.last_dir.trim().is_empty() {
            category_folder.clone()
        } else {
            PathBuf::from(settings.last_dir.trim())
        };
        Ok(Prepared {
            url: url.to_string(),
            filename: fname,
            total: if is_hls { None } else { info.total },
            resumable: info.ranges && !is_hls,
            category: category.to_string(),
            folder: folder.to_string_lossy().into_owned(),
            category_folder: category_folder.to_string_lossy().into_owned(),
        })
    }

    pub fn pause(&self, id: u64) {
        let was_running = {
            let mut run = self.inner.running.lock().unwrap();
            match run.get_mut(&id) {
                Some(r) => {
                    r.reason = Reason::Pause;
                    r.stop.store(true, Ordering::SeqCst);
                    true
                }
                None => false,
            }
        };
        if !was_running {
            self.inner.update_item(id, |it| {
                if it.status == Status::Queued {
                    set_status(it, Status::Paused);
                }
            });
            self.inner.persist_items();
        }
    }

    pub fn resume(&self, id: u64) {
        self.inner.update_item(id, |it| {
            if matches!(it.status, Status::Paused | Status::Error) {
                set_status(it, Status::Queued);
                it.error = None;
            }
        });
        self.inner.schedule();
    }

    pub fn pause_all(&self) {
        let ids: Vec<u64> = self
            .inner
            .items
            .lock()
            .unwrap()
            .iter()
            .filter(|i| matches!(i.status, Status::Downloading | Status::Queued))
            .map(|i| i.id)
            .collect();
        for id in ids {
            self.pause(id);
        }
    }

    pub fn resume_all(&self) {
        {
            let mut items = self.inner.items.lock().unwrap();
            for it in items.iter_mut() {
                if matches!(it.status, Status::Paused | Status::Error) {
                    set_status(it, Status::Queued);
                    it.error = None;
                }
            }
        }
        self.inner.schedule();
    }

    /// Starts a download again from the beginning. A finished file is never overwritten:
    /// the new copy gets a new name ("name (1).ext"). Running downloads are left alone.
    pub fn restart(&self, id: u64) {
        if self.inner.running.lock().unwrap().contains_key(&id) {
            return;
        }
        let item = {
            let items = self.inner.items.lock().unwrap();
            items.iter().find(|i| i.id == id).cloned()
        };
        let Some(item) = item else { return };
        let old = PathBuf::from(&item.save_path);
        let new_path = if item.status == Status::Completed {
            self.unique_path(old.clone())
        } else {
            old.clone()
        };
        engine::remove_partial(&old);
        let new_name = new_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(item.filename.clone());
        self.inner.update_item(id, |it| {
            it.save_path = new_path.to_string_lossy().into_owned();
            it.filename = new_name;
            it.downloaded = 0;
            set_status(it, Status::Queued);
            it.error = None;
            it.completed_at = None;
            it.retries = 0;
            it.active_secs = 0;
            it.integrity.clear();
            it.actual_hash = None;
            it.scan.clear();
            it.scan_note = None;
            if it.integrity_source == "server" {
                it.expected_hash = None;
                it.integrity_source.clear();
            }
        });
        self.inner.schedule();
    }

    /// Speed limit of one download (KiB/s, 0 = none). Applied to the running engine at once.
    pub fn set_speed_limit(&self, id: u64, kbps: u64) {
        self.inner.update_item(id, |it| it.speed_limit_kbps = kbps);
        if let Some(r) = self.inner.running.lock().unwrap().get(&id) {
            r.limiter.set_rate(kbps.saturating_mul(1024));
        }
        self.inner.persist_items();
    }

    /// What to do when this download completes.
    pub fn set_on_complete(&self, id: u64, action: &str) -> Result<()> {
        if !valid_action(action) {
            bail!("unknown completion action: {action}");
        }
        let value = if action == "none" { "" } else { action };
        self.inner
            .update_item(id, |it| it.on_complete = value.to_string());
        self.inner.persist_items();
        Ok(())
    }

    /// Removes the item from the list. Temporary files are always deleted;
    /// the finished file only when `delete_file` is true.
    pub fn remove(&self, id: u64, delete_file: bool) {
        let item = {
            let mut items = self.inner.items.lock().unwrap();
            let pos = items.iter().position(|i| i.id == id);
            pos.map(|p| items.remove(p))
        };
        let Some(item) = item else { return };
        let out = PathBuf::from(&item.save_path);

        let was_running = {
            let mut run = self.inner.running.lock().unwrap();
            match run.get_mut(&id) {
                Some(r) => {
                    // The download task cleans up after itself when it notices the stop.
                    r.reason = Reason::Remove { delete_file };
                    r.stop.store(true, Ordering::SeqCst);
                    true
                }
                None => false,
            }
        };
        if !was_running {
            engine::remove_partial(&out);
            if delete_file && item.status == Status::Completed {
                let _ = std::fs::remove_file(&out);
            }
        }
        self.inner.persist_items();
    }

    /// Removes finished downloads from the list (files stay on disk).
    pub fn clear_completed(&self) {
        self.inner
            .items
            .lock()
            .unwrap()
            .retain(|i| i.status != Status::Completed);
        self.inner.persist_items();
    }

    /// Tells the manager the local time (minute of the day, 0..1440) so the speed schedule
    /// can switch limits. Returns the global limit now in force, in KiB/s.
    pub fn apply_speed_schedule(&self, minute_of_day: u32) -> u64 {
        self.inner
            .rate_minute
            .store(minute_of_day, Ordering::Relaxed);
        self.inner.apply_rate()
    }

    pub fn queues(&self) -> Vec<Queue> {
        self.inner.queues.lock().unwrap().clone()
    }

    /// Creates a queue (`id == 0`) or changes an existing one. Returns its id.
    pub fn save_queue(&self, mut q: Queue) -> Result<u32> {
        q.sanitize();
        let id = {
            let mut list = self.inner.queues.lock().unwrap();
            if q.id == 0 {
                q.id = list.iter().map(|x| x.id).max().unwrap_or(0) + 1;
                list.push(q.clone());
            } else {
                match list.iter_mut().find(|x| x.id == q.id) {
                    Some(slot) => *slot = q.clone(),
                    None => bail!("queue not found"),
                }
            }
            q.id
        };
        self.inner
            .queue_limiter(id)
            .set_rate(q.speed_limit_kbps.saturating_mul(1024));
        self.inner.persist_queues();
        self.inner.schedule();
        Ok(id)
    }

    /// Deletes a queue; its downloads move to the main queue. The main queue cannot be deleted.
    pub fn delete_queue(&self, id: u32) -> Result<()> {
        if id == MAIN_QUEUE {
            bail!("the main queue cannot be deleted");
        }
        let mut list = self.inner.queues.lock().unwrap();
        let before = list.len();
        list.retain(|q| q.id != id);
        if list.len() == before {
            bail!("queue not found");
        }
        drop(list);
        for it in self.inner.items.lock().unwrap().iter_mut() {
            if it.queue_id == id {
                it.queue_id = MAIN_QUEUE;
            }
        }
        self.inner.queue_limiters.lock().unwrap().remove(&id);
        self.inner.persist_queues();
        self.inner.persist_items();
        self.inner.schedule();
        Ok(())
    }

    /// Priority of one download: -1 low, 0 normal, 1 high.
    pub fn set_priority(&self, id: u64, priority: i32) {
        let p = priority.clamp(-1, 1);
        self.inner.update_item(id, |it| it.priority = p);
        self.inner.persist_items();
        self.inner.schedule();
    }

    /// Moves a download to another queue. A running download must be paused first.
    pub fn set_item_queue(&self, id: u64, queue_id: u32) -> Result<()> {
        if !self
            .inner
            .queues
            .lock()
            .unwrap()
            .iter()
            .any(|q| q.id == queue_id)
        {
            bail!("queue not found");
        }
        if self.inner.running.lock().unwrap().contains_key(&id) {
            bail!("pause the download before moving it to another queue");
        }
        self.inner.update_item(id, |it| it.queue_id = queue_id);
        self.inner.persist_items();
        self.inner.schedule();
        Ok(())
    }

    /// Starts every paused or failed download of one queue (and enables the queue).
    pub fn start_queue(&self, qid: u32) {
        for q in self.inner.queues.lock().unwrap().iter_mut() {
            if q.id == qid {
                q.enabled = true;
            }
        }
        {
            let mut items = self.inner.items.lock().unwrap();
            for it in items.iter_mut().filter(|i| i.queue_id == qid) {
                if matches!(it.status, Status::Paused | Status::Error) {
                    set_status(it, Status::Queued);
                    it.error = None;
                }
            }
        }
        self.inner.persist_queues();
        self.inner.schedule();
    }

    /// Pauses every active or waiting download of one queue.
    pub fn stop_queue(&self, qid: u32) {
        let ids: Vec<u64> = self
            .inner
            .items
            .lock()
            .unwrap()
            .iter()
            .filter(|i| {
                i.queue_id == qid && matches!(i.status, Status::Downloading | Status::Queued)
            })
            .map(|i| i.id)
            .collect();
        for id in ids {
            self.pause(id);
        }
    }

    /// (something is still running or waiting, number of failed downloads) for one queue.
    pub fn queue_state(&self, qid: u32) -> (bool, usize) {
        let items = self.inner.items.lock().unwrap();
        let mut busy = false;
        let mut failed = 0;
        for it in items.iter().filter(|i| i.queue_id == qid) {
            match it.status {
                Status::Downloading | Status::Queued => busy = true,
                Status::Error => failed += 1,
                _ => {}
            }
            if it.integrity == "verifying" || it.scan == "scanning" {
                busy = true;
            }
            if it.integrity == "mismatch" || it.scan == "threat" {
                failed += 1;
            }
        }
        (busy, failed)
    }

    /// Forgets the connection limits remembered per host.
    pub fn clear_host_limits(&self) {
        self.inner.host_caps.lock().unwrap().clear();
        self.inner.persist_host_caps();
    }

    /// Checks the finished file against a checksum. `expected` is what the user typed
    /// ("sha256:<hex>", "SHA-256=<hex>", or bare hex); None re-checks against the stored
    /// one, or only calculates SHA-256 when nothing is known. Runs in the background.
    pub fn verify(&self, id: u64, expected: Option<&str>) -> Result<()> {
        let parsed = match expected.map(str::trim).filter(|e| !e.is_empty()) {
            Some(e) => Some(integrity::parse_expected(e).ok_or_else(|| {
                anyhow!("invalid checksum: use 64 hex digits (SHA-256) or 128 (SHA-512)")
            })?),
            None => None,
        };
        {
            let mut items = self.inner.items.lock().unwrap();
            let it = items
                .iter_mut()
                .find(|i| i.id == id)
                .ok_or_else(|| anyhow!("download not found"))?;
            if it.status != Status::Completed {
                bail!("the download is not finished yet");
            }
            if !Path::new(&it.save_path).is_file() {
                bail!("the finished file no longer exists on disk");
            }
            if it.integrity == "verifying" {
                return Ok(());
            }
            if let Some((a, hex)) = parsed {
                it.expected_hash = Some(integrity::format_hash(a, &hex));
                it.integrity_source = "user".to_string();
            }
        }
        let inner = self.inner.clone();
        tokio::spawn(async move { verify_item(&inner, id).await });
        Ok(())
    }

    /// Runs the configured virus scanner on a finished file again.
    pub fn rescan(&self, id: u64) -> Result<()> {
        let ok = {
            let items = self.inner.items.lock().unwrap();
            items
                .iter()
                .any(|i| i.id == id && i.status == Status::Completed && i.scan != "scanning")
        };
        if !ok {
            bail!("nothing to scan");
        }
        if self.inner.settings.lock().unwrap().scanner_path.is_empty() {
            bail!("no scanner is configured in Settings");
        }
        let inner = self.inner.clone();
        tokio::spawn(async move { scan_item(&inner, id).await });
        Ok(())
    }

    /// True while a checksum check or virus scan of this download is still running.
    pub fn is_post_processing(&self, id: u64) -> bool {
        self.inner
            .items
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.id == id)
            .map_or(false, |i| {
                i.integrity == "verifying" || i.scan == "scanning"
            })
    }

    /// False when the file failed the checksum or the scanner found a threat.
    pub fn is_safe_to_open(&self, id: u64) -> bool {
        self.inner
            .items
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.id == id)
            .map_or(false, |i| i.integrity != "mismatch" && i.scan != "threat")
    }

    pub fn path_of(&self, id: u64) -> Option<String> {
        self.inner
            .items
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.save_path.clone())
    }

    /// Current list with live speed and ETA. Call it about twice a second.
    pub fn snapshot(&self) -> Vec<ItemView> {
        let now = Instant::now();
        let live: HashMap<u64, Live> = {
            let mut run = self.inner.running.lock().unwrap();
            run.iter_mut()
                .map(|(id, r)| (*id, update_live(r, now)))
                .collect()
        };
        let items = self.inner.items.lock().unwrap();
        items
            .iter()
            .map(|it| make_view(it, live.get(&it.id)))
            .collect()
    }

    /// Full live state of one download for the details window.
    pub fn details(&self, id: u64) -> Option<DownloadDetails> {
        let now = Instant::now();
        let (live, engine, avg_speed, session_secs, note) = {
            let mut run = self.inner.running.lock().unwrap();
            match run.get_mut(&id) {
                Some(r) => {
                    let live = update_live(r, now);
                    let engine = r.progress.snapshot(128);
                    let secs = r.started.elapsed().as_secs();
                    let avg = r.progress.session() / secs.max(1);
                    (Some(live), Some(engine), avg, secs, r.progress.note())
                }
                None => (None, None, 0, 0, String::new()),
            }
        };
        let item = {
            let items = self.inner.items.lock().unwrap();
            items.iter().find(|i| i.id == id).cloned()
        }?;
        let view = make_view(&item, live.as_ref());
        Some(DownloadDetails {
            elapsed_secs: item.active_secs + session_secs,
            view,
            engine,
            avg_speed,
            resume_note: note,
            integrity: if item.integrity.is_empty() {
                "not_verified".to_string()
            } else {
                item.integrity.clone()
            },
            queue: self
                .inner
                .queues
                .lock()
                .unwrap()
                .iter()
                .find(|q| q.id == item.queue_id)
                .map_or_else(|| "Main".to_string(), |q| q.name.clone()),
        })
    }
}
