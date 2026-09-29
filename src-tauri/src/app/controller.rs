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
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::adb::{Adb, Device};
use crate::pipeline::{Pipeline, PipelineConfig, Status};
use crate::preview::PreviewSlot;
use crate::scrcpy::cameras::{self, CameraInfo, CaptureMode, FpsOption};
use crate::scrcpy::protocol::ControlMessage;
use crate::scrcpy::{server, zoom};
use crate::settings::{Settings, VCAM_SIZES};
use crate::vcam::VirtualCamera;
use super::i18n;
use crate::{platform, portable, resources};

const DEVICE_POLL: Duration = Duration::from_millis(1500);
/// After this long without zoom messages, the zoom the phone reported is trusted over the
/// steps counted here (a message sent before the camera was ready is dropped by the server).
const ZOOM_RESYNC: Duration = Duration::from_secs(1);

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
    /// Resolutions the lens captures as they are (all of them while its list is unknown).
    pub resolutions: Vec<(u32, u32)>,
    /// Frame rates offered at the chosen resolution.
    pub fps_options: Vec<FpsOption>,
    /// What the phone is asked for with the current settings, once its lens list is known.
    pub capture: Option<CaptureMode>,
    /// At the chosen resolution, 60 fps on this lens comes only from a high-speed session,
    /// behind a setting.
    pub high_speed_offered: bool,
    pub torch: bool,
    pub fps: f32,
    /// Camera zoom as the phone last reported it.
    pub zoom: f32,
    /// Zoom range of the running lens, when known.
    pub zoom_range: Option<(f32, f32)>,
    /// Something that stops the app from working at all (missing adb, camera not installed…).
    pub problem: Option<String>,
    /// One-off information, e.g. a lens that was hidden.
    pub notice: Option<Notice>,
    /// Running from a portable folder rather than installed.
    pub portable: bool,
    /// Portable only: "Plugcam Camera" still has to be added to Windows (needs admin once).
    pub camera_missing: bool,
    pub mica: bool,
    /// The UI language in use: the setting, else the one matching Windows.
    pub language: String,
    /// What "same as Windows" means on this PC.
    pub system_language: String,
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
    pub zoom_max: Option<f32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Notice {
    LensHidden { id: String },
    /// The camera could not be turned on; pressing the button again may work.
    StartFailed { message: String },
}

/// What a pipeline reports, tagged with its generation.
enum Event {
    Status(Status),
    Zoom(f32),
}

/// Per-phone facts fetched once per connection.
#[derive(Debug, Clone)]
struct PhoneInfo {
    name: Option<String>,
    /// e.g. `15`.
    android: Option<String>,
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
    /// Reported by the phone; also passed to the next pipeline so a restart keeps it.
    zoom: f32,
    /// The step counted after the last zoom messages, and when they were sent.
    zoom_sent: Option<(i32, Instant)>,
    zoom_range: Option<(f32, f32)>,
    problem: Option<String>,
    camera_missing: bool,
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
    status_tx: Sender<(u64, Event)>,
    mica: bool,
    /// Windows locale, e.g. `ru-RU`.
    system_locale: String,
}

