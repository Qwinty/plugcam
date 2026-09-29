//! App state behind the window and the tray: settings, phones, the running pipeline.
//!
//! Locking rule: never stop a pipeline while holding `state`. The pipeline thread reports
//! through a channel that the status thread applies, so it never waits on `state` itself.
//! Starting and stopping go through `lifecycle` (taken before `state`), so two of them never
//! overlap: otherwise a second start could try to create the camera while the first pipeline
//! still owns it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::adb::{self, Adb, Device, MdnsKind};
use crate::pipeline::{Pipeline, PipelineConfig, Status};
use crate::preview::PreviewSlot;
use crate::scrcpy::cameras::{self, CameraInfo, Quality};
use crate::scrcpy::protocol::ControlMessage;
use crate::scrcpy::{server, zoom};
use crate::settings::Settings;
use crate::vcam::VirtualCamera;
use crate::wifi::{self, QrPairing};
use super::i18n;
use crate::{platform, portable, resources};

const DEVICE_POLL: Duration = Duration::from_millis(1500);
/// After this long without zoom messages, the zoom the phone reported is trusted over the
/// steps counted here (a message sent before the camera was ready is dropped by the server).
const ZOOM_RESYNC: Duration = Duration::from_secs(1);
/// How often remembered Wi-Fi phones that are not connected are looked for.
const AUTO_CONNECT_PERIOD: Duration = Duration::from_secs(10);
/// How long the QR code waits for the phone to scan it.
const QR_WAIT: Duration = Duration::from_secs(180);
/// After pairing, how long to wait for the phone to show up for connecting.
const PAIRED_CONNECT_WAIT: Duration = Duration::from_secs(15);
/// `adb pair` per address tried; the key exchange can take a few seconds on a slow network.
const PAIR_TIMEOUT: Duration = Duration::from_secs(10);
/// Our QR codes announce themselves as `plugcam-…` while pairing; phones as `adb-…`.
const QR_NAME_PREFIX: &str = "plugcam-";

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
    /// Bit rate the phone streams at, in bits per second; 0 when not streaming.
    pub bitrate: u32,
    /// Every phone adb lists, to pick one from.
    pub devices: Vec<DeviceView>,
    /// Phones remembered for Wi-Fi and whether each is connected now.
    pub wifi_phones: Vec<WifiPhoneView>,
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
pub struct WifiPhoneView {
    pub id: String,
    pub name: Option<String>,
    /// The serial it is connected under, when it is connected.
    pub serial: Option<String>,
    /// Switched over from the cable (`adb tcpip`): not encrypted, gone after the phone restarts.
    pub plain: bool,
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
    /// A Wi-Fi phone came back under another serial.
    Serial { old: String, new: String },
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
    bitrate: u32,
    /// Remembered Wi-Fi phone ids of connected `ip:port` serials (mDNS serials carry theirs).
    wifi_ids: HashMap<String, String>,
    last_auto_connect: Option<Instant>,
    auto_connecting: bool,
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
    /// The QR code on screen, tagged so a wait for an older one gives up.
    qr: Mutex<Option<(u64, QrPairing)>>,
    qr_counter: AtomicU64,
    /// Wi-Fi phones the user forgot: neither shown as the active phone nor picked or reconnected
    /// by pipelines. Not in `state`, since pipelines read it (see the locking rule); taken after
    /// `state` when both are needed.
    forgotten: Arc<Mutex<Forgotten>>,
    /// The last phone paired in this session: its mDNS name (when adb gave it) and the Wi-Fi IP
    /// it was paired at. Used when the address the phone shows does not work (a VPN on it).
    paired: Mutex<Option<(Option<String>, String)>>,
    mica: bool,
    /// Windows locale, e.g. `ru-RU`.
    system_locale: String,
}

