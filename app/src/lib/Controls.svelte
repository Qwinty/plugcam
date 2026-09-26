<script lang="ts">
  // The side panel next to the video: phone and status, the on/off button and all controls.
  import {
    Smartphone,
    Usb,
    Wifi,
    Settings as SettingsIcon,
    Video,
    VideoOff,
    Aperture,
    RotateCcwSquare,
    RotateCwSquare,
    ZoomIn,
    ZoomOut,
    Info,
    CircleAlert,
    X,
  } from "@lucide/svelte";
  import * as api from "./api";
  import type { Snapshot, CameraView, Quality } from "./api";
  import { t } from "./i18n";
  import { statusInfo } from "./status";
  import Segmented from "./Segmented.svelte";
  import Select from "./Select.svelte";
  import Toggle from "./Toggle.svelte";

  let { snap, onsettings }: { snap: Snapshot; onsettings: () => void } = $props();

  let busy = $state<"starting" | "stopping" | null>(null);

  const info = $derived(statusInfo(snap));
  const s = $derived(snap.settings);
  const streaming = $derived(snap.cameraOn && snap.status?.kind === "streaming");
  const lenses = $derived(snap.cameras.filter((c) => c.facing === s.facing));
  const ready = $derived(snap.device?.state === "device" && !snap.problem);
  const torchAvailable = $derived(streaming && s.facing === "back");

  function lensLabel(c: CameraView, i: number) {
    const name =
      c.zoomMin !== null && c.zoomMin < 1
        ? t("lens.wide")
        : i === 0
          ? t(c.facing === "front" ? "lens.front" : "lens.main")
          : t("lens.other", { id: c.id });
    return `${name} · ${t("lens.mp", { mp: c.megapixels })}`;
  }

  async function toggleCamera() {
    busy = snap.cameraOn ? "stopping" : "starting";
    try {
      await api.setCamera(!snap.cameraOn);
    } finally {
      busy = null;
    }
  }

  const rotate = (by: 90 | 270) => api.updateSettings({ rotation: ((s.rotation + by) % 360) as 0 | 90 | 180 | 270 });
</script>