impl Controller {
    pub fn new(app: AppHandle, settings_path: PathBuf) -> Arc<Self> {
        let settings = Settings::load(&settings_path);
        let adb = Adb::locate().map_err(|e| log::error!("{e}")).ok();
        let server_file = server::server_file().map_err(|e| e.to_string());
        let registered = platform::vcam_registered_path().is_some();
        let problem = if adb.is_none() {
            Some("adb.exe not found".to_string())
        } else if let Err(e) = &server_file {
            Some(e.clone())
        } else if !registered && !portable::is_portable() {
            Some("Plugcam Camera is not installed (camera DLL not registered)".to_string())
        } else {
            None
        };
        let (status_tx, status_rx) = mpsc::channel::<(u64, Event)>();

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
                zoom: 1.0,
                zoom_sent: None,
                zoom_range: None,
                problem,
                camera_missing: !registered && portable::is_portable(),
                notice: None,
                last_sent: None,
            }),
            preview: Arc::new(PreviewSlot::default()),
            adb,
            server_file,
            settings_path,
            status_tx,
            mica: platform::windows_build() >= 22000,
            system_locale: sys_locale::get_locale().unwrap_or_default(),
        });

        let c = this.clone();
        std::thread::Builder::new()
            .name("status".into())
            .spawn(move || {
                for (generation, event) in status_rx {
                    match event {
                        Event::Status(status) => c.on_pipeline_status(generation, status),
                        Event::Zoom(z) => c.on_zoom(generation, z),
                    }
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

    /// The phone and camera part of a bug report (see `super::report`).
    pub fn report_facts(&self) -> String {
        let st = self.state.lock().unwrap();
        let mut out = String::new();
        if st.devices.is_empty() {
            out += "Phone: none connected\n";
        }
        for d in &st.devices {
            let model = d.model.clone().unwrap_or_default();
            let phone = st.phones.get(&d.serial);
            out += &format!(
                "Phone: {} ({model}), Android {}, {}, adb state `{}`\n",
                phone.and_then(|p| p.name.as_deref()).unwrap_or("?"),
                phone.and_then(|p| p.android.as_deref()).unwrap_or("?"),
                if d.is_wifi() { "Wi-Fi" } else { "USB" },
                d.state,
            );
            for c in phone.map(|p| p.cameras.as_slice()).unwrap_or_default() {
                out += &format!(
                    "  lens {} {}: {:.0} MP, fps {:?}, zoom {:?}, {} sizes, {} high-speed modes{}\n",
                    c.id,
                    c.facing,
                    c.megapixels(),
                    c.fps,
                    c.zoom,
                    c.sizes.len(),
                    c.high_speed.len(),
                    if st.settings.is_broken(&model, &c.id) { ", hidden: sent no picture" } else { "" },
                );
            }
        }
        out += &format!(
            "Camera: {}, lens {:?}, status {:?}, {:.1} fps\n",
            if st.camera_on { "on" } else { "off" },
            st.running_camera,
            st.status,
            st.fps
        );
        if let Some(problem) = &st.problem {
            out += &format!("Problem: {problem}\n");
        }
        if let Some(notice) = &st.notice {
            out += &format!("Notice: {notice:?}\n");
        }
        out += &format!("Settings: {}\n", serde_json::to_string_pretty(&st.settings).unwrap_or_default());
        out
    }

    /// Phone name and Android version for the new-issue form, when a phone is connected.
    pub fn phone_summary(&self) -> Option<String> {
        let st = self.state.lock().unwrap();
        let d = active_device(&st.devices)?;
        let phone = st.phones.get(&d.serial);
        let name = phone.and_then(|p| p.name.clone()).or_else(|| d.model.clone())?;
        Some(match phone.and_then(|p| p.android.as_deref()) {
            Some(v) => format!("{name}, Android {v}"),
            None => name,
        })
    }

    fn build_snapshot(&self, st: &State) -> Snapshot {
        let device = active_device(&st.devices).map(|d| DeviceView {
            serial: d.serial.clone(),
            model: d.model.clone().unwrap_or_else(|| d.serial.clone()),
            name: st.phones.get(&d.serial).and_then(|p| p.name.clone()),
            wifi: d.is_wifi(),
            state: d.state.clone(),
        });
        let usable = usable_cameras(st);
        let selected = selected_camera(&st.settings, &usable);
        let s = &st.settings;
        let (resolutions, fps_options) = match selected {
            Some(cam) => (
                cameras::resolutions(cam, &VCAM_SIZES),
                cameras::fps_options(cam, (s.vcam_width, s.vcam_height), s.allow_high_speed),
            ),
            None => (
                VCAM_SIZES.to_vec(),
                [30, 60].map(|fps| FpsOption { value: fps, fps, high_speed: false }).to_vec(),
            ),
        };
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
                    zoom_max: c.zoom.map(|z| z.1),
                })
                .collect(),
            selected_camera: selected.map(|c| c.id.clone()),
            resolutions,
            fps_options,
            capture: selected.and_then(|c| capture_mode(s, c)),
            high_speed_offered: selected.is_some_and(|c| cameras::sixty_needs_high_speed(c, (s.vcam_width, s.vcam_height))),
            torch: st.torch,
            fps: st.fps,
            zoom: st.zoom,
            zoom_range: st.zoom_range,
            problem: st.problem.clone(),
            notice: st.notice.clone(),
            portable: portable::is_portable(),
            camera_missing: st.camera_missing,
            mica: self.mica,
            language: i18n::resolve(st.settings.language.as_deref(), &self.system_locale).into(),
            system_language: i18n::resolve(None, &self.system_locale).into(),
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
            if st.camera_on == on || (on && (st.problem.is_some() || st.camera_missing)) {
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
            st.zoom = 1.0;
            st.zoom_sent = None;
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
        let usable = usable_cameras(st);
        let selected = selected_camera(&st.settings, &usable);
        let zoom_range = selected.and_then(|c| c.zoom);
        // Without the lens list the server picks the lens by facing, at a size every phone has.
        params.camera_id = selected.map(|c| c.id.clone());
        match selected.and_then(|c| capture_mode(&st.settings, c)) {
            Some(m) => (params.size, params.fps, params.high_speed) = (Some(m.size), m.fps, m.high_speed),
            None => {
                let size = if st.settings.vcam_height <= 720 { (1280, 720) } else { (1920, 1080) };
                (params.size, params.fps) = (Some(size), 30);
            }
        }
        if st.running_camera != params.camera_id {
            st.zoom = 1.0; // another lens starts at its own 1x
        }
        st.running_camera = params.camera_id.clone();
        st.zoom_range = zoom_range;
        st.zoom_sent = None;
        params.zoom = (st.zoom != 1.0).then_some(st.zoom);

        st.generation += 1;
        let generation = st.generation;
        let tx = self.status_tx.clone();
        let zoom_tx = self.status_tx.clone();
        let config = PipelineConfig {
            adb,
            server_file,
            serial: None,
            camera: params,
            mirror: st.settings.mirror,
            rotation: st.settings.rotation,
            color: st.settings.color,
            dump: None,
            preview: Some(self.preview.clone()),
            on_zoom: Some(Arc::new(move |z| {
                let _ = zoom_tx.send((generation, Event::Zoom(z)));
            })),
        };
        let vcam = st.vcam.take().expect("vcam created above");
        st.status = Some(Status::WaitingForDevice);
        st.pipeline = Some(Pipeline::start(config, vcam, move |s| {
            let _ = tx.send((generation, Event::Status(s.clone())));
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

    fn on_zoom(&self, generation: u64, z: f32) {
        let mut st = self.state.lock().unwrap();
        if generation == st.generation && st.camera_on {
            st.zoom = z;
            self.publish(&mut st);
        }
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
            p.set_color(new.color);
        }
        if old.launch_at_login != new.launch_at_login {
            super::set_launch_at_login(&self.app, new.launch_at_login);
        }
        if old.detailed_log != new.detailed_log {
            crate::diag::set_detailed(new.detailed_log);
        }
        let usable = usable_cameras(&st);
        let mode = |s: &Settings| selected_camera(s, &usable).and_then(|c| capture_mode(s, c));
        let needs_restart = old.facing != new.facing
            || old.camera_id != new.camera_id
            || mode(&old) != mode(&new)
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

    /// One zoom step in or out.
    pub fn zoom(&self, zoom_in: bool) -> Result<(), String> {
        self.zoom_by(|step| if zoom_in { step + 1 } else { step - 1 })
    }

    /// Zooms to `target` (e.g. 2.0) by sending as many steps as the phone needs to get there.
    pub fn set_zoom(&self, target: f32) -> Result<(), String> {
        self.zoom_by(|_| zoom::step_of(target))
    }

    fn zoom_by(&self, target_step: impl FnOnce(i32) -> i32) -> Result<(), String> {
        let mut st = self.state.lock().unwrap();
        let current = match st.zoom_sent {
            Some((step, at)) if at.elapsed() < ZOOM_RESYNC => step,
            _ => zoom::step_of(st.zoom),
        };
        let (lo, hi) = st.zoom_range.map(zoom::step_range).unwrap_or((i32::MIN, i32::MAX));
        let target = target_step(current).clamp(lo, hi);
        if target == current {
            return Ok(());
        }
        let pipeline = st.pipeline.as_ref().ok_or("camera is off")?;
        let msg = if target > current { ControlMessage::ZoomIn } else { ControlMessage::ZoomOut };
        for _ in 0..(target - current).abs() {
            pipeline.send_control(msg).map_err(|e| e.to_string())?;
        }
        st.zoom_sent = Some((target, Instant::now()));
        Ok(())
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

    /// Portable only: adds "Plugcam Camera" to Windows or removes it, after Windows' consent
    /// prompt. The camera is turned off before removing.
    pub fn set_camera_registered(&self, on: bool) -> Result<(), String> {
        if !portable::is_portable() {
            return Err("the installer manages the camera".into());
        }
        if !on {
            self.set_camera_on(false);
        }
        let result = portable::set_camera_registered(on);
        let mut st = self.state.lock().unwrap();
        st.camera_missing = platform::vcam_registered_path().is_none();
        self.publish(&mut st);
        result
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
            for d in &devices {
                crate::diag::hide_serial(&d.serial);
            }
            let mut st = self.state.lock().unwrap();
            if devices != st.devices {
                let list: Vec<String> = devices.iter().map(|d| format!("{} {}", d.serial, d.state)).collect();
                log::info!("phones: {}", if list.is_empty() { "none".into() } else { list.join(", ") });
            }
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
        let android = adb.shell(serial, "getprop ro.build.version.release").ok().map(|s| s.trim().to_string());
        let cameras = match &self.server_file {
            Ok(file) => server::list_cameras(adb, serial, file).map(|out| cameras::parse_camera_list(&out)).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        log::info!("{serial}: {name:?}, Android {}, {} cameras", android.as_deref().unwrap_or("?"), cameras.len());
        let mut st = self.state.lock().unwrap();
        st.fetching.retain(|s| s != serial);
        // An empty list means the camera service is busy (docs/stage2.md); try again later.
        if !cameras.is_empty() {
            st.phones.insert(serial.to_string(), PhoneInfo { name, android, cameras });
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

/// Lenses of the active phone, without the ones that turned out broken.
fn usable_cameras(st: &State) -> Vec<&CameraInfo> {
    let Some(device) = active_device(&st.devices) else { return Vec::new() };
    let model = device.model.as_deref().unwrap_or_default();
    st.phones
        .get(&device.serial)
        .map(|p| p.cameras.iter().filter(|c| !st.settings.is_broken(model, &c.id)).collect())
        .unwrap_or_default()
}

fn capture_mode(settings: &Settings, cam: &CameraInfo) -> Option<CaptureMode> {
    let size = (settings.vcam_width, settings.vcam_height);
    cameras::capture_mode(cam, size, settings.fps, settings.allow_high_speed)
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