impl Controller {
    pub fn new(app: AppHandle, settings_path: PathBuf) -> Arc<Self> {
        let settings = Settings::load(&settings_path);
        let forgotten = Forgotten { ids: settings.forgotten_wifi.iter().cloned().collect(), serials: HashSet::new() };
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
                bitrate: 0,
                wifi_ids: HashMap::new(),
                last_auto_connect: None,
                auto_connecting: false,
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
            qr: Mutex::new(None),
            qr_counter: AtomicU64::new(0),
            forgotten: Arc::new(Mutex::new(forgotten)),
            paired: Mutex::new(None),
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
                        Event::Serial { old, new } => c.on_serial_change(&old, &new),
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

    fn build_snapshot(&self, st: &State) -> Snapshot {
        let view = |d: &Device| DeviceView {
            serial: d.serial.clone(),
            model: d.model.clone().unwrap_or_else(|| d.serial.clone()),
            name: st.phones.get(&d.serial).and_then(|p| p.name.clone()),
            wifi: d.is_wifi(),
            state: d.state.clone(),
        };
        let device = st.active(&self.forgotten.lock().unwrap()).map(view);
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
                    zoom_max: c.zoom.map(|z| z.1),
                })
                .collect(),
            selected_camera: selected.map(|c| c.id.clone()),
            smooth_available: selected.is_some_and(|c| cameras::capture_mode(c, Quality::Smooth).is_some()),
            torch: st.torch,
            fps: st.fps,
            bitrate: st.bitrate,
            devices: st.devices.iter().map(view).collect(),
            wifi_phones: st
                .settings
                .wifi_phones
                .iter()
                .map(|p| WifiPhoneView {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    serial: connected_serial(st, &p.id),
                    plain: is_plain(&p.id),
                })
                .collect(),
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
        let device = st.active(&self.forgotten.lock().unwrap()).cloned();
        let model = device.as_ref().and_then(|d| d.model.clone()).unwrap_or_default();
        let phone = device.as_ref().and_then(|d| st.phones.get(&d.serial));
        let usable: Vec<&CameraInfo> = phone
            .map(|p| p.cameras.iter().filter(|c| !st.settings.is_broken(&model, &c.id)).collect())
            .unwrap_or_default();
        let selected = selected_camera(&st.settings, &usable);
        let zoom_range = selected.and_then(|c| c.zoom);
        let mode = match selected {
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
        let serial_tx = self.status_tx.clone();
        let forgotten = self.forgotten.clone();
        let config = PipelineConfig {
            adb,
            server_file,
            serial: st.picked_serial(),
            camera: params,
            mirror: st.settings.mirror,
            rotation: st.settings.rotation,
            color: st.settings.color,
            dump: None,
            preview: Some(self.preview.clone()),
            on_zoom: Some(Arc::new(move |z| {
                let _ = zoom_tx.send((generation, Event::Zoom(z)));
            })),
            on_serial: Some(Arc::new(move |old, new| {
                let _ = serial_tx.send((generation, Event::Serial { old: old.into(), new: new.into() }));
            })),
            wanted: Some(Arc::new(move |serial| !forgotten.lock().unwrap().contains(serial))),
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
            st.bitrate = 0;
        }
        st.status = Some(status);
        self.publish(&mut st);
    }

    /// Keeps a phone's Wi-Fi id when a pipeline got it back under a new serial, so the picked
    /// phone (stored by id) still resolves to it in the window and in the running pipeline.
    fn on_serial_change(&self, old: &str, new: &str) {
        let mut st = self.state.lock().unwrap();
        let id = st.wifi_ids.remove(old).or_else(|| wifi::mdns_name(old).map(str::to_string));
        if let Some(id) = id {
            st.wifi_ids.insert(new.to_string(), id);
        }
        self.follow_picked(&st);
        self.publish(&mut st);
    }

