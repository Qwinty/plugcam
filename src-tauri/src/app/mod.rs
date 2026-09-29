//! The Tauri app: window, tray, commands for the UI.

mod controller;
pub mod i18n;
mod report;
mod tray;
mod updates;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, RunEvent, State, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

pub use controller::{Controller, Snapshot};

use crate::preview::{self, PreviewEncoder};

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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(PreviewChannel::default())
        .manage(updates::Updates::default())
        .setup(|app| {
            let portable = crate::portable::is_portable();
            let dir = if portable { crate::portable::data_dir() } else { app.path().app_config_dir()? };
            let log_dir = if portable { dir.join("logs") } else { app.path().app_log_dir()? };
            let settings_path = dir.join("settings.json");
            let controller = Controller::new(app.handle().clone(), settings_path);
            crate::diag::configure(&log_dir, controller.snapshot().settings.detailed_log);
            log::info!(
                "Plugcam {} ({}), {}",
                env!("CARGO_PKG_VERSION"),
                if portable { "portable" } else { "installed" },
                crate::platform::windows_name()
            );
            app.manage(controller.clone());
            tray::create(app.handle(), &controller.snapshot())?;
            start_preview_thread(app.handle().clone(), controller);
            updates::start(app.handle().clone());

            if !std::env::args().any(|a| a == MINIMIZED_ARG) {
                show_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing to the tray destroys the window rather than hiding it: its WebView2 processes
            // hold about 200 MB that nothing needs while the camera works in the background.
            WindowEvent::CloseRequested { api, .. } => {
                let app = window.app_handle();
                let controller = app.state::<Arc<Controller>>();
                if controller.snapshot().settings.close_to_tray {
                    controller.preview.set_enabled(false);
                } else {
                    api.prevent_close();
                    quit(app);
                }
            }
            // WebView2 does not report minimizing to the page, so stop the preview from here.
            WindowEvent::Resized(_) => {
                let visible = window.is_visible().unwrap_or(true) && !window.is_minimized().unwrap_or(false);
                window.app_handle().state::<Arc<Controller>>().preview.set_enabled(visible);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            set_camera,
            update_settings,
            set_torch,
            zoom,
            set_zoom,
            dismiss_notice,
            subscribe_preview,
            set_preview_active,
            set_preview_width,
            open_url,
            set_camera_registered,
            select_phone,
            wifi_qr_start,
            wifi_qr_wait,
            wifi_qr_cancel,
            wifi_pair_code,
            wifi_connect,
            wifi_from_cable,
            wifi_forget,
            save_report,
            open_log_folder,
            updates::update_state,
            updates::check_for_updates,
            updates::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while running Plugcam")
        .run(|_, event| {
            // The last window closing leaves the app in the tray; only `quit` ends it.
            if let RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
            }
        });
}

/// Brings the window forward, creating it (from tauri.conf.json) if it was closed to the tray.
pub fn show_window(app: &AppHandle) {
    let window = app.get_webview_window("main").or_else(|| {
        let config = app.config().app.windows.iter().find(|w| w.label == "main")?;
        WebviewWindowBuilder::from_config(app, config)
            .and_then(|b| b.build())
            .map_err(|e| log::error!("creating the window: {e}"))
            .ok()
    });
    if let Some(w) = window {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        app.state::<Arc<Controller>>().preview.set_enabled(true);
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
            let mut encoder = PreviewEncoder::new(preview::MAX_WIDTH);
            let mut buf = Vec::new();
            loop {
                let Some((w, h)) = controller.preview.take(&mut buf, Duration::from_millis(500)) else { continue };
                encoder.set_max_width(controller.preview.width());
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
fn set_zoom(c: Ctl, value: f32) -> Result<(), String> {
    c.set_zoom(value)
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

/// How wide the window shows the preview, in physical pixels.
#[tauri::command]
fn set_preview_width(c: Ctl, width: u32) {
    c.preview.set_width(width);
}

/// Portable only: adds "Plugcam Camera" to Windows (or removes it) after the consent prompt.
#[tauri::command]
async fn set_camera_registered(c: Ctl<'_>, on: bool) -> Result<(), String> {
    let c = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.set_camera_registered(on)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn select_phone(c: Ctl<'_>, serial: Option<String>) -> Result<(), String> {
    let c = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.select_phone(serial)).await.map_err(|e| e.to_string())
}

/// A new QR code for Wi-Fi pairing, as SVG.
#[tauri::command]
fn wifi_qr_start(c: Ctl) -> String {
    c.wifi_qr_start()
}

/// Waits for the phone to scan the QR code (minutes), then pairs; returns the phone's serial.
#[tauri::command]
async fn wifi_qr_wait(c: Ctl<'_>) -> Result<String, String> {
    let c = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.wifi_qr_wait()).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn wifi_qr_cancel(c: Ctl) {
    c.wifi_qr_cancel()
}

#[tauri::command]
async fn wifi_pair_code(c: Ctl<'_>, code: String, address: Option<String>) -> Result<String, String> {
    let c = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.wifi_pair_code(&code, address.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn wifi_connect(c: Ctl<'_>, address: String) -> Result<String, String> {
    let c = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.wifi_connect(&address)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn wifi_from_cable(c: Ctl<'_>) -> Result<String, String> {
    let c = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.wifi_from_cable()).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn wifi_forget(c: Ctl<'_>, id: String) -> Result<(), String> {
    let c = c.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.wifi_forget(&id)).await.map_err(|e| e.to_string())
}

/// Opens help links (driver download, docs) in the default browser; https only.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https links".into());
    }
    std::process::Command::new("explorer").arg(&url).spawn().map(drop).map_err(|e| e.to_string())
}

/// "Save report…": writes the report to Downloads, shows it in Explorer, opens the issue form.
#[tauri::command]
async fn save_report(c: Ctl<'_>) -> Result<String, String> {
    let c = c.inner().clone();
    let path = tauri::async_runtime::spawn_blocking(move || report::create(&c)).await.map_err(|e| e.to_string())??;
    Ok(path.display().to_string())
}

#[tauri::command]
fn open_log_folder() -> Result<(), String> {
    report::open_log_folder()
}