<div class="panel">
  <header>
    <div class="badge"><Smartphone size={20} /></div>
    <div class="phone">
      <div class="name" title={snap.device?.name ?? snap.device?.model ?? "Plugcam"}>
        {snap.device?.name ?? snap.device?.model ?? "Plugcam"}
      </div>
      {#if snap.device}
        <div class="conn">
          {#if snap.device.wifi}<Wifi size={14} />{:else}<Usb size={14} />{/if}
          {t(snap.device.wifi ? "conn.wifi" : "conn.usb")}
        </div>
      {/if}
    </div>
    <button class="icon-btn" title={t("app.settings")} aria-label={t("app.settings")} onclick={onsettings}>
      <SettingsIcon size={20} />
    </button>
  </header>

  <div class="status tone-{info.tone}" role="status">
    <span class="dot"></span>
    <div class="status-text">
      <span class="status-title">{info.title}</span>
      {#if info.hint}<span class="status-hint">{info.hint}</span>{/if}
    </div>
  </div>

  <button
    class="btn power"
    class:accent={!snap.cameraOn}
    disabled={busy !== null || (!snap.cameraOn && !!snap.problem)}
    onclick={toggleCamera}>
    {#if snap.cameraOn}<VideoOff size={18} />{:else}<Video size={18} />{/if}
    {#if busy === "starting"}{t("camera.starting")}{:else if busy === "stopping"}{t("camera.stopping")}{:else if snap.cameraOn}{t(
        "camera.off",
      )}{:else}{t("camera.on")}{/if}
  </button>

  {#if snap.notice}
    <div class="notice" class:error={snap.notice.kind === "startFailed"} role="alert">
      {#if snap.notice.kind === "startFailed"}<CircleAlert size={16} />{:else}<Info size={16} />{/if}
      <span>
        {#if snap.notice.kind === "lensHidden"}
          {t("notice.lensHidden", { id: snap.notice.id })}
        {:else}
          {t("notice.startFailed", { message: snap.notice.message })}
        {/if}
      </span>
      <button class="icon-btn small" aria-label={t("notice.close")} title={t("notice.close")} onclick={api.dismissNotice}>
        <X size={16} />
      </button>
    </div>
  {/if}

  <section class="section">
    <h2>{t("sec.camera")}</h2>
    <Segmented
      label={t("sec.camera")}
      value={s.facing}
      onchange={(f) => api.updateSettings({ facing: f, cameraId: null })}
      options={[
        { value: "back", label: t("facing.back") },
        { value: "front", label: t("facing.front") },
      ]} />
    {#if lenses.length > 1}
      <Select
        label={t("ctl.lens")}
        value={snap.selectedCamera ?? ""}
        onchange={(id) => api.updateSettings({ cameraId: id || null })}
        options={lenses.map((c, i) => ({ value: c.id, label: lensLabel(c, i) }))}>
        {#snippet icon()}<Aperture size={16} />{/snippet}
      </Select>
    {/if}
  </section>

  <section class="section">
    <h2>{t("sec.picture")}</h2>
    <div class="card">
      <div class="row">
        <span class="row-text">
          <span>{t("ctl.rotation")}</span>
          <span class="row-hint">{s.rotation}°</span>
        </span>
        <div class="tools">
          <button class="icon-btn" title={t("ctl.rotateLeft")} aria-label={t("ctl.rotateLeft")} onclick={() => rotate(270)}>
            <RotateCcwSquare size={20} />
          </button>
          <button class="icon-btn" title={t("ctl.rotateRight")} aria-label={t("ctl.rotateRight")} onclick={() => rotate(90)}>
            <RotateCwSquare size={20} />
          </button>
        </div>
      </div>
      <Toggle label={t("ctl.mirror")} checked={s.mirror} onchange={(v) => api.updateSettings({ mirror: v })} />
    </div>
  </section>

  <section class="section">
    <h2>{t("sec.phone")}</h2>
    <div class="card">
      <Toggle
        label={t("ctl.torch")}
        hint={!streaming ? t("ctl.needsStream") : s.facing !== "back" ? t("ctl.torch.na") : ""}
        checked={snap.torch}
        disabled={!torchAvailable}
        onchange={(v) => api.setTorch(v)} />
      <div class="row" class:off={!streaming}>
        <span class="row-text">
          <span>{t("ctl.zoom")}</span>
          {#if !streaming}<span class="row-hint">{t("ctl.needsStream")}</span>{/if}
        </span>
        <div class="tools">
          <button class="icon-btn" disabled={!streaming} title={t("ctl.zoomOut")} aria-label={t("ctl.zoomOut")} onclick={() => api.zoom(false)}>
            <ZoomOut size={20} />
          </button>
          <button class="icon-btn" disabled={!streaming} title={t("ctl.zoomIn")} aria-label={t("ctl.zoomIn")} onclick={() => api.zoom(true)}>
            <ZoomIn size={20} />
          </button>
        </div>
      </div>
    </div>
  </section>

  <section class="section">
    <h2>{t("sec.quality")}</h2>
    <Segmented
      label={t("sec.quality")}
      value={s.quality}
      onchange={(q: Quality) => api.updateSettings({ quality: q })}
      options={[
        { value: "economy", label: t("quality.economy") },
        { value: "standard", label: t("quality.standard") },
        {
          value: "smooth",
          label: t("quality.smooth"),
          title: snap.smoothAvailable || !ready ? undefined : t("quality.smooth.na"),
          disabled: ready && snap.cameras.length > 0 && !snap.smoothAvailable,
        },
      ]} />
    <p class="quality-hint">{t(`quality.${s.quality}.hint` as const)}</p>
  </section>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .badge {
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent-text);
  }
  .phone {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-size: 16px;
    font-weight: 600;
    line-height: 1.25;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .conn {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--text-2);
  }

  .status {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin-top: -4px;
  }
  .status-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .status-title {
    overflow-wrap: anywhere;
  }
  .status-hint {
    font-size: 12px;
    color: var(--text-2);
    text-wrap: pretty;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    margin-top: 6px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .tone-ok .dot {
    background: var(--ok);
  }
  .tone-live .dot {
    background: var(--ok);
    box-shadow: 0 0 0 3px var(--ok-soft);
  }
  .tone-busy .dot {
    background: var(--accent);
  }
  .tone-warn .dot {
    background: var(--warn);
  }
  .tone-error .dot {
    background: var(--error);
  }

  .power {
    min-height: 40px;
    font-size: 15px;
  }

  .notice {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 4px 8px 12px;
    border-radius: 8px;
    background: var(--warn-soft);
    font-size: 13px;
  }
  .notice.error {
    background: var(--error-soft);
  }
  .notice > :global(svg) {
    margin-top: 2px;
  }
  .notice span {
    flex: 1;
    text-wrap: pretty;
  }
  .icon-btn.small {
    width: 24px;
    height: 24px;
  }

  .tools {
    display: flex;
    gap: 4px;
  }
  .row.off > .row-text > span:first-child {
    color: var(--text-3);
  }
  .quality-hint {
    margin: 0 2px;
    font-size: 12px;
    color: var(--text-2);
  }
</style>