    /// Points the running pipeline at the picked phone's current serial once it is connected;
    /// its address may have changed since the pipeline started.
    fn follow_picked(&self, st: &State) {
        if let (Some(p), Some(serial)) = (&st.pipeline, st.picked_ready()) {
            p.set_preferred(Some(serial));
        }
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
            let mut st = self.state.lock().unwrap();
            let was_ready = st.active(&self.forgotten.lock().unwrap()).is_some_and(Device::is_ready);
            st.phones.retain(|serial, _| devices.iter().any(|d| &d.serial == serial && d.is_ready()));
            // Kept while the phone is off the network, so it is known again when it comes back.
            let State { wifi_ids, settings, .. } = &mut *st;
            wifi_ids.retain(|_, id| settings.wifi_phones.iter().any(|p| &p.id == id));
            st.devices = devices;
            let now_ready = st.active(&self.forgotten.lock().unwrap()).filter(|d| d.is_ready()).cloned();
            self.follow_picked(&st);
            self.name_wifi_phones(&mut st);
            self.auto_connect(&mut st, &adb);

            if let Some(d) = &now_ready {
                if !st.phones.contains_key(&d.serial) && !st.fetching.contains(&d.serial) {
                    st.fetching.push(d.serial.clone());
                    let (c, adb, serial) = (self.clone(), adb.clone(), d.serial.clone());
                    std::thread::spawn(move || c.fetch_phone(&adb, &serial));
                }
            }
            if let Some(p) = st.pipeline.as_ref() {
                let stats = p.stats();
                let streaming = matches!(st.status, Some(Status::Streaming { .. }));
                (st.fps, st.bitrate) = if streaming { (stats.fps, stats.bitrate) } else { (0.0, 0) };
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

    /// Keeps the names of remembered Wi-Fi phones up to date once they are fetched.
    fn name_wifi_phones(&self, st: &mut State) {
        let mut changed = false;
        for d in st.devices.iter().filter(|d| d.is_ready()) {
            let (Some(id), Some(name)) = (wifi_id_of(st, d), st.phones.get(&d.serial).and_then(|p| p.name.clone())) else {
                continue;
            };
            if let Some(p) = st.settings.wifi_phones.iter_mut().find(|p| p.id == id && p.name.as_ref() != Some(&name)) {
                p.name = Some(name);
                changed = true;
            }
        }
        if changed {
            self.save(&st.settings);
        }
    }

    /// Now and then, connects remembered Wi-Fi phones that are on the network but not connected.
    /// Paired phones are found by their mDNS name, so a new address does not matter.
    fn auto_connect(self: &Arc<Self>, st: &mut State, adb: &Adb) {
        if st.auto_connecting || st.last_auto_connect.is_some_and(|t| t.elapsed() < AUTO_CONNECT_PERIOD) {
            return;
        }
        let missing: Vec<String> =
            st.settings.wifi_phones.iter().filter(|p| connected_serial(st, &p.id).is_none()).map(|p| p.id.clone()).collect();
        st.last_auto_connect = Some(Instant::now());
        if missing.is_empty() {
            return;
        }
        st.auto_connecting = true;
        let (c, adb) = (self.clone(), adb.clone());
        std::thread::spawn(move || {
            let connected: Vec<(String, String)> =
                missing.into_iter().filter_map(|id| connect_known(&adb, &id).map(|serial| (serial, id))).collect();
            let mut unwanted = Vec::new();
            let mut st = c.state.lock().unwrap();
            for (serial, id) in connected {
                if st.settings.wifi_phones.iter().any(|p| p.id == id) {
                    log::info!("connected {id} over Wi-Fi as {serial}");
                    st.wifi_ids.insert(serial, id);
                } else {
                    unwanted.push(serial); // forgotten while it was being connected
                }
            }
            st.auto_connecting = false;
            st.last_auto_connect = Some(Instant::now());
            drop(st);
            for serial in unwanted {
                let _ = adb.disconnect(&serial);
            }
        });
    }

    fn adb(&self) -> Result<Adb, String> {
        self.adb.clone().ok_or_else(|| "adb.exe not found".to_string())
    }

    /// Picks the phone to use (`None`: the first ready one, USB first). A Wi-Fi phone is stored
    /// by its id, so the choice holds when its address changes.
    pub fn select_phone(&self, serial: Option<String>) {
        let restart = {
            let mut st = self.state.lock().unwrap();
            let key = serial.as_deref().map(|s| match st.devices.iter().find(|d| d.serial == s) {
                Some(d) => phone_key(d, &st.wifi_ids),
                None => wifi::mdns_name(s).unwrap_or(s).to_string(),
            });
            if let (Some(key), Some(serial)) = (&key, &serial) {
                // Picking a forgotten phone that adb connected by itself takes it back.
                st.settings.forgotten_wifi.retain(|i| i != key);
                let mut forgotten = self.forgotten.lock().unwrap();
                forgotten.remove(key);
                forgotten.remove(serial);
            }
            let changed = st.settings.phone != key;
            st.settings.phone = key;
            self.save(&st.settings);
            let target = st.picked_serial();
            if let Some(p) = &st.pipeline {
                p.set_preferred(target.clone());
            }
            self.publish(&mut st);
            // A picked phone the pipeline is on already (found automatically) needs no restart;
            // one it is not on does, even when picked again, so a click brings it back.
            let on_it = target.is_some() && st.status.as_ref().and_then(status_serial) == target.as_deref();
            st.camera_on && if target.is_some() { !on_it } else { changed }
        };
        if restart {
            self.restart();
        }
    }

    /// A new QR code for pairing; the phone scans it under Wireless debugging → "Pair device
    /// with QR code". Returns it as SVG.
    pub fn wifi_qr_start(&self) -> String {
        let pairing = QrPairing::new();
        let svg = pairing.svg();
        let id = self.qr_counter.fetch_add(1, Ordering::Relaxed) + 1;
        *self.qr.lock().unwrap() = Some((id, pairing));
        svg
    }

    pub fn wifi_qr_cancel(&self) {
        *self.qr.lock().unwrap() = None;
    }

    /// Waits for the phone to scan the QR code on screen, then pairs and connects. Returns the
    /// phone's serial. Fails with `cancelled` when the code was closed or replaced.
    pub fn wifi_qr_wait(&self) -> Result<String, String> {
        let adb = self.adb()?;
        let (id, pairing) = self.qr.lock().unwrap().clone().ok_or("cancelled")?;
        let current = || self.qr.lock().unwrap().as_ref().is_some_and(|(i, _)| *i == id);
        let until = Instant::now() + QR_WAIT;
        loop {
            if !current() {
                return Err("cancelled".into());
            }
            if Instant::now() > until {
                return Err("qrTimeout".into());
            }
            let found = adb
                .mdns_services()
                .unwrap_or_default()
                .into_iter()
                .find(|s| s.kind == MdnsKind::Pairing && s.name == pairing.name);
            if let Some(service) = found {
                let guid = adb.pair(&service.address, &pairing.password).map_err(|e| e.to_string());
                if current() {
                    *self.qr.lock().unwrap() = None;
                }
                return self.finish_pairing(&adb, guid?, service.ip());
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    /// Pairs with the six-digit code the phone shows under Wireless debugging → "Pair device with
    /// pairing code". Without an address the phone is looked up on the network.
    pub fn wifi_pair_code(&self, code: &str, address: Option<&str>) -> Result<String, String> {
        let adb = self.adb()?;
        let code: String = code.chars().filter(char::is_ascii_digit).collect();
        if code.len() != 6 {
            return Err("badCode".into());
        }
        let address = match address.map(str::trim).filter(|a| !a.is_empty()) {
            Some(a) => a.to_string(),
            None => find_pairing_phone(&adb).ok_or("noPairingPhone")?,
        };
        let (address, guid) =
            self.try_addresses(&adb, &address, MdnsKind::Pairing, |a| adb.pair_within(a, &code, PAIR_TIMEOUT))?;
        let ip = address.rsplit_once(':').map_or(address.as_str(), |(ip, _)| ip).to_string();
        self.finish_pairing(&adb, guid, &ip)
    }

    /// After pairing: connects the phone (adb usually does it by itself once the phone announces
    /// itself on mDNS), remembers it and switches to it.
    /// Fails with `pairedNotFound:<ip>` when the phone does not announce itself for connecting
    /// (a VPN on the phone does that): the window then asks for the port at that IP.
    fn finish_pairing(&self, adb: &Adb, guid: Option<String>, ip: &str) -> Result<String, String> {
        log::info!("paired with {guid:?} at {ip}");
        *self.paired.lock().unwrap() = Some((guid.clone(), ip.to_string()));
        let until = Instant::now() + PAIRED_CONNECT_WAIT;
        let serial = loop {
            let devices = adb.devices().unwrap_or_default();
            let by_mdns = devices.iter().find(|d| d.is_ready() && guid.is_some() && wifi::mdns_name(&d.serial) == guid.as_deref());
            if let Some(d) = by_mdns {
                break d.serial.clone();
            }
            let service = adb.mdns_services().unwrap_or_default().into_iter().find(|s| {
                s.kind == MdnsKind::Connect && guid.as_ref().map_or(s.ip() == ip, |g| &s.name == g)
            });
            if let Some(s) = service
                && adb.connect(&s.address).is_ok()
            {
                break s.address;
            }
            if Instant::now() > until {
                return Err(format!("pairedNotFound:{ip}"));
            }
            std::thread::sleep(Duration::from_secs(1));
        };
        let id = guid.unwrap_or_else(|| serial.clone());
        self.use_wifi_phone(&serial, &id, None);
        Ok(serial)
    }

    /// Connects to `ip:port` from the Wireless debugging screen, for networks where phones cannot
    /// be found by name. The phone must have been paired before.
    pub fn wifi_connect(&self, address: &str) -> Result<String, String> {
        let adb = self.adb()?;
        let address = address.trim();
        if !address.contains(':') {
            return Err("badAddress".into());
        }
        let (address, ()) =
            self.try_addresses(&adb, address, MdnsKind::Connect, |a| {
                // A failed connect can leave an `offline` transport behind (an address that
                // answers but is not the phone, e.g. the phone's VPN address); drop it.
                adb.connect_within(a, wifi::QUICK_TIMEOUT).inspect_err(|_| {
                    let _ = adb.disconnect_within(a, wifi::QUICK_TIMEOUT);
                })
            })?;
        // A paired phone is remembered by its mDNS name when the network lets us see it, or by
        // the name it was just paired under at this IP (a VPN on the phone hides it from mDNS).
        let paired_name = self
            .paired
            .lock()
            .unwrap()
            .clone()
            .and_then(|(name, ip)| name.filter(|_| address.rsplit_once(':').is_some_and(|(a, _)| a == ip)));
        let id = adb
            .mdns_services()
            .unwrap_or_default()
            .into_iter()
            .find(|s| s.kind == MdnsKind::Connect && s.address == address)
            .map(|s| s.name)
            .or(paired_name)
            .unwrap_or_else(|| address.clone());
        self.use_wifi_phone(&address, &id, None);
        Ok(address)
    }

    /// Runs `attempt` (pair or connect) on the address the user typed; if that fails, on the
    /// same port at other IPs this phone may have (see `other_addresses`). Returns the address
    /// that worked, or the first error.
    fn try_addresses<T>(
        &self,
        adb: &Adb,
        address: &str,
        kind: MdnsKind,
        attempt: impl Fn(&str) -> Result<T, adb::AdbError>,
    ) -> Result<(String, T), String> {
        let first = match attempt(address) {
            Ok(v) => return Ok((address.to_string(), v)),
            Err(e) => e.to_string(),
        };
        for other in self.other_addresses(adb, address, kind) {
            log::info!("{address} failed, trying {other}");
            if let Ok(v) = attempt(&other) {
                return Ok((other, v));
            }
        }
        Err(first)
    }

    /// With a VPN on, the phone shows its VPN address, which the PC cannot reach; the port is
    /// right. IPs worth trying with that port: a phone announcing the same port on the network,
    /// the IP the last phone was paired at, and the Wi-Fi IP of the phone on the cable when it
    /// is plausibly the same phone.
    fn other_addresses(&self, adb: &Adb, address: &str, kind: MdnsKind) -> Vec<String> {
        let port = address.rsplit_once(':').map(|(_, p)| p).unwrap_or_default();
        let mut ips: Vec<String> = adb
            .mdns_services_within(wifi::QUICK_TIMEOUT)
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.kind == kind && s.address.rsplit_once(':').is_some_and(|(_, p)| p == port))
            .map(|s| s.ip().to_string())
            .collect();
        let paired = self.paired.lock().unwrap().clone();
        if let Some((_, ip)) = &paired {
            ips.push(ip.clone());
        }
        let usb: Vec<String> = {
            let st = self.state.lock().unwrap();
            st.devices.iter().filter(|d| d.is_ready() && !d.is_wifi()).map(|d| d.serial.clone()).collect()
        };
        let usb: Vec<&str> = usb.iter().map(String::as_str).collect();
        let guid = paired.as_ref().and_then(|(g, _)| g.as_deref());
        if let Some(ip) = usb_phone_for(guid, &usb).and_then(|serial| adb.wifi_ip(serial)) {
            ips.push(ip);
        }
        let mut out: Vec<String> = Vec::new();
        for other in ips.iter().filter_map(|ip| with_ip(address, ip)) {
            if !out.contains(&other) {
                out.push(other);
            }
        }
        out
    }

    /// "From cable to Wi-Fi": moves the phone on the cable to plain TCP adb and connects over the
    /// network. Not encrypted; lasts until the phone restarts.
    pub fn wifi_from_cable(&self) -> Result<String, String> {
        let adb = self.adb()?;
        let (serial, name) = {
            let st = self.state.lock().unwrap();
            let usb = st.devices.iter().filter(|d| d.is_ready() && !d.is_wifi());
            let chosen = usb.clone().find(|d| st.settings.phone.as_ref() == Some(&d.serial)).or(usb.clone().next());
            let d = chosen.ok_or("noUsbPhone")?;
            (d.serial.clone(), st.phones.get(&d.serial).and_then(|p| p.name.clone()))
        };
        let address = wifi::switch_from_cable(&adb, &serial).map_err(|e| match e {
            wifi::SwitchError::NoWifi => "phoneNotOnWifi".to_string(),
            e => e.to_string(),
        })?;
        self.use_wifi_phone(&address, &address, name);
        Ok(address)
    }

    /// Remembers a phone just connected over Wi-Fi and makes it the one in use.
    fn use_wifi_phone(&self, serial: &str, id: &str, name: Option<String>) {
        let restart = {
            let mut st = self.state.lock().unwrap();
            {
                let mut forgotten = self.forgotten.lock().unwrap();
                forgotten.remove(serial);
                forgotten.remove(id);
                forgotten.remove(&wifi::mdns_serial(id));
            }
            st.wifi_ids.insert(serial.to_string(), id.to_string());
            st.settings.remember_wifi_phone(id, name);
            st.settings.phone = Some(id.to_string());
            self.save(&st.settings);
            if let Some(p) = &st.pipeline {
                p.set_preferred(Some(serial.to_string()));
            }
            self.publish(&mut st);
            st.camera_on && st.status.as_ref().and_then(status_serial) != Some(serial)
        };
        if restart {
            self.restart();
        }
    }

    /// Forgets a Wi-Fi phone and disconnects it. If the camera uses it, it moves to another
    /// phone. The phone stays paired with this PC; only the phone can remove that.
    pub fn wifi_forget(&self, id: &str) {
        let (listed, restart) = {
            let mut st = self.state.lock().unwrap();
            // Every serial it may have now or after coming back.
            let mut serials: HashSet<String> =
                st.devices.iter().filter(|d| wifi_id_of(&st, d).as_deref() == Some(id)).map(|d| d.serial.clone()).collect();
            serials.extend(st.wifi_ids.iter().filter(|(_, i)| *i == id).map(|(s, _)| s.clone()));
            serials.insert(id.to_string());
            serials.insert(wifi::mdns_serial(id));
            st.wifi_ids.retain(|_, i| i != id);
            st.settings.forget_wifi_phone(id);
            let picked = st.settings.phone.as_ref().is_some_and(|s| serials.contains(s));
            if picked {
                st.settings.phone = None;
            }
            let in_use = st.status.as_ref().and_then(status_serial).is_some_and(|s| serials.contains(s));
            let listed: Vec<String> = st.devices.iter().filter(|d| serials.contains(&d.serial)).map(|d| d.serial.clone()).collect();
            {
                let mut forgotten = self.forgotten.lock().unwrap();
                forgotten.ids.insert(id.to_string());
                forgotten.serials.extend(serials);
            }
            self.save(&st.settings);
            self.publish(&mut st);
            (listed, st.camera_on && (picked || in_use))
        };
        if let Some(adb) = &self.adb {
            for serial in &listed {
                let _ = adb.disconnect(serial);
            }
        }
        if restart {
            self.restart();
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

impl State {
    /// The serial the picked phone has now (see `resolve_phone`).
    fn picked_serial(&self) -> Option<String> {
        self.settings.phone.as_deref().map(|p| resolve_phone(&self.devices, &self.wifi_ids, p))
    }

    /// The picked phone's serial when it is connected and ready.
    fn picked_ready(&self) -> Option<String> {
        let phone = self.settings.phone.as_deref()?;
        find_phone(&self.devices, &self.wifi_ids, phone).map(|d| d.serial.clone())
    }

    /// The phone the app works with, leaving out forgotten ones.
    fn active(&self, forgotten: &Forgotten) -> Option<&Device> {
        active_device(&self.devices, self.picked_serial().as_deref(), |s| !forgotten.contains(s))
    }
}

/// Wi-Fi phones the user forgot: by id (saved in the settings) and by the serials they had.
#[derive(Debug, Default)]
struct Forgotten {
    ids: HashSet<String>,
    serials: HashSet<String>,
}

impl Forgotten {
    fn contains(&self, serial: &str) -> bool {
        self.serials.contains(serial)
            || self.ids.contains(serial)
            || wifi::mdns_name(serial).is_some_and(|name| self.ids.contains(name))
    }

    /// Takes back a phone, by id or serial.
    fn remove(&mut self, key: &str) {
        self.ids.remove(key);
        self.serials.remove(key);
        if let Some(name) = wifi::mdns_name(key) {
            self.ids.remove(name);
        }
    }
}

/// The phone the app works with: the picked one if it is ready, else the first ready one (USB
/// first), else any listed one so the window can say "allow USB debugging". Phones that are not
/// `wanted` are left out.
fn active_device<'a>(devices: &'a [Device], preferred: Option<&str>, wanted: impl Fn(&str) -> bool) -> Option<&'a Device> {
    let usable = || devices.iter().filter(|d| wanted(&d.serial));
    adb::choose(usable(), preferred).or_else(|| usable().next())
}

/// The remembered Wi-Fi phone a device is, by its mDNS name or the address we connected to.
fn wifi_id(d: &Device, wifi_ids: &HashMap<String, String>) -> Option<String> {
    if !d.is_wifi() {
        return None;
    }
    let id = wifi::mdns_name(&d.serial).map(str::to_string).or_else(|| wifi_ids.get(&d.serial).cloned());
    Some(id.unwrap_or_else(|| d.serial.clone()))
}

fn wifi_id_of(st: &State, d: &Device) -> Option<String> {
    wifi_id(d, &st.wifi_ids)
}

/// What `settings.phone` stores for a device: a Wi-Fi phone's id, a USB phone's serial.
fn phone_key(d: &Device, wifi_ids: &HashMap<String, String>) -> String {
    wifi_id(d, wifi_ids).unwrap_or_else(|| d.serial.clone())
}

/// The ready device a `settings.phone` value means now. Older settings may hold an mDNS serial
/// instead of the id.
fn find_phone<'a>(devices: &'a [Device], wifi_ids: &HashMap<String, String>, phone: &str) -> Option<&'a Device> {
    // An id saved with adb's " (2)" suffix still means the same phone.
    let serial = wifi::mdns_serial(phone);
    let key = wifi::mdns_name(phone).or_else(|| wifi::mdns_name(&serial)).unwrap_or(phone);
    devices.iter().find(|d| d.is_ready() && (d.serial == phone || wifi_id(d, wifi_ids).as_deref() == Some(key)))
}

/// The serial a `settings.phone` value has now: the ready device it is, else the serial adb
/// gives it by itself (an mDNS phone's mDNS serial, or the address or USB serial as it is).
fn resolve_phone(devices: &[Device], wifi_ids: &HashMap<String, String>, phone: &str) -> String {
    if let Some(d) = find_phone(devices, wifi_ids, phone) {
        return d.serial.clone();
    }
    if wifi::mdns_name(phone).is_none() && phone.starts_with("adb-") && !phone.contains(':') {
        wifi::mdns_serial(phone)
    } else {
        phone.to_string()
    }
}

/// The phone a pipeline status is about.
fn status_serial(status: &Status) -> Option<&str> {
    match status {
        Status::Connecting { serial }
        | Status::Streaming { serial, .. }
        | Status::Reconnecting { serial, .. }
        | Status::NoPicture { serial } => Some(serial),
        Status::WaitingForDevice | Status::Error { .. } | Status::Stopped => None,
    }
}

fn connected_serial(st: &State, id: &str) -> Option<String> {
    st.devices.iter().find(|d| d.is_ready() && wifi_id_of(st, d).as_deref() == Some(id)).map(|d| d.serial.clone())
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

/// Connects a remembered Wi-Fi phone if it is on the network. Returns its serial.
fn connect_known(adb: &Adb, id: &str) -> Option<String> {
    let address = if id.contains(':') {
        id.to_string()
    } else {
        let services = adb.mdns_services().ok()?;
        services.into_iter().find(|s| s.kind == MdnsKind::Connect && s.name == id)?.address
    };
    adb.connect(&address).ok().map(|()| address)
}

/// The address of the phone showing "Pair device with pairing code", if exactly one does.
fn find_pairing_phone(adb: &Adb) -> Option<String> {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let phones: Vec<String> = adb
            .mdns_services()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.kind == MdnsKind::Pairing && !s.name.starts_with(QR_NAME_PREFIX))
            .map(|s| s.address)
            .collect();
        if phones.len() == 1 {
            return phones.into_iter().next();
        }
        if Instant::now() > until {
            return None;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Whether a remembered Wi-Fi phone is on plain TCP adb ("from cable", port 5555) rather than
/// paired TLS, which is remembered by its mDNS name or by an address on a random port.
fn is_plain(id: &str) -> bool {
    !id.starts_with("adb-") && id.rsplit_once(':').is_some_and(|(_, port)| port == wifi::TCPIP_PORT.to_string())
}

/// `address` with its IP replaced by `ip`, keeping the port; `None` when there is no port or
/// the IP is the same.
fn with_ip(address: &str, ip: &str) -> Option<String> {
    let (host, port) = address.rsplit_once(':')?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let valid = !ip.is_empty() && !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit());
    (valid && host != ip).then(|| format!("{ip}:{port}"))
}

/// The phone on the cable that is the one paired over Wi-Fi: the one named in its mDNS name
/// (`adb-<serial>-…`), or without a name, the only phone on the cable.
fn usb_phone_for<'a>(mdns_name: Option<&str>, usb: &[&'a str]) -> Option<&'a str> {
    match mdns_name {
        Some(name) => {
            let rest = name.strip_prefix("adb-")?;
            usb.iter().copied().find(|s| rest.strip_prefix(s).is_some_and(|r| r.starts_with('-')))
        }
        None => (usb.len() == 1).then(|| usb[0]),
    }
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
        let all = |_: &str| true;
        let list = [device("1.2.3.4:5555", "device"), device("abc", "unauthorized"), device("usb1", "device")];
        assert_eq!(active_device(&list, None, all).unwrap().serial, "usb1");
        assert_eq!(active_device(&list, Some("1.2.3.4:5555"), all).unwrap().serial, "1.2.3.4:5555");
        assert_eq!(active_device(&list, Some("gone"), all).unwrap().serial, "usb1");
        let list = [device("abc", "unauthorized")];
        assert_eq!(active_device(&list, None, all).unwrap().serial, "abc");
        assert!(active_device(&[], None, all).is_none());
    }

