#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod server;

use chrono::{Datelike, Local, Timelike};
use rdm_core::manager::{
    DownloadDetails, ItemView, Manager as Downloads, Prepared, Settings, Status,
};
use rdm_core::queues::Queue;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

type Res<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    format!("{:#}", e)
}

/// Sends the current list to the UI right away (after a user action).
fn notify(app: &AppHandle, dl: &Downloads) {
    let _ = app.emit("downloads", dl.snapshot());
}

/// A browser download waiting for the user's decision in the confirmation window.
#[derive(Clone)]
pub struct Pending {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub prepared: Prepared,
}

#[derive(Default)]
pub struct PendingStore {
    map: Mutex<HashMap<u64, Pending>>,
    next: AtomicU64,
}

impl PendingStore {
    pub fn insert(&self, p: Pending) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.map.lock().unwrap().insert(id, p);
        id
    }
    pub fn get(&self, id: u64) -> Option<Pending> {
        self.map.lock().unwrap().get(&id).cloned()
    }
    pub fn remove(&self, id: u64) -> Option<Pending> {
        self.map.lock().unwrap().remove(&id)
    }
}

/// Opens the small "New download" window (like IDM's file-info dialog).
pub fn open_confirm_window(app: &AppHandle, id: u64) {
    let label = format!("confirm-{id}");
    let result = tauri::WebviewWindowBuilder::new(
        app,
        label.clone(),
        tauri::WebviewUrl::App("confirm.html".into()),
    )
    .title("RDM")
    .inner_size(640.0, 520.0)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .always_on_top(true)
    .center()
    .theme(Some(tauri::Theme::Dark))
    .build();
    if let Err(e) = result {
        eprintln!("cannot open the confirmation window: {e}");
    }
}

