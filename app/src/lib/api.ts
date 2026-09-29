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
  /** Write a detailed log to a file for bug reports. */
  detailedLog: boolean;
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
/** Saves a bug report to Downloads and opens the new-issue form; returns the file's path. */
export const saveReport = () => invoke<string>("save_report");
export const openLogFolder = () => invoke<void>("open_log_folder");

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
