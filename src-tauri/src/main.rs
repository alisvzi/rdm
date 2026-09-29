#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod server;

use chrono::{Datelike, Local, Timelike};
use rdm_core::manager::{ItemView, Manager as Downloads, Settings, Status};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
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
    let p = dl.path_of(id).ok_or_else(|| "download not found".to_string())?;
    let path = Path::new(&p);
    if !path.exists() {
        return Err("file not found".into());
    }
    open_default(path)
}

#[tauri::command]
async fn open_folder(dl: State<'_, Downloads>, id: u64) -> Res<()> {
    let p = dl.path_of(id).ok_or_else(|| "download not found".to_string())?;
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
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
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
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
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

#[derive(Default)]
struct SchedState {
    last_minute: Option<(u32, u32, u32)>,
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
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
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
        })
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let default_dir = app
                .path()
                .download_dir()
                .unwrap_or_else(|_| data_dir.join("Downloads"));
            let manager = Downloads::new(data_dir, default_dir)?;
            app.manage(manager.clone());
            app.manage(server::ApiStatus(AtomicBool::new(false)));

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

            // Ticker: live progress for the UI + the scheduler.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut was_active = false;
                let mut sched = SchedState::default();
                loop {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    let snap = manager.snapshot();
                    let active = snap.iter().any(|v| v.item.status == Status::Downloading);
                    if active || was_active {
                        let _ = handle.emit("downloads", snap.clone());
                    }
                    was_active = active;

                    // --- scheduler ---
                    let now = Local::now();
                    let key = (now.ordinal(), now.hour(), now.minute());
                    if sched.last_minute != Some(key) {
                        sched.last_minute = Some(key);
                        let s = manager.settings();
                        if s.schedule_enabled {
                            let hm = format!("{:02}:{:02}", now.hour(), now.minute());
                            if !s.schedule_start.is_empty() && s.schedule_start == hm {
                                let has_work = snap.iter().any(|v| {
                                    matches!(
                                        v.item.status,
                                        Status::Paused | Status::Error | Status::Queued
                                    )
                                });
                                manager.resume_all();
                                sched.armed = has_work;
                                let _ = handle.emit("downloads", manager.snapshot());
                            }
                            if !s.schedule_stop.is_empty() && s.schedule_stop == hm {
                                manager.pause_all();
                                sched.armed = false;
                                let _ = handle.emit("downloads", manager.snapshot());
                            }
                        }
                    }

                    let busy = snap
                        .iter()
                        .any(|v| matches!(v.item.status, Status::Downloading | Status::Queued));
                    if sched.was_busy && !busy && sched.armed {
                        sched.armed = false;
                        let s = manager.settings();
                        if s.schedule_enabled && s.after_finish != "none" {
                            power_action(&handle, &s.after_finish);
                        }
                    }
                    sched.was_busy = busy;
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
            cancel_shutdown,
            open_file,
            open_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running RDM");
}