fn close_confirm(app: &AppHandle, id: u64) {
    let label = format!("confirm-{id}");
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.close();
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

// Every command is `async` on purpose: the manager spawns Tokio tasks,
// and async commands run inside Tauri's Tokio runtime.

#[tauri::command]
async fn list_downloads(dl: State<'_, Downloads>) -> Res<Vec<ItemView>> {
    Ok(dl.snapshot())
}

#[tauri::command]
async fn add_download(
    app: AppHandle,
    dl: State<'_, Downloads>,
    url: String,
    dir: Option<String>,
    start_now: bool,
) -> Res<u64> {
    let id = dl.add(&url, dir.as_deref(), start_now).await.map_err(err)?;
    notify(&app, &dl);
    if start_now {
        auto_details(&app, &dl, id);
    }
    Ok(id)
}

#[tauri::command]
async fn pause_download(app: AppHandle, dl: State<'_, Downloads>, id: u64) -> Res<()> {
    dl.pause(id);
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn resume_download(app: AppHandle, dl: State<'_, Downloads>, id: u64) -> Res<()> {
    dl.resume(id);
    notify(&app, &dl);
    auto_details(&app, &dl, id);
    Ok(())
}

#[tauri::command]
async fn remove_download(
    app: AppHandle,
    dl: State<'_, Downloads>,
    id: u64,
    delete_file: bool,
) -> Res<()> {
    dl.remove(id, delete_file);
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn pause_all(app: AppHandle, dl: State<'_, Downloads>) -> Res<()> {
    dl.pause_all();
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn resume_all(app: AppHandle, dl: State<'_, Downloads>) -> Res<()> {
    dl.resume_all();
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn clear_completed(app: AppHandle, dl: State<'_, Downloads>) -> Res<()> {
    dl.clear_completed();
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn get_settings(dl: State<'_, Downloads>) -> Res<Settings> {
    Ok(dl.settings())
}

#[tauri::command]
async fn save_settings(app: AppHandle, dl: State<'_, Downloads>, settings: Settings) -> Res<()> {
    dl.set_settings(settings).map_err(err)?;
    notify(&app, &dl);
    Ok(())
}

/// Opens (or focuses) the live details window of one download.
#[tauri::command]
async fn open_details(app: AppHandle, id: u64) -> Res<()> {
    open_details_window(&app, id)
}

/// Opens the details window by itself when the user starts a download (setting-controlled).
/// Never used for batch adds or queue starts, so hundreds of windows cannot open at once.
pub(crate) fn auto_details(app: &AppHandle, dl: &Downloads, id: u64) {
    // At most 3 details windows open by themselves, so a flood of adds cannot flood the screen.
    let open = app
        .webview_windows()
        .keys()
        .filter(|k| k.starts_with("details-"))
        .count();
    if dl.settings().show_details_on_start && open < 3 {
        let _ = open_details_window(app, id);
    }
}

pub(crate) fn open_details_window(app: &AppHandle, id: u64) -> Res<()> {
    let label = format!("details-{id}");
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App("details.html".into()))
        .title("RDM")
        .inner_size(920.0, 760.0)
        .min_inner_size(740.0, 540.0)
        .center()
        .theme(Some(tauri::Theme::Dark))
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn close_details(app: AppHandle, id: u64) -> Res<()> {
    if let Some(w) = app.get_webview_window(&format!("details-{id}")) {
        let _ = w.close();
    }
    Ok(())
}

#[tauri::command]
async fn get_details(dl: State<'_, Downloads>, id: u64) -> Res<Option<DownloadDetails>> {
    Ok(dl.details(id))
}

#[tauri::command]
async fn set_item_speed_limit(
    app: AppHandle,
    dl: State<'_, Downloads>,
    id: u64,
    kbps: u64,
) -> Res<()> {
    dl.set_speed_limit(id, kbps);
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn set_on_complete(
    app: AppHandle,
    dl: State<'_, Downloads>,
    id: u64,
    action: String,
) -> Res<()> {
    dl.set_on_complete(id, &action).map_err(err)?;
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn set_item_priority(
    app: AppHandle,
    dl: State<'_, Downloads>,
    id: u64,
    priority: i32,
) -> Res<()> {
    dl.set_priority(id, priority);
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn get_queues(dl: State<'_, Downloads>) -> Res<Vec<Queue>> {
    Ok(dl.queues())
}

#[tauri::command]
async fn save_queue(app: AppHandle, dl: State<'_, Downloads>, queue: Queue) -> Res<u32> {
    let id = dl.save_queue(queue).map_err(err)?;
    notify(&app, &dl);
    Ok(id)
}

#[tauri::command]
async fn delete_queue(app: AppHandle, dl: State<'_, Downloads>, id: u32) -> Res<()> {
    dl.delete_queue(id).map_err(err)?;
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn set_item_queue(
    app: AppHandle,
    dl: State<'_, Downloads>,
    id: u64,
    queue_id: u32,
) -> Res<()> {
    dl.set_item_queue(id, queue_id).map_err(err)?;
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn start_queue(app: AppHandle, dl: State<'_, Downloads>, id: u32) -> Res<()> {
    dl.start_queue(id);
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn stop_queue(app: AppHandle, dl: State<'_, Downloads>, id: u32) -> Res<()> {
    dl.stop_queue(id);
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn clear_host_limits(dl: State<'_, Downloads>) -> Res<()> {
    dl.clear_host_limits();
    Ok(())
}

#[tauri::command]
async fn verify_download(
    app: AppHandle,
    dl: State<'_, Downloads>,
    id: u64,
    expected: Option<String>,
) -> Res<()> {
    dl.verify(id, expected.as_deref()).map_err(err)?;
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn rescan_download(app: AppHandle, dl: State<'_, Downloads>, id: u64) -> Res<()> {
    dl.rescan(id).map_err(err)?;
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn restart_download(app: AppHandle, dl: State<'_, Downloads>, id: u64) -> Res<()> {
    dl.restart(id);
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn get_pending(store: State<'_, PendingStore>, id: u64) -> Res<Prepared> {
    store
        .get(id)
        .map(|p| p.prepared)
        .ok_or_else(|| "this request is no longer valid".to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn confirm_pending(
    app: AppHandle,
    dl: State<'_, Downloads>,
    store: State<'_, PendingStore>,
    id: u64,
    folder: String,
    filename: String,
    start: bool,
    dont_ask: bool,
) -> Res<()> {
    let p = store
        .get(id)
        .ok_or_else(|| "this request is no longer valid".to_string())?;
    let new_id = dl
        .add_full(
            &p.url,
            Some(folder.as_str()),
            Some(filename.as_str()),
            start,
            p.headers.clone(),
        )
        .await
        .map_err(err)?;
    if start {
        auto_details(&app, &dl, new_id);
    }
    // "Don't ask again" no longer exists: downloads from the browser always need a confirmation.
    let _ = dont_ask;
    store.remove(id);
    close_confirm(&app, id);
    notify(&app, &dl);
    Ok(())
}

#[tauri::command]
async fn cancel_pending(app: AppHandle, store: State<'_, PendingStore>, id: u64) -> Res<()> {
    store.remove(id);
    close_confirm(&app, id);
    Ok(())
}

#[tauri::command]
async fn clear_last_dir(app: AppHandle, dl: State<'_, Downloads>) -> Res<()> {
    dl.clear_last_dir();
    notify(&app, &dl);
    Ok(())
}

/// Port, pairing token and server state for the browser-extension section.
#[tauri::command]
async fn get_integration(
    dl: State<'_, Downloads>,
    api: State<'_, server::ApiStatus>,
) -> Res<serde_json::Value> {
    Ok(serde_json::json!({
        "port": server::PORT,
        "token": dl.settings().api_token,
        "running": api.0.load(Ordering::Relaxed),
    }))
}

#[tauri::command]
async fn cancel_shutdown() -> Res<()> {
    run_shutdown(&["/a"])
}

#[tauri::command]
async fn open_file(dl: State<'_, Downloads>, id: u64) -> Res<()> {
    let p = dl
        .path_of(id)
        .ok_or_else(|| "download not found".to_string())?;
    let path = Path::new(&p);
    if !path.exists() {
        return Err("file not found".into());
    }
    open_default(path)
}

#[tauri::command]
async fn open_folder(dl: State<'_, Downloads>, id: u64) -> Res<()> {
    let p = dl
        .path_of(id)
        .ok_or_else(|| "download not found".to_string())?;
    reveal(Path::new(&p))
}

#[cfg(windows)]
fn open_default(path: &Path) -> Res<()> {
    Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn open_default(path: &Path) -> Res<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Opens the folder and selects the file when it exists.
#[cfg(windows)]
fn reveal(path: &Path) -> Res<()> {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("explorer");
    if path.exists() {
        cmd.raw_arg(format!("/select,\"{}\"", path.display()));
    } else if let Some(parent) = path.parent() {
        cmd.arg(parent);
    }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn reveal(path: &Path) -> Res<()> {
    let dir = path.parent().unwrap_or(path);
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener)
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn run_shutdown(args: &[&str]) -> Res<()> {
    use std::os::windows::process::CommandExt;
    Command::new("shutdown")
        .args(args)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn run_shutdown(_args: &[&str]) -> Res<()> {
    Err("power actions are only supported on Windows".into())
}

/// Runs the "after scheduled downloads finish" action.
/// Shutdown waits 60 seconds so the user can cancel it from the app.
fn power_action(app: &AppHandle, action: &str) {
    match action {
        "shutdown" => {
            if run_shutdown(&["/s", "/t", "60"]).is_ok() {
                let _ = app.emit("power", "shutdown");
            }
        }
        "hibernate" => {
            let _ = run_shutdown(&["/h"]);
        }
        _ => {}
    }
}

/// What to do when a single download completes (chosen in its details window).
fn run_completion(app: &AppHandle, action: &str, path: &str) {
    match action {
        "open_file" => {
            let _ = open_default(Path::new(path));
        }
        "open_folder" => {
            let _ = reveal(Path::new(path));
        }
        "shutdown" | "hibernate" => power_action(app, action),
        _ => {}
    }
}

#[derive(Default)]
struct QueueSched {
    /// A scheduled start found work to do and nothing else has happened since.
    armed: bool,
    was_busy: bool,
}

fn main() {
    tauri::Builder::default()
        // Must be registered first: a second launch just brings the first window forward.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                if window.label() == "main" {
                    let to_tray = window
                        .app_handle()
                        .state::<Downloads>()
                        .settings()
                        .close_to_tray;
                    if to_tray {
                        let _ = window.hide();
                        api.prevent_close();
                    }
                }
            }
            WindowEvent::Destroyed => {
                // Closing the confirmation window with X = cancel.
                if let Some(id) = window
                    .label()
                    .strip_prefix("confirm-")
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    let _ = window.app_handle().state::<PendingStore>().remove(id);
                }
            }
            _ => {}
        })
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            // A crash leaves a trace in crash.log (next to settings.json) so it can be diagnosed.
            let crash_log = data_dir.join("crash.log");
            std::panic::set_hook(Box::new(move |info| {
                let line = format!("{} {}\n", Local::now().format("%Y-%m-%d %H:%M:%S"), info);
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&crash_log)
                {
                    let _ = std::io::Write::write_all(&mut f, line.as_bytes());
                }
            }));
            let default_dir = app
                .path()
                .download_dir()
                .unwrap_or_else(|_| data_dir.join("Downloads"));
            let manager = Downloads::new(data_dir, default_dir)?;
            app.manage(manager.clone());
            app.manage(server::ApiStatus(AtomicBool::new(false)));
            app.manage(PendingStore::default());

            // Browser-extension API.
            server::start(app.handle().clone(), manager.clone());

            // System tray.
            let show = MenuItem::with_id(app, "show", "نمایش RDM / Show", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "خروج / Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("RDM")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => {
                        // Pause everything first so progress is saved cleanly.
                        let app = app.clone();
                        let m = app.state::<Downloads>().inner().clone();
                        tauri::async_runtime::spawn(async move {
                            m.pause_all();
                            tokio::time::sleep(Duration::from_millis(1200)).await;
                            app.exit(0);
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Details windows get their own, faster, targeted stream of live state.
            let details_manager = manager.clone();
            let details_handle = app.handle().clone();

            // Ticker: live progress for the UI + the scheduler.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut was_active = false;
                let mut qsched: HashMap<u32, QueueSched> = HashMap::new();
                let mut last_minute: Option<(u32, u32, u32)> = None;
                let mut prev_status: HashMap<u64, Status> = HashMap::new();
                let mut waiting: std::collections::HashSet<u64> = Default::default();
                loop {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    let snap = manager.snapshot();
                    // Per-download completion actions (only for downloads that really completed).
                    for v in &snap {
                        let was = prev_status.insert(v.item.id, v.item.status);
                        if was == Some(Status::Downloading)
                            && v.item.status == Status::Completed
                            && !v.item.on_complete.is_empty()
                        {
                            waiting.insert(v.item.id);
                        }
                    }
                    // Completion actions wait for the checksum check and the virus scan, and are
                    // skipped for a file that failed either one.
                    for v in &snap {
                        let id = v.item.id;
                        if waiting.contains(&id) && !manager.is_post_processing(id) {
                            waiting.remove(&id);
                            if manager.is_safe_to_open(id) {
                                run_completion(&handle, &v.item.on_complete, &v.item.save_path);
                            }
                        }
                    }
                    let active = snap.iter().any(|v| v.item.status == Status::Downloading);
                    if active || was_active {
                        let _ = handle.emit("downloads", snap.clone());
                    }
                    was_active = active;

                    // --- scheduler: every queue has its own start/stop time and finish action ---
                    let now = Local::now();
                    let queues = manager.queues();
                    let key = (now.ordinal(), now.hour(), now.minute());
                    if last_minute != Some(key) {
                        last_minute = Some(key);
                        // The speed schedule switches limits at minute boundaries.
                        manager.apply_speed_schedule(now.hour() * 60 + now.minute());
                        let hm = format!("{:02}:{:02}", now.hour(), now.minute());
                        for q in queues.iter().filter(|q| q.schedule_enabled) {
                            if !q.schedule_start.is_empty() && q.schedule_start == hm {
                                let has_work = snap.iter().any(|v| {
                                    v.item.queue_id == q.id && v.item.status == Status::Queued
                                });
                                manager.start_queue_scheduled(q.id);
                                qsched.entry(q.id).or_default().armed = has_work;
                                let _ = handle.emit("downloads", manager.snapshot());
                            }
                            if !q.schedule_stop.is_empty() && q.schedule_stop == hm {
                                manager.stop_queue(q.id);
                                qsched.entry(q.id).or_default().armed = false;
                                let _ = handle.emit("downloads", manager.snapshot());
                            }
                        }
                    }
                    // A queue that finished a scheduled run may shut the computer down, but by
                    // default only when nothing failed (no errors, no checksum or scan problems).
                    for q in &queues {
                        let (busy, failed) = manager.queue_state(q.id);
                        let st = qsched.entry(q.id).or_default();
                        if st.was_busy && !busy && st.armed {
                            st.armed = false;
                            let ok = !q.only_if_all_succeeded || failed == 0;
                            if q.schedule_enabled && q.after_finish != "none" && ok {
                                power_action(&handle, &q.after_finish);
                            }
                        }
                        st.was_busy = busy;
                    }
                }
            });
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    for (label, _) in details_handle.webview_windows() {
                        let Some(id) = label
                            .strip_prefix("details-")
                            .and_then(|s| s.parse::<u64>().ok())
                        else {
                            continue;
                        };
                        if let Some(d) = details_manager.details(id) {
                            let _ = details_handle.emit_to(label.as_str(), "details", d);
                        }
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_downloads,
            add_download,
            pause_download,
            resume_download,
            remove_download,
            pause_all,
            resume_all,
            clear_completed,
            get_settings,
            save_settings,
            get_integration,
            open_details,
            close_details,
            get_details,
            set_item_speed_limit,
            set_on_complete,
            restart_download,
            verify_download,
            get_queues,
            set_item_priority,
            save_queue,
            delete_queue,
            set_item_queue,
            start_queue,
            stop_queue,
            clear_host_limits,
            rescan_download,
            get_pending,
            confirm_pending,
            cancel_pending,
            clear_last_dir,
            cancel_shutdown,
            open_file,
            open_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running RDM");
}
