//! The Tauri app: window, tray, commands for the UI.

mod controller;
mod tray;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

pub use controller::{Controller, Snapshot};

use crate::preview::PreviewEncoder;

/// Passed by the autostart entry so the app starts in the tray.
const MINIMIZED_ARG: &str = "--minimized";

/// Where preview JPEGs go; replaced when the window reloads.
#[derive(Default)]
struct PreviewChannel(Mutex<Option<Channel<InvokeResponseBody>>>);

pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch only brings the running window forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![MINIMIZED_ARG])))
        .manage(PreviewChannel::default())
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let controller = Controller::new(app.handle().clone(), settings_path);
            app.manage(controller.clone());
            tray::create(app.handle(), &controller.snapshot())?;
            start_preview_thread(app.handle().clone(), controller);

            if !std::env::args().any(|a| a == MINIMIZED_ARG) {
                show_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                let app = window.app_handle();
                let controller = app.state::<Arc<Controller>>();
                if controller.snapshot().settings.close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                    controller.preview.set_enabled(false);
                } else {
                    quit(app);
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            set_camera,
            update_settings,
            set_torch,
            zoom,
            dismiss_notice,
            subscribe_preview,
            set_preview_active,
            open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Plugcam");
}

pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn quit(app: &AppHandle) {
    app.state::<Arc<Controller>>().shutdown();
    app.exit(0);
}

pub(crate) fn set_launch_at_login(app: &AppHandle, on: bool) {
    let autolaunch = app.autolaunch();
    let result = if on { autolaunch.enable() } else { autolaunch.disable() };
    if let Err(e) = result {
        log::error!("launch at login: {e}");
    }
}

/// Scales and encodes preview frames off the pipeline thread.
fn start_preview_thread(app: AppHandle, controller: Arc<Controller>) {
    std::thread::Builder::new()
        .name("preview".into())
        .spawn(move || {
            let mut encoder = PreviewEncoder::new(960);
            let mut buf = Vec::new();
            loop {
                let Some((w, h)) = controller.preview.take(&mut buf, Duration::from_millis(500)) else { continue };
                let jpeg = match encoder.encode(&buf, w, h) {
                    Ok(j) => j,
                    Err(e) => {
                        log::warn!("preview: {e}");
                        continue;
                    }
                };
                let channel = app.state::<PreviewChannel>().0.lock().unwrap().clone();
                if let Some(ch) = channel {
                    if ch.send(InvokeResponseBody::Raw(jpeg)).is_err() {
                        controller.preview.set_enabled(false);
                    }
                }
            }
        })
        .unwrap();
}

type Ctl<'a> = State<'a, Arc<Controller>>;

#[tauri::command]
fn get_state(c: Ctl) -> Snapshot {
    c.snapshot()
}

#[tauri::command]
async fn set_camera(c: Ctl<'_>, on: bool) -> Result<(), String> {
    // Starting/stopping waits for threads and adb; keep it off the UI thread.
    let c = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.set_camera_on(on)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_settings(c: Ctl<'_>, patch: serde_json::Value) -> Result<(), String> {
    let c = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.update_settings(patch)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn set_torch(c: Ctl, on: bool) -> Result<(), String> {
    c.set_torch(on)
}

#[tauri::command]
fn zoom(c: Ctl, zoom_in: bool) -> Result<(), String> {
    c.zoom(zoom_in)
}

#[tauri::command]
fn dismiss_notice(c: Ctl) {
    c.dismiss_notice()
}

#[tauri::command]
fn subscribe_preview(channel: Channel<InvokeResponseBody>, holder: State<PreviewChannel>, c: Ctl) {
    *holder.0.lock().unwrap() = Some(channel);
    c.preview.set_enabled(true);
}

/// The window reports when it is hidden or minimized, so no preview is made for nobody.
#[tauri::command]
fn set_preview_active(c: Ctl, active: bool) {
    c.preview.set_enabled(active);
}

/// Opens help links (driver download, docs) in the default browser; https only.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https links".into());
    }
    std::process::Command::new("explorer").arg(&url).spawn().map(drop).map_err(|e| e.to_string())
}
