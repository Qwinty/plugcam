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
    Download,
    ShieldCheck,
    LoaderCircle,
  } from "@lucide/svelte";
  import * as api from "./api";
  import type { Snapshot, CameraView, Quality, UpdateView } from "./api";
  import { t } from "./i18n";
  import { holdRepeat } from "./holdRepeat";
  import { statusInfo } from "./status";
  import Segmented from "./Segmented.svelte";
  import Select from "./Select.svelte";
  import Toggle from "./Toggle.svelte";
  import ColorControls from "./ColorControls.svelte";

  let {
    snap,
    update,
    onsettings,
    onupdate,
  }: { snap: Snapshot; update: UpdateView; onsettings: () => void; onupdate: () => void } = $props();

  let busy = $state<"starting" | "stopping" | null>(null);
  let adding = $state(false);
  let addFailed = $state(false);

  async function addCamera() {
    adding = true;
    addFailed = false;
    try {
      await api.setCameraRegistered(true);
    } catch {
      addFailed = true;
    } finally {
      adding = false;
    }
  }

  const info = $derived(statusInfo(snap));
  const s = $derived(snap.settings);
  const streaming = $derived(snap.cameraOn && snap.status?.kind === "streaming");
  const lenses = $derived(snap.cameras.filter((c) => c.facing === s.facing));
  const ready = $derived(snap.device?.state === "device" && !snap.problem);
  const torchAvailable = $derived(streaming && s.facing === "back");

  // Quick zoom values the running lens can reach, like the 1x/2x/5x of a camera app.
  const zoomPresets = $derived(
    snap.zoomRange ? [0.5, 1, 2, 5].filter((z) => z >= snap.zoomRange![0] - 0.01 && z <= snap.zoomRange![1] + 0.01) : [1, 2, 5],
  );
  // Steps are ~6% apart, so a preset is reached within ~3%.
  const atZoom = (z: number) => Math.abs(snap.zoom / z - 1) < 0.04;
  // 1.948 is what "2x" gets on the step grid; show it as the 2,0x its button promises.
  const shownZoom = $derived(streaming ? (zoomPresets.find(atZoom) ?? snap.zoom) : 1);
  const zoomFormat = $derived(new Intl.NumberFormat(snap.language, { minimumFractionDigits: 1, maximumFractionDigits: 1 }));
  const presetFormat = $derived(new Intl.NumberFormat(snap.language, { maximumFractionDigits: 1 }));

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
    {#if update.version}
      <button class="update-chip" title={t("upd.available.short", { v: update.version })} onclick={onupdate}>
        <Download size={14} />{t("upd.chip")}
      </button>
    {/if}
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

  {#if snap.cameraMissing}
    <div class="setup card">
      <div class="setup-text">
        <span class="setup-title">{t("portable.camera.title")}</span>
        <span class="setup-hint">{t(addFailed ? "portable.camera.failed" : "portable.camera.hint")}</span>
      </div>
      <button class="btn" disabled={adding} onclick={addCamera}>
        {#if adding}<LoaderCircle size={18} class="spin" />{:else}<ShieldCheck size={18} />{/if}
        {t("portable.camera.add")}
      </button>
    </div>
  {/if}

  <button
    class="btn power"
    class:accent={!snap.cameraOn}
    disabled={busy !== null || (!snap.cameraOn && (!!snap.problem || snap.cameraMissing))}
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
      <ColorControls value={s.color} language={snap.language} />
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
      <div class="row zoom" class:off={!streaming}>
        <div class="zoom-top">
          <span class="row-text">
            <span>{t("ctl.zoom")}</span>
            {#if !streaming}<span class="row-hint">{t("ctl.needsStream")}</span>{/if}
          </span>
          <div class="tools">
            <button class="icon-btn" disabled={!streaming} title={t("ctl.zoomOut")} aria-label={t("ctl.zoomOut")} use:holdRepeat={() => api.zoom(false)}>
              <ZoomOut size={20} />
            </button>
            <output class="zoom-value" aria-live="polite">{zoomFormat.format(shownZoom)}×</output>
            <button class="icon-btn" disabled={!streaming} title={t("ctl.zoomIn")} aria-label={t("ctl.zoomIn")} use:holdRepeat={() => api.zoom(true)}>
              <ZoomIn size={20} />
            </button>
          </div>
        </div>
        {#if streaming}
          <div class="presets">
            {#each zoomPresets as z (z)}
              <button
                class="preset"
                aria-pressed={atZoom(z)}
                aria-label={t("ctl.zoomTo", { v: `${presetFormat.format(z)}×` })}
                onclick={() => api.setZoom(z)}>{presetFormat.format(z)}×</button>
            {/each}
          </div>
        {/if}
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

  .update-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: none;
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 13px;
    background: var(--accent-soft);
    color: var(--accent-text);
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition-property: scale, background-color;
    transition-duration: 150ms;
  }
  .update-chip:hover {
    background: color-mix(in srgb, var(--accent-soft), var(--accent) 10%);
  }
  .update-chip:active {
    scale: 0.96;
  }
  .setup {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
  }
  .setup-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .setup-title {
    font-weight: 600;
  }
  .setup-hint {
    font-size: 12px;
    color: var(--text-2);
    text-wrap: pretty;
  }
  .setup :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      rotate: 360deg;
    }
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
  .row.off .row-text > span:first-child {
    color: var(--text-3);
  }
  .row.zoom {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    padding-block: 6px 8px;
  }
  .zoom-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 32px;
  }
  .zoom .tools {
    align-items: center;
  }
  .zoom-value {
    min-width: 44px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .off .zoom-value {
    color: var(--text-3);
  }
  .presets {
    display: flex;
    gap: 6px;
  }
  .preset {
    min-width: 44px;
    height: 28px;
    padding: 0 10px;
    border-radius: 14px;
    border: 1px solid var(--stroke);
    background: var(--control);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
    transition-property: scale, background-color, color;
    transition-duration: 150ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
  }
  .preset:hover {
    background: var(--control-hover);
  }
  .preset:active {
    scale: 0.96;
  }
  .preset[aria-pressed="true"] {
    border-color: transparent;
    background: var(--accent-soft);
    color: var(--accent-text);
    font-weight: 600;
  }
  .quality-hint {
    margin: 0 2px;
    font-size: 12px;
    color: var(--text-2);
  }
</style>
