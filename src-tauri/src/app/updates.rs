//! Updates from GitHub Releases: a check at start and once a day (unless turned off), and an
//! install when the user asks. The manifest (`latest.json`) and every file in it are signed with
//! the key whose public half is in tauri.conf.json, so a changed download is refused.
//!
//! The installed app downloads the new installer, which replaces it and starts it again. The
//! portable one downloads the portable zip and swaps its own files (see `crate::portable`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use super::Controller;
use crate::portable;

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(10);
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// Manifest entries: the installer for installed copies, the zip for portable ones.
const TARGET_INSTALLER: &str = "windows-x86_64";
const TARGET_PORTABLE: &str = "windows-x86_64-portable";

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Installing,
    Error,
}

/// What the window shows about updates. Sent as the `update` event on every change.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateView {
    pub phase: Phase,
    /// The newer version, once one was found.
    pub version: Option<String>,
    /// Its release notes (Markdown).
    pub notes: Option<String>,
    /// 0..1 while downloading, when the size is known.
    pub progress: Option<f32>,
    pub error: Option<String>,
    pub portable: bool,
}

#[derive(Default)]
pub struct Updates {
    view: Mutex<UpdateView>,
    pending: Mutex<Option<Update>>,
    /// One check or install at a time.
    busy: AtomicBool,
}

/// Holds `Updates::busy` until dropped.
struct Busy<'a>(&'a AtomicBool);

impl<'a> Busy<'a> {
    fn take(flag: &'a AtomicBool) -> Option<Self> {
        (!flag.swap(true, Ordering::AcqRel)).then_some(Self(flag))
    }
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Updates {
    fn set(&self, app: &AppHandle, f: impl FnOnce(&mut UpdateView)) {
        let mut view = self.view.lock().unwrap();
        f(&mut view);
        view.portable = portable::is_portable();
        let _ = app.emit("update", view.clone());
    }
}

fn updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let mut builder = app
        .updater_builder()
        .target(if portable::is_portable() { TARGET_PORTABLE } else { TARGET_INSTALLER })
        .timeout(Duration::from_secs(30));
    // For testing against a local server; the files must still carry our signature.
    if let Some(url) = std::env::var("PLUGCAM_UPDATE_URL").ok().and_then(|u| u.parse().ok()) {
        builder = builder.endpoints(vec![url]).map_err(|e| e.to_string())?;
    }
    if !portable::is_portable() {
        let app = app.clone();
        // The installer takes over from here: stop the camera and let adb go first.
        builder = builder.on_before_exit(move || app.state::<Arc<Controller>>().shutdown());
    }
    builder.build().map_err(|e| e.to_string())
}

/// Checks the manifest. `manual` checks report "up to date" and errors; automatic ones stay
/// quiet unless they find something.
async fn check(app: &AppHandle, manual: bool) {
    let updates = app.state::<Updates>();
    let Some(_busy) = Busy::take(&updates.busy) else { return };
    if manual {
        updates.set(app, |v| {
            v.phase = Phase::Checking;
            v.error = None;
        });
    }
    let result = match updater(app) {
        Ok(u) => u.check().await.map_err(|e| e.to_string()),
        Err(e) => Err(e),
    };
    match result {
        Ok(Some(update)) => {
            log::info!("update available: {}", update.version);
            let (version, notes) = (update.version.clone(), update.body.clone());
            *updates.pending.lock().unwrap() = Some(update);
            updates.set(app, |v| {
                v.phase = Phase::Available;
                v.version = Some(version);
                v.notes = notes;
                v.error = None;
            });
        }
        Ok(None) => {
            *updates.pending.lock().unwrap() = None;
            updates.set(app, |v| *v = UpdateView { phase: if manual { Phase::UpToDate } else { Phase::Idle }, ..Default::default() });
        }
        Err(e) => {
            log::warn!("update check: {e}");
            if manual {
                updates.set(app, |v| {
                    v.phase = Phase::Error;
                    v.error = Some(e);
                });
            }
        }
    }
}

/// Checks at start and then daily while the setting is on.
pub fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("updates".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK_DELAY);
            loop {
                if app.state::<Arc<Controller>>().snapshot().settings.check_updates {
                    tauri::async_runtime::block_on(check(&app, false));
                }
                std::thread::sleep(CHECK_EVERY);
            }
        })
        .unwrap();
}

#[tauri::command]
pub fn update_state(updates: State<Updates>) -> UpdateView {
    let mut view = updates.view.lock().unwrap().clone();
    view.portable = portable::is_portable();
    view
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) {
    check(&app, true).await
}

/// Downloads the update, checks its signature and installs it. Returns only on failure: on
/// success this process exits and the new version starts.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updates = app.state::<Updates>();
    let _busy = Busy::take(&updates.busy).ok_or("busy")?;
    let Some(update) = updates.pending.lock().unwrap().clone() else { return Err("no update".into()) };
    let result = download_and_install(&app, &update).await;
    if let Err(e) = &result {
        log::error!("update to {}: {e}", update.version);
        updates.set(&app, |v| {
            v.phase = Phase::Error;
            v.progress = None;
            v.error = Some(e.clone());
        });
    }
    result
}

async fn download_and_install(app: &AppHandle, update: &Update) -> Result<(), String> {
    let updates = app.state::<Updates>();
    updates.set(app, |v| {
        v.phase = Phase::Downloading;
        v.progress = Some(0.0);
        v.error = None;
    });
    let mut received = 0usize;
    let mut last_emit = Instant::now();
    let bytes = update
        .download(
            |chunk, total| {
                received += chunk;
                if last_emit.elapsed() > Duration::from_millis(100) {
                    last_emit = Instant::now();
                    let progress = total.map(|t| (received as f64 / t.max(1) as f64) as f32);
                    updates.set(app, |v| v.progress = progress);
                }
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    updates.set(app, |v| {
        v.phase = Phase::Installing;
        v.progress = None;
    });

    let app = app.clone();
    let update = update.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if portable::is_portable() {
            portable::install_update(&bytes)?;
            super::quit(&app);
            Ok(())
        } else {
            // Starts the installer and exits the process.
            update.install(bytes).map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
