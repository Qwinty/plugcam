//! App state behind the window and the tray: settings, phones, the running pipeline.
//!
//! Locking rule: never stop a pipeline while holding `state`. The pipeline thread reports
//! through a channel that the status thread applies, so it never waits on `state` itself.
//! Starting and stopping go through `lifecycle` (taken before `state`), so two of them never
//! overlap: otherwise a second start could try to create the camera while the first pipeline
//! still owns it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::adb::{Adb, Device};
use crate::pipeline::{Pipeline, PipelineConfig, Status};
use crate::preview::PreviewSlot;
use crate::scrcpy::cameras::{self, CameraInfo, Quality};
use crate::scrcpy::protocol::ControlMessage;
use crate::scrcpy::server;
use crate::settings::Settings;
use crate::vcam::VirtualCamera;
use crate::{platform, resources};

const DEVICE_POLL: Duration = Duration::from_millis(1500);

/// What the window needs to draw itself. Sent whole on every change (it is small).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub settings: Settings,
    pub camera_on: bool,
    pub status: Option<Status>,
    pub device: Option<DeviceView>,
    /// Usable lenses of the active phone (broken ones left out).
    pub cameras: Vec<CameraView>,
    /// The lens that is (or would be) used, if the phone's list is known.
    pub selected_camera: Option<String>,
    pub smooth_available: bool,
    pub torch: bool,
    pub fps: f32,
    /// Something that stops the app from working at all (missing adb, camera not installed…).
    pub problem: Option<String>,
    /// One-off information, e.g. a lens that was hidden.
    pub notice: Option<Notice>,
    pub mica: bool,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    pub serial: String,
    /// adb model, e.g. `PHK110`; also the key for broken lenses.
    pub model: String,
    /// Human name, e.g. `OnePlus PHK110`, once known.
    pub name: Option<String>,
    pub wifi: bool,
    /// `device`, `unauthorized`, `offline`…
    pub state: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CameraView {
    pub id: String,
    pub facing: String,
    pub megapixels: f32,
    pub zoom_min: Option<f32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Notice {
    LensHidden { id: String },
    /// The camera could not be turned on; pressing the button again may work.
    StartFailed { message: String },
}

/// Per-phone facts fetched once per connection.
#[derive(Debug, Clone)]
struct PhoneInfo {
    name: Option<String>,
    cameras: Vec<CameraInfo>,
}

struct State {
    settings: Settings,
    camera_on: bool,
    pipeline: Option<Pipeline>,
    /// Kept between pipelines while the camera is on, so apps never see it vanish.
    vcam: Option<VirtualCamera>,
    /// Bumped on every pipeline start; statuses from older pipelines are ignored.
    generation: u64,
    running_camera: Option<String>,
    status: Option<Status>,
    devices: Vec<Device>,
    phones: HashMap<String, PhoneInfo>,
    fetching: Vec<String>,
    torch: bool,
    fps: f32,
    problem: Option<String>,
    notice: Option<Notice>,
    last_sent: Option<Snapshot>,
}

pub struct Controller {
    app: AppHandle,
    lifecycle: Mutex<()>,
    state: Mutex<State>,
    pub preview: Arc<PreviewSlot>,
    adb: Option<Adb>,
    server_file: Result<PathBuf, String>,
    settings_path: PathBuf,
    status_tx: Sender<(u64, Status)>,
    mica: bool,
    language: String,
}

impl Controller {
    pub fn new(app: AppHandle, settings_path: PathBuf) -> Arc<Self> {
        let settings = Settings::load(&settings_path);
        let adb = Adb::locate().map_err(|e| log::error!("{e}")).ok();
        let server_file = server::server_file().map_err(|e| e.to_string());
        let problem = if adb.is_none() {
            Some("adb.exe not found".to_string())
        } else if let Err(e) = &server_file {
            Some(e.clone())
        } else if platform::vcam_registered_path().is_none() {
            Some("Plugcam Camera is not installed (camera DLL not registered)".to_string())
        } else {
            None
        };
        let language = sys_locale::get_locale().unwrap_or_default();
        let (status_tx, status_rx) = mpsc::channel::<(u64, Status)>();

        let this = Arc::new(Self {
            app,
            lifecycle: Mutex::new(()),
            state: Mutex::new(State {
                settings,
                camera_on: false,
                pipeline: None,
                vcam: None,
                generation: 0,
                running_camera: None,
                status: None,
                devices: Vec::new(),
                phones: HashMap::new(),
                fetching: Vec::new(),
                torch: false,
                fps: 0.0,
                problem,
                notice: None,
                last_sent: None,
            }),
            preview: Arc::new(PreviewSlot::default()),
            adb,
            server_file,
            settings_path,
            status_tx,
            mica: platform::windows_build() >= 22000,
            language: if language.starts_with("ru") { "ru".into() } else { "en".into() },
        });

        let c = this.clone();
        std::thread::Builder::new()
            .name("status".into())
            .spawn(move || {
                for (generation, status) in status_rx {
                    c.on_pipeline_status(generation, status);
                }
            })
            .unwrap();
        let c = this.clone();
        std::thread::Builder::new().name("devices".into()).spawn(move || c.watch_devices()).unwrap();
        this
    }

    pub fn snapshot(&self) -> Snapshot {
        let st = self.state.lock().unwrap();
        self.build_snapshot(&st)
    }

    fn build_snapshot(&self, st: &State) -> Snapshot {
        let device = active_device(&st.devices).map(|d| DeviceView {
            serial: d.serial.clone(),
            model: d.model.clone().unwrap_or_else(|| d.serial.clone()),
            name: st.phones.get(&d.serial).and_then(|p| p.name.clone()),
            wifi: d.is_wifi(),
            state: d.state.clone(),
        });
        let model = device.as_ref().map(|d| d.model.clone()).unwrap_or_default();
        let phone = device.as_ref().and_then(|d| st.phones.get(&d.serial));
        let usable: Vec<&CameraInfo> = phone
            .map(|p| p.cameras.iter().filter(|c| !st.settings.is_broken(&model, &c.id)).collect())
            .unwrap_or_default();
        let selected = selected_camera(&st.settings, &usable);
        Snapshot {
            settings: st.settings.clone(),
            camera_on: st.camera_on,
            status: st.status.clone(),
            device,
            cameras: usable
                .iter()
                .map(|c| CameraView {
                    id: c.id.clone(),
                    facing: c.facing.clone(),
                    megapixels: c.megapixels(),
                    zoom_min: c.zoom.map(|z| z.0),
                })
                .collect(),
            selected_camera: selected.map(|c| c.id.clone()),
            smooth_available: selected.is_some_and(|c| cameras::capture_mode(c, Quality::Smooth).is_some()),
            torch: st.torch,
            fps: st.fps,
            problem: st.problem.clone(),
            notice: st.notice.clone(),
            mica: self.mica,
            language: self.language.clone(),
        }
    }

    /// Sends the snapshot to the window and updates the tray, if anything changed.
    fn publish(&self, st: &mut State) {
        let snap = self.build_snapshot(st);
        if st.last_sent.as_ref() == Some(&snap) {
            return;
        }
        let _ = self.app.emit("state", &snap);
        super::tray::update(&self.app, &snap);
        st.last_sent = Some(snap);
    }

    pub fn set_camera_on(&self, on: bool) {
        let _lifecycle = self.lifecycle.lock().unwrap();
        let old = {
            let mut st = self.state.lock().unwrap();
            if st.camera_on == on || (on && st.problem.is_some()) {
                return;
            }
            st.camera_on = on;
            st.notice = None;
            st.pipeline.take()
        };
        let vcam = old.and_then(Pipeline::stop);
        let mut st = self.state.lock().unwrap();
        st.torch = false;
        if on {
            st.vcam = vcam;
            self.start_pipeline(&mut st);
        } else {
            // Dropping the camera makes it disappear from the apps; that is what "off" means.
            st.vcam = None;
            st.status = None;
            st.fps = 0.0;
            st.running_camera = None;
            self.preview_clear();
        }
        self.publish(&mut st);
    }

    /// Stops the running pipeline (if any) and starts a new one with the current settings.
    fn restart(&self) {
        let _lifecycle = self.lifecycle.lock().unwrap();
        let old = self.state.lock().unwrap().pipeline.take();
        let vcam = old.and_then(Pipeline::stop);
        let mut st = self.state.lock().unwrap();
        if vcam.is_some() {
            st.vcam = vcam;
        }
        st.torch = false;
        if st.camera_on {
            self.start_pipeline(&mut st);
        }
        self.publish(&mut st);
    }

    fn start_pipeline(&self, st: &mut State) {
        let (Some(adb), Ok(server_file)) = (self.adb.clone(), self.server_file.clone()) else { return };

        let size = (st.settings.vcam_width, st.settings.vcam_height);
        if st.vcam.as_ref().is_some_and(|v| (v.width(), v.height()) != size) {
            st.vcam = None; // size changed: apps will reopen the device
        }
        if st.vcam.is_none() {
            let created = resources::vcam_dll()
                .ok_or_else(|| "plugcam_cam.dll not found".to_string())
                .and_then(|dll| VirtualCamera::create(&dll, size.0, size.1).map_err(|e| e.to_string()));
            match created {
                Ok(v) => st.vcam = Some(v),
                Err(message) => {
                    log::error!("creating the camera: {message}");
                    st.notice = Some(Notice::StartFailed { message });
                    st.camera_on = false;
                    st.status = None;
                    return;
                }
            }
        }

        let mut params = st.settings.camera_params();
        let device = active_device(&st.devices).cloned();
        let model = device.as_ref().and_then(|d| d.model.clone()).unwrap_or_default();
        let phone = device.as_ref().and_then(|d| st.phones.get(&d.serial));
        let usable: Vec<&CameraInfo> = phone
            .map(|p| p.cameras.iter().filter(|c| !st.settings.is_broken(&model, &c.id)).collect())
            .unwrap_or_default();
        let mode = match selected_camera(&st.settings, &usable) {
            Some(cam) => {
                params.camera_id = Some(cam.id.clone());
                cameras::capture_mode(cam, st.settings.quality)
                    .or_else(|| cameras::capture_mode(cam, Quality::Standard))
            }
            None => None, // list not known yet: let the server pick by facing
        };
        match mode {
            Some(m) => (params.size, params.fps, params.high_speed) = (Some(m.size), m.fps, m.high_speed),
            None => {
                let (w, h) = if st.settings.quality == Quality::Economy { (1280, 720) } else { (1920, 1080) };
                (params.size, params.fps) = (Some((w, h)), 30);
            }
        }
        st.running_camera = params.camera_id.clone();

        st.generation += 1;
        let generation = st.generation;
        let tx = self.status_tx.clone();
        let config = PipelineConfig {
            adb,
            server_file,
            serial: None,
            camera: params,
            mirror: st.settings.mirror,
            rotation: st.settings.rotation,
            dump: None,
            preview: Some(self.preview.clone()),
        };
        let vcam = st.vcam.take().expect("vcam created above");
        st.status = Some(Status::WaitingForDevice);
        st.pipeline = Some(Pipeline::start(config, vcam, move |s| {
            let _ = tx.send((generation, s.clone()));
        }));
    }

    fn on_pipeline_status(self: &Arc<Self>, generation: u64, status: Status) {
        let mut st = self.state.lock().unwrap();
        if generation != st.generation || !st.camera_on {
            return;
        }
        if let Status::NoPicture { serial } = &status {
            let model = st.devices.iter().find(|d| &d.serial == serial).and_then(|d| d.model.clone());
            if let (Some(model), Some(id)) = (model, st.running_camera.clone()) {
                log::warn!("camera {id} on {model} sends no picture, hiding it");
                st.settings.mark_broken(&model, &id);
                st.settings.camera_id = None;
                self.save(&st.settings);
                st.notice = Some(Notice::LensHidden { id });
                st.status = Some(status);
                self.publish(&mut st);
                drop(st);
                let c = self.clone();
                std::thread::spawn(move || c.restart());
                return;
            }
        }
        if !matches!(status, Status::Streaming { .. }) {
            st.fps = 0.0;
        }
        st.status = Some(status);
        self.publish(&mut st);
    }

    /// Applies a partial settings object from the window (camelCase keys).
    pub fn update_settings(&self, patch: serde_json::Value) -> Result<(), String> {
        let mut st = self.state.lock().unwrap();
        let mut value = serde_json::to_value(&st.settings).map_err(|e| e.to_string())?;
        let (Some(obj), Some(patch)) = (value.as_object_mut(), patch.as_object()) else {
            return Err("settings patch must be an object".into());
        };
        for (k, v) in patch {
            obj.insert(k.clone(), v.clone());
        }
        let new: Settings = serde_json::from_value::<Settings>(value).map_err(|e| e.to_string())?.sanitized();
        let old = std::mem::replace(&mut st.settings, new.clone());
        self.save(&new);

        if let Some(p) = &st.pipeline {
            p.set_mirror(new.mirror);
            p.set_rotation(new.rotation);
        }
        if old.launch_at_login != new.launch_at_login {
            super::set_launch_at_login(&self.app, new.launch_at_login);
        }
        let needs_restart = old.facing != new.facing
            || old.camera_id != new.camera_id
            || old.quality != new.quality
            || old.bitrate() != new.bitrate()
            || (old.vcam_width, old.vcam_height) != (new.vcam_width, new.vcam_height);
        st.notice = None;
        self.publish(&mut st);
        let restart = needs_restart && st.camera_on;
        drop(st);
        if restart {
            self.restart();
        }
        Ok(())
    }

    pub fn set_torch(&self, on: bool) -> Result<(), String> {
        let mut st = self.state.lock().unwrap();
        let pipeline = st.pipeline.as_ref().ok_or("camera is off")?;
        pipeline.send_control(ControlMessage::SetTorch(on)).map_err(|e| e.to_string())?;
        st.torch = on;
        self.publish(&mut st);
        Ok(())
    }

    pub fn zoom(&self, zoom_in: bool) -> Result<(), String> {
        let st = self.state.lock().unwrap();
        let pipeline = st.pipeline.as_ref().ok_or("camera is off")?;
        let msg = if zoom_in { ControlMessage::ZoomIn } else { ControlMessage::ZoomOut };
        pipeline.send_control(msg).map_err(|e| e.to_string())
    }

    pub fn dismiss_notice(&self) {
        let mut st = self.state.lock().unwrap();
        st.notice = None;
        self.publish(&mut st);
    }

    pub fn is_camera_on(&self) -> bool {
        self.state.lock().unwrap().camera_on
    }

    /// Before the app exits: stop streaming and remove the camera.
    pub fn shutdown(&self) {
        let _lifecycle = self.lifecycle.lock().unwrap();
        let old = self.state.lock().unwrap().pipeline.take();
        drop(old.and_then(Pipeline::stop));
        let mut st = self.state.lock().unwrap();
        st.vcam = None;
        st.camera_on = false;
    }

    fn save(&self, settings: &Settings) {
        if let Err(e) = settings.save(&self.settings_path) {
            log::error!("saving settings: {e}");
        }
    }

    fn preview_clear(&self) {
        let _ = self.app.emit("preview-clear", ());
    }

    /// Polls adb for phones, fetches each new phone's name and cameras, and turns the camera
    /// on when a phone appears if the user asked for that.
    fn watch_devices(self: Arc<Self>) {
        let Some(adb) = self.adb.clone() else { return };
        loop {
            let devices = adb.devices().unwrap_or_default();
            let mut st = self.state.lock().unwrap();
            let was_ready = active_device(&st.devices).is_some_and(Device::is_ready);
            st.phones.retain(|serial, _| devices.iter().any(|d| &d.serial == serial && d.is_ready()));
            st.devices = devices;
            let now_ready = active_device(&st.devices).filter(|d| d.is_ready()).cloned();

            if let Some(d) = &now_ready {
                if !st.phones.contains_key(&d.serial) && !st.fetching.contains(&d.serial) {
                    st.fetching.push(d.serial.clone());
                    let (c, adb, serial) = (self.clone(), adb.clone(), d.serial.clone());
                    std::thread::spawn(move || c.fetch_phone(&adb, &serial));
                }
            }
            if let Some(p) = st.pipeline.as_ref() {
                st.fps = if matches!(st.status, Some(Status::Streaming { .. })) { p.stats().fps } else { 0.0 };
            }
            let auto_start = !was_ready && now_ready.is_some() && st.settings.auto_start_camera && !st.camera_on;
            self.publish(&mut st);
            drop(st);
            if auto_start {
                self.set_camera_on(true);
            }
            std::thread::sleep(DEVICE_POLL);
        }
    }

    fn fetch_phone(&self, adb: &Adb, serial: &str) {
        let name = adb
            .shell(serial, "echo $(getprop ro.product.manufacturer) $(getprop ro.product.marketname) $(getprop ro.product.model)")
            .ok()
            .map(|s| pretty_name(&s));
        let cameras = match &self.server_file {
            Ok(file) => server::list_cameras(adb, serial, file).map(|out| cameras::parse_camera_list(&out)).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        log::info!("{serial}: {name:?}, {} cameras", cameras.len());
        let mut st = self.state.lock().unwrap();
        st.fetching.retain(|s| s != serial);
        // An empty list means the camera service is busy (docs/stage2.md); try again later.
        if !cameras.is_empty() {
            st.phones.insert(serial.to_string(), PhoneInfo { name, cameras });
        }
        self.publish(&mut st);
    }
}

/// The phone the app works with: first ready one (USB first), else any listed one so the
/// window can say "allow USB debugging".
fn active_device(devices: &[Device]) -> Option<&Device> {
    let ready: Vec<&Device> = devices.iter().filter(|d| d.is_ready()).collect();
    ready.iter().find(|d| !d.is_wifi()).or(ready.first()).copied().or(devices.first())
}

fn selected_camera<'a>(settings: &Settings, usable: &[&'a CameraInfo]) -> Option<&'a CameraInfo> {
    let of_facing = |c: &&&CameraInfo| c.facing == settings.facing;
    settings
        .camera_id
        .as_ref()
        .and_then(|id| usable.iter().filter(of_facing).find(|c| &c.id == id))
        .or_else(|| usable.iter().find(of_facing))
        .copied()
}

/// `OnePlus  PHK110` → `OnePlus PHK110`; drops the model when the marketing name has it.
fn pretty_name(raw: &str) -> String {
    let mut words: Vec<&str> = Vec::new();
    for w in raw.split_whitespace() {
        if !words.iter().any(|x| x.eq_ignore_ascii_case(w)) {
            words.push(w);
        }
    }
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(serial: &str, state: &str) -> Device {
        Device { serial: serial.into(), state: state.into(), model: Some("M".into()) }
    }

    #[test]
    fn active_device_prefers_ready_usb() {
        let list = [device("1.2.3.4:5555", "device"), device("abc", "unauthorized"), device("usb1", "device")];
        assert_eq!(active_device(&list).unwrap().serial, "usb1");
        let list = [device("abc", "unauthorized")];
        assert_eq!(active_device(&list).unwrap().serial, "abc");
        assert!(active_device(&[]).is_none());
    }

    #[test]
    fn names() {
        assert_eq!(pretty_name("OnePlus  PHK110\n"), "OnePlus PHK110");
        assert_eq!(pretty_name("Google Pixel 8 Pixel 8"), "Google Pixel 8");
    }
}
