//! Download manager: keeps the list of downloads, runs a limited number of them at
//! the same time (queue), supports pause/resume/remove, and saves everything to disk
//! so the list survives a restart.
//!
//! Methods that can start a download (`add`, `resume`, `resume_all`, `set_settings`)
//! must be called from inside a Tokio runtime, because they spawn tasks.

use crate::engine::{self, Job, Progress, Stopped};
use crate::limiter::Limiter;
use crate::probe;
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
    /// Parallel connections per download.
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: String::new(),
            connections: 16,
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
        }
    }
}

impl Settings {
    fn sanitize(&mut self, fallback_dir: &Path) {
        self.connections = self.connections.clamp(1, 64);
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
}

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
    last_t: Instant,
    last_bytes: u64,
    speed: f64,
}

struct Inner {
    client: reqwest::Client,
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

fn valid_hm(s: &str) -> bool {
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
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "ts" | "mpg"
        | "mpeg" => "Video",
        "exe" | "msi" | "msix" | "apk" | "dmg" | "iso" | "deb" | "appimage" => "Programs",
        _ => "General",
    }
}

impl Inner {
    fn persist_items(&self) {
        let _guard = self.persist_lock.lock().unwrap();
        let data = {
            let items = self.items.lock().unwrap();
            serde_json::to_vec_pretty(&*items)
        };
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
    fn schedule(self: &Arc<Self>) {
        loop {
            let max = self.settings.lock().unwrap().max_concurrent.max(1);
            if self.running.lock().unwrap().len() >= max {
                break;
            }
            let id = {
                let mut items = self.items.lock().unwrap();
                match items.iter_mut().find(|i| i.status == Status::Queued) {
                    Some(it) => {
                        it.status = Status::Downloading;
                        it.error = None;
                        it.id
                    }
                    None => break,
                }
            };

            let progress = Arc::new(Progress::default());
            let stop = Arc::new(AtomicBool::new(false));
            self.running.lock().unwrap().insert(
                id,
                Running {
                    progress: progress.clone(),
                    stop: stop.clone(),
                    reason: Reason::Pause,
                    last_t: Instant::now(),
                    last_bytes: 0,
                    speed: 0.0,
                },
            );
            let inner = self.clone();
            tokio::spawn(async move { run_download(inner, id, progress, stop).await });
        }
        self.persist_items();
    }
}

async fn run_download(inner: Arc<Inner>, id: u64, progress: Arc<Progress>, stop: Arc<AtomicBool>) {
    let found = {
        let items = inner.items.lock().unwrap();
        items
            .iter()
            .find(|i| i.id == id)
            .map(|i| (i.url.clone(), PathBuf::from(&i.save_path), i.headers.clone()))
    };
    let Some((url, out, item_headers)) = found else {
        inner.running.lock().unwrap().remove(&id);
        inner.schedule();
        return;
    };
    let settings = inner.settings.lock().unwrap().clone();

    let result: Result<()> = async {
        let headers = headers_from(&item_headers);
        // Probing again on every start also refreshes redirects and the resume validator.
        let info = probe::probe(&inner.client, &url, &headers).await?;
        inner.update_item(id, |it| {
            it.total = info.total;
            it.resumable = info.ranges;
        });
        engine::run(Job {
            client: inner.client.clone(),
            probe: info,
            out: out.clone(),
            connections: settings.connections,
            retries: settings.retries,
            headers,
            limiter: inner.limiter.clone(),
            progress: progress.clone(),
            stop: stop.clone(),
        })
        .await
    }
    .await;

    let reason = inner.running.lock().unwrap().remove(&id).map(|r| r.reason);
    let downloaded = progress.downloaded();

    if let Some(Reason::Remove { delete_file }) = reason {
        engine::remove_partial(&out);
        if delete_file && result.is_ok() {
            let _ = std::fs::remove_file(&out);
        }
        inner.schedule();
        return;
    }

    match result {
        Ok(()) => inner.update_item(id, |it| {
            it.status = Status::Completed;
            it.error = None;
            it.total = it.total.or(Some(downloaded));
            it.downloaded = it.total.unwrap_or(downloaded);
        }),
        Err(e) if e.downcast_ref::<Stopped>().is_some() => inner.update_item(id, |it| {
            it.status = Status::Paused;
            it.downloaded = downloaded;
        }),
        Err(e) => {
            let msg = format!("{:#}", e);
            inner.update_item(id, |it| {
                it.status = Status::Error;
                it.error = Some(msg);
                it.downloaded = downloaded;
            });
        }
    }
    inner.schedule();
}

impl Manager {
    pub fn new(data_dir: PathBuf, default_download_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let client = probe::build_client(probe::DEFAULT_UA)?;

        let mut settings: Settings =
            read_json(&data_dir.join("settings.json")).unwrap_or_default();
        settings.sanitize(&default_download_dir);

        let mut items: Vec<Item> = read_json(&data_dir.join("downloads.json")).unwrap_or_default();
        for it in items.iter_mut() {
            // The app was closed while these were active: let the user resume them.
            if matches!(it.status, Status::Downloading | Status::Queued) {
                it.status = Status::Paused;
            }
            if it.status != Status::Completed {
                it.downloaded = engine::saved_progress(Path::new(&it.save_path)).unwrap_or(0);
            }
        }
        let next_id = items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        let limiter = Arc::new(Limiter::new(settings.speed_limit_kbps.saturating_mul(1024)));

        let inner = Arc::new(Inner {
            client,
            data_dir,
            items: Mutex::new(items),
            running: Mutex::new(HashMap::new()),
            settings: Mutex::new(settings),
            limiter,
            next_id: AtomicU64::new(next_id),
            persist_lock: Mutex::new(()),
        });
        inner.persist_settings();
        inner.persist_items();
        Ok(Self { inner })
    }