    #[test]
    fn forgotten_phones_are_never_active() {
        let forgotten = Forgotten { ids: ["adb-1-x".to_string()].into(), serials: ["10.0.0.2:4000".to_string()].into() };
        let wanted = |s: &str| !forgotten.contains(s);
        let list = [device("adb-1-x._adb-tls-connect._tcp", "device"), device("10.0.0.2:4000", "device")];
        assert!(active_device(&list, None, wanted).is_none());
        let list = [device("adb-1-x._adb-tls-connect._tcp", "device"), device("10.0.0.3:5555", "device")];
        assert_eq!(active_device(&list, Some("adb-1-x._adb-tls-connect._tcp"), wanted).unwrap().serial, "10.0.0.3:5555");
        let mut forgotten = forgotten;
        forgotten.remove("adb-1-x._adb-tls-connect._tcp");
        assert!(!forgotten.contains("adb-1-x._adb-tls-connect._tcp") && forgotten.contains("10.0.0.2:4000"));
    }

    #[test]
    fn picked_wifi_phone_follows_its_address() {
        let mut ids = HashMap::new();
        let usb = device("usb1", "device");
        let mdns = device("adb-1-x._adb-tls-connect._tcp", "device");
        let by_address = device("10.0.0.2:4001", "device");
        // Stored by id, found under whichever serial it has now.
        assert_eq!(phone_key(&mdns, &ids), "adb-1-x");
        assert_eq!(phone_key(&usb, &ids), "usb1");
        assert_eq!(resolve_phone(&[usb.clone(), mdns.clone()], &ids, "adb-1-x"), mdns.serial);
        ids.insert(by_address.serial.clone(), "adb-1-x".to_string());
        assert_eq!(phone_key(&by_address, &ids), "adb-1-x");
        assert_eq!(resolve_phone(&[usb.clone(), by_address.clone()], &ids, "adb-1-x"), "10.0.0.2:4001");
        // Older settings stored the mDNS serial.
        assert_eq!(resolve_phone(std::slice::from_ref(&by_address), &ids, &mdns.serial), "10.0.0.2:4001");
        // Not connected: the serial adb gives it by itself.
        let only_usb = std::slice::from_ref(&usb);
        assert_eq!(resolve_phone(only_usb, &ids, "adb-1-x"), mdns.serial);
        assert!(find_phone(only_usb, &ids, "adb-1-x").is_none());
        assert_eq!(resolve_phone(&[], &ids, "10.0.0.9:5555"), "10.0.0.9:5555");
        assert_eq!(resolve_phone(only_usb, &ids, "usb1"), "usb1");
        assert!(find_phone(&[device("usb1", "offline")], &ids, "usb1").is_none());
    }

