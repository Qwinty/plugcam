// Types and calls mirroring src-tauri/src/app (Snapshot and commands).

import { invoke, Channel } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Quality = "economy" | "standard" | "smooth";

export interface Settings {
  onboardingDone: boolean;
  facing: "back" | "front";
  cameraId: string | null;
  quality: Quality;
  mirror: boolean;
  rotation: 0 | 90 | 180 | 270;
  color: ColorAdjust;
  bitrateMbps: number | null;
  vcamWidth: number;
  vcamHeight: number;
  launchAtLogin: boolean;
  closeToTray: boolean;
  autoStartCamera: boolean;
  /** `null` = same as Windows. */
  language: string | null;
  checkUpdates: boolean;
  /** The version whose "What's new" was last shown; `null` on a fresh install. */
  lastSeenVersion: string | null;
  /** The picked phone's serial; `null` = the first ready one, USB first. */
  phone: string | null;
}

/** Picture adjustments made on the PC, each from -100 to 100. */
export interface ColorAdjust {
  brightness: number;
  contrast: number;
  saturation: number;
  warmth: number;
}

export type Status =
  | { kind: "waitingForDevice" }
  | { kind: "connecting"; serial: string }
  | { kind: "streaming"; serial: string; device: string; width: number; height: number }
  | { kind: "reconnecting"; serial: string; attempt: number; tries: number }
  | { kind: "noPicture"; serial: string }
  | { kind: "error"; message: string }
  | { kind: "stopped" };

export interface DeviceView {
  serial: string;
  model: string;
  name: string | null;
  wifi: boolean;
  state: string;
}

/** A phone remembered for Wi-Fi. */
export interface WifiPhoneView {
  id: string;
  name: string | null;
  /** The serial it is connected under, when it is connected. */
  serial: string | null;
  /** Switched over from the cable: not encrypted, gone after the phone restarts. */
  plain: boolean;
}

export interface CameraView {
  id: string;
  facing: string;
  megapixels: number;
  zoomMin: number | null;
  zoomMax: number | null;
}

export type Notice = { kind: "lensHidden"; id: string } | { kind: "startFailed"; message: string };

export interface Snapshot {
  settings: Settings;
  cameraOn: boolean;
  status: Status | null;
  device: DeviceView | null;
  cameras: CameraView[];
  selectedCamera: string | null;
  smoothAvailable: boolean;
  torch: boolean;
  fps: number;
  /** Bits per second the phone streams at; 0 when not streaming. */
  bitrate: number;
  /** Every phone adb lists. */
  devices: DeviceView[];
  wifiPhones: WifiPhoneView[];
  /** As the phone last reported it. */
  zoom: number;
  zoomRange: [number, number] | null;
  problem: string | null;
  notice: Notice | null;
  /** Running from a portable folder rather than installed. */
  portable: boolean;
  /** Portable only: "Plugcam Camera" still has to be added to Windows. */
  cameraMissing: boolean;
  mica: boolean;
  /** The language in use. */
  language: string;
  /** What "same as Windows" resolves to. */
  systemLanguage: string;
}

export const getState = () => invoke<Snapshot>("get_state");
export const setCamera = (on: boolean) => invoke<void>("set_camera", { on });
export const updateSettings = (patch: Partial<Settings>) => invoke<void>("update_settings", { patch });
export const setTorch = (on: boolean) => invoke<void>("set_torch", { on });
export const zoom = (zoomIn: boolean) => invoke<void>("zoom", { zoomIn });
export const setZoom = (value: number) => invoke<void>("set_zoom", { value });
export const dismissNotice = () => invoke<void>("dismiss_notice");
export const openUrl = (url: string) => invoke<void>("open_url", { url });
export const setPreviewActive = (active: boolean) => invoke<void>("set_preview_active", { active });
/** How wide the picture is shown, in physical pixels, so no wider JPEGs are made. */
export const setPreviewWidth = (width: number) => invoke<void>("set_preview_width", { width });
export const setCameraRegistered = (on: boolean) => invoke<void>("set_camera_registered", { on });
export const selectPhone = (serial: string | null) => invoke<void>("select_phone", { serial });
/** A new QR code for pairing over Wi-Fi, as SVG. */
export const wifiQrStart = () => invoke<string>("wifi_qr_start");
/** Resolves with the phone's serial once it scanned the code and paired; rejects with an error code. */
export const wifiQrWait = () => invoke<string>("wifi_qr_wait");
export const wifiQrCancel = () => invoke<void>("wifi_qr_cancel");
export const wifiPairCode = (code: string, address: string | null) => invoke<string>("wifi_pair_code", { code, address });
export const wifiConnect = (address: string) => invoke<string>("wifi_connect", { address });
export const wifiFromCable = () => invoke<string>("wifi_from_cable");
export const wifiForget = (id: string) => invoke<void>("wifi_forget", { id });

export type UpdatePhase = "idle" | "checking" | "upToDate" | "available" | "downloading" | "installing" | "error";

export interface UpdateView {
  phase: UpdatePhase;
  /** The newer version, once one was found. */
  version: string | null;
  /** Its release notes (Markdown). */
  notes: string | null;
  /** 0..1 while downloading, when the size is known. */
  progress: number | null;
  error: string | null;
  portable: boolean;
}

export const updateState = () => invoke<UpdateView>("update_state");
export const checkForUpdates = () => invoke<void>("check_for_updates");
/** Resolves only on failure: on success the app restarts as the new version. */
export const installUpdate = () => invoke<void>("install_update");

export function onUpdate(cb: (u: UpdateView) => void): Promise<UnlistenFn> {
  return listen<UpdateView>("update", (e) => cb(e.payload));
}

export function onState(cb: (s: Snapshot) => void): Promise<UnlistenFn> {
  return listen<Snapshot>("state", (e) => cb(e.payload));
}

export function onPreviewClear(cb: () => void): Promise<UnlistenFn> {
  return listen("preview-clear", () => cb());
}

/** JPEG frames of the preview, as raw bytes. */
export function subscribePreview(cb: (jpeg: ArrayBuffer) => void): Promise<void> {
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = cb;
  return invoke("subscribe_preview", { channel });
}
