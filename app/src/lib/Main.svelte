<script lang="ts">
  import * as api from "./api";
  import type { Snapshot, CameraView, Quality } from "./api";
  import { t } from "./i18n";
  import { statusInfo } from "./status";
  import Icon from "./Icon.svelte";
  import Preview from "./Preview.svelte";
  import Segmented from "./Segmented.svelte";

  let { snap, onsettings }: { snap: Snapshot; onsettings: () => void } = $props();

  let busy = $state<"starting" | "stopping" | null>(null);

  const info = $derived(statusInfo(snap));
  const s = $derived(snap.settings);
  const streaming = $derived(snap.cameraOn && snap.status?.kind === "streaming");
  const lenses = $derived(snap.cameras.filter((c) => c.facing === s.facing));
  const ready = $derived(snap.device?.state === "device" && !snap.problem);

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

  const switchFacing = () => api.updateSettings({ facing: s.facing === "back" ? "front" : "back", cameraId: null });
</script>

<div class="page">
  <header class="card phone">
    <div class="badge"><Icon name="phone" /></div>
    <div class="info">
      <div class="name">
        {snap.device?.name ?? snap.device?.model ?? "Plugcam"}
        {#if snap.device}
          <span class="conn"><Icon name={snap.device.wifi ? "wifi" : "usb"} size={14} />{t(snap.device.wifi ? "conn.wifi" : "conn.usb")}</span>
        {/if}
      </div>
      <div class="status" role="status">
        <span class="dot {info.tone}"></span>
        <span>{info.title}</span>
      </div>
      {#if info.hint}<div class="hint">{info.hint}</div>{/if}
    </div>
    <button class="icon-btn" title={t("app.settings")} aria-label={t("app.settings")} onclick={onsettings}>
      <Icon name="gear" />
    </button>
  </header>

  <Preview {streaming} cameraOn={snap.cameraOn} />

  <button
    class="primary"
    class:stop={snap.cameraOn}
    disabled={busy !== null || (!snap.cameraOn && !!snap.problem)}
    onclick={toggleCamera}>
    {#if busy === "starting"}{t("camera.starting")}{:else if busy === "stopping"}{t("camera.stopping")}{:else if snap.cameraOn}{t("camera.off")}{:else}{t("camera.on")}{/if}
  </button>

  {#if snap.notice?.kind === "lensHidden"}
    <div class="notice" role="alert">
      <Icon name="info" />
      <span>{t("notice.lensHidden", { id: snap.notice.id })}</span>
      <button class="icon-btn small" aria-label={t("notice.close")} onclick={api.dismissNotice}><Icon name="close" size={16} /></button>
    </div>
  {/if}

  <div class="controls">
    <button
      class="tool"
      title={t("ctl.switch", { to: t(s.facing === "back" ? "ctl.front" : "ctl.back") })}
      aria-label={t("ctl.switch", { to: t(s.facing === "back" ? "ctl.front" : "ctl.back") })}
      onclick={switchFacing}><Icon name="switch" /></button>

    {#if lenses.length > 1}
      <label class="lens" title={t("ctl.lens")}>
        <Icon name="lens" size={18} />
        <select
          aria-label={t("ctl.lens")}
          value={snap.selectedCamera ?? ""}
          onchange={(e) => api.updateSettings({ cameraId: e.currentTarget.value || null })}>
          {#each lenses as c, i (c.id)}
            <option value={c.id}>{lensLabel(c, i)}</option>
          {/each}
        </select>
      </label>
    {:else}
      <span class="spacer"></span>
    {/if}

    <button
      class="tool"
      aria-pressed={s.mirror}
      title={t("ctl.mirror")}
      aria-label={t("ctl.mirror")}
      onclick={() => api.updateSettings({ mirror: !s.mirror })}><Icon name="mirror" /></button>
    <button
      class="tool"
      aria-pressed={snap.torch}
      disabled={!streaming || s.facing !== "back"}
      title={t("ctl.torch")}
      aria-label={t("ctl.torch")}
      onclick={() => api.setTorch(!snap.torch)}><Icon name="torch" /></button>
    <button class="tool" disabled={!streaming} title={t("ctl.zoomOut")} aria-label={t("ctl.zoomOut")} onclick={() => api.zoom(false)}
      ><Icon name="zoomOut" /></button>
    <button class="tool" disabled={!streaming} title={t("ctl.zoomIn")} aria-label={t("ctl.zoomIn")} onclick={() => api.zoom(true)}
      ><Icon name="zoomIn" /></button>
  </div>

  <Segmented
    label="Quality"
    value={s.quality}
    onchange={(q: Quality) => api.updateSettings({ quality: q })}
    options={[
      { value: "economy", label: t("quality.economy"), title: t("quality.economy.hint") },
      { value: "standard", label: t("quality.standard"), title: t("quality.standard.hint") },
      {
        value: "smooth",
        label: t("quality.smooth"),
        title: snap.smoothAvailable || !ready ? t("quality.smooth.hint") : t("quality.smooth.na"),
        disabled: ready && snap.cameras.length > 0 && !snap.smoothAvailable,
      },
    ]} />

  <p class="apps">{t("preview.inApps")}</p>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
  }
  .phone {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 8px 12px 12px;
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
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .conn {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 12px;
    font-weight: 400;
    color: var(--text-2);
  }
  .status {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .hint {
    font-size: 12px;
    color: var(--text-2);
    text-wrap: pretty;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .dot.ok {
    background: var(--ok);
  }
  .dot.live {
    background: var(--ok);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 30%, transparent);
  }
  .dot.busy {
    background: var(--accent);
  }
  .dot.warn {
    background: var(--warn);
  }
  .dot.error {
    background: var(--error);
  }

  .primary {
    min-height: 44px;
    border: 0;
    border-radius: 8px;
    background: var(--accent);
    color: var(--on-accent);
    font: inherit;
    font-size: 15px;
    font-weight: 600;
    cursor: pointer;
  }
  .primary:hover:not(:disabled) {
    background: var(--accent-hover);
  }
  .primary.stop {
    background: var(--control);
    color: var(--text);
    border: 1px solid var(--stroke);
  }
  .primary.stop:hover:not(:disabled) {
    background: var(--control-hover);
  }
  .primary:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .notice {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 8px 10px 12px;
    border-radius: 8px;
    background: var(--warn-soft);
    font-size: 13px;
  }
  .notice span {
    flex: 1;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .spacer {
    flex: 1;
  }
  .tool {
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 8px;
    border: 1px solid var(--stroke);
    background: var(--control);
    color: var(--text);
    cursor: pointer;
  }
  .tool:hover:not(:disabled) {
    background: var(--control-hover);
  }
  .tool[aria-pressed="true"] {
    background: var(--accent-soft);
    color: var(--accent-text);
    border-color: transparent;
  }
  .tool:disabled {
    color: var(--text-3);
    cursor: default;
  }
  .lens {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 40px;
    padding: 0 8px;
    border-radius: 8px;
    border: 1px solid var(--stroke);
    background: var(--control);
    color: var(--text-2);
  }
  .lens select {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
    outline: none;
  }
  .lens:focus-within {
    outline: 2px solid var(--text);
    outline-offset: 1px;
  }
  .apps {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
    text-align: center;
    text-wrap: balance;
  }
</style>