    pub fn settings(&self) -> Settings {
        self.inner.settings.lock().unwrap().clone()
    }

    pub fn set_settings(&self, mut s: Settings) -> Result<()> {
        let (fallback, token) = {
            let cur = self.inner.settings.lock().unwrap();
            (cur.download_dir.clone(), cur.api_token.clone())
        };
        s.api_token = token; // the UI can never change the pairing token
        s.sanitize(Path::new(&fallback));
        std::fs::create_dir_all(&s.download_dir)
            .map_err(|e| anyhow!("cannot create folder {}: {e}", s.download_dir))?;
        self.inner
            .limiter
            .set_rate(s.speed_limit_kbps.saturating_mul(1024));
        *self.inner.settings.lock().unwrap() = s;
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
        let url = url.trim();
        let parsed = reqwest::Url::parse(url).map_err(|e| anyhow!("invalid URL: {e}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            bail!("only http:// and https:// links are supported");
        }

        let info = probe::probe(&self.inner.client, url, &headers_from(&headers)).await?;
        let settings = self.settings();
        let custom = dir.map(str::trim).filter(|d| !d.is_empty());
        let category = category_of(&info.filename);
        let folder = match custom {
            Some(d) => PathBuf::from(d),
            None if settings.categorize => Path::new(&settings.download_dir).join(category),
            None => PathBuf::from(&settings.download_dir),
        };
        std::fs::create_dir_all(&folder)
            .map_err(|e| anyhow!("cannot create folder {}: {e}", folder.display()))?;

        let path = self.unique_path(folder.join(&info.filename));
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(info.filename.clone());
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let item = Item {
            id,
            url: url.to_string(),
            filename,
            save_path: path.to_string_lossy().into_owned(),
            category: category.to_string(),
            total: info.total,
            downloaded: 0,
            status: if start { Status::Queued } else { Status::Paused },
            error: None,
            resumable: info.ranges,
            added: now_secs(),
            headers,
        };
        self.inner.items.lock().unwrap().push(item);
        self.inner.persist_items();
        if start {
            self.inner.schedule();
        }
        Ok(id)
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
                    it.status = Status::Paused;
                }
            });
            self.inner.persist_items();
        }
    }

    pub fn resume(&self, id: u64) {
        self.inner.update_item(id, |it| {
            if matches!(it.status, Status::Paused | Status::Error) {
                it.status = Status::Queued;
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
                    it.status = Status::Queued;
                    it.error = None;
                }
            }
        }
        self.inner.schedule();
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
        // id -> (downloaded bytes, speed, stopping)
        let live: HashMap<u64, (u64, u64, bool)> = {
            let mut run = self.inner.running.lock().unwrap();
            run.iter_mut()
                .map(|(id, r)| {
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
                    (
                        *id,
                        (
                            r.progress.downloaded(),
                            r.speed as u64,
                            r.stop.load(Ordering::Relaxed),
                        ),
                    )
                })
                .collect()
        };

        let items = self.inner.items.lock().unwrap();
        items
            .iter()
            .map(|it| {
                let mut item = it.clone();
                item.headers.clear(); // never send cookies to the UI
                let mut speed = 0u64;
                let mut stopping = false;
                if let Some((dl, sp, st)) = live.get(&item.id) {
                    item.downloaded = *dl;
                    speed = *sp;
                    stopping = *st;
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
                }
            })
            .collect()
    }
}