    #[test]
    fn only_tcpip_phones_are_plain() {
        assert!(is_plain("192.168.3.237:5555"));
        assert!(!is_plain("192.168.3.237:37075"));
        assert!(!is_plain("adb-834ee2d7-nJE3Xk"));
    }

    #[test]
    fn other_ip_same_port() {
        assert_eq!(with_ip("26.26.26.1:37075", "192.168.3.237").as_deref(), Some("192.168.3.237:37075"));
        assert_eq!(with_ip("192.168.3.237:37075", "192.168.3.237"), None);
        assert_eq!(with_ip("26.26.26.1", "192.168.3.237"), None);
        assert_eq!(with_ip("26.26.26.1:", "192.168.3.237"), None);
        assert_eq!(with_ip("26.26.26.1:37075", ""), None);
    }

    #[test]
    fn cable_phone_is_the_paired_one() {
        assert_eq!(usb_phone_for(Some("adb-834ee2d7-nJE3Xk"), &["R58N", "834ee2d7"]), Some("834ee2d7"));
        assert_eq!(usb_phone_for(Some("adb-834ee2d7-nJE3Xk"), &["834ee2"]), None);
        assert_eq!(usb_phone_for(Some("adb-834ee2d7-nJE3Xk"), &["R58N"]), None);
        assert_eq!(usb_phone_for(None, &["R58N"]), Some("R58N"));
        assert_eq!(usb_phone_for(None, &["R58N", "834ee2d7"]), None);
        assert_eq!(usb_phone_for(None, &[]), None);
    }

    #[test]
    fn names() {
        assert_eq!(pretty_name("OnePlus  PHK110\n"), "OnePlus PHK110");
        assert_eq!(pretty_name("Google Pixel 8 Pixel 8"), "Google Pixel 8");
    }
}
