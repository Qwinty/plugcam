<script lang="ts">
  import * as api from "./api";
  import type { Snapshot } from "./api";
  import { t } from "./i18n";
  import Icon from "./Icon.svelte";
  import Segmented from "./Segmented.svelte";
  import Toggle from "./Toggle.svelte";

  let { snap, onback, onwizard }: { snap: Snapshot; onback: () => void; onwizard: () => void } = $props();

  const s = $derived(snap.settings);
  const sizes = [
    [1280, 720],
    [1920, 1080],
    [2560, 1440],
    [3840, 2160],
  ] as const;

  // The slider only sends its value when released, so dragging does not restart the stream.
  let bitrateDraft = $state<number | null>(null);
  const bitrate = $derived(bitrateDraft ?? s.bitrateMbps ?? 12);
</script>

<div class="page">
  <header>
    <button class="icon-btn" aria-label={t("app.back")} title={t("app.back")} onclick={onback}><Icon name="back" /></button>
    <h1>{t("app.settings")}</h1>
  </header>

  <section>
    <h2>{t("set.vcam")}</h2>
    <div class="card">
      <div class="row">
        <span class="text"><span>{t("set.resolution")}</span><span class="hint">{t("set.resolution.hint")}</span></span>
        <select
          value={`${s.vcamWidth}x${s.vcamHeight}`}
          onchange={(e) => {
            const [w, h] = e.currentTarget.value.split("x").map(Number);
            api.updateSettings({ vcamWidth: w, vcamHeight: h });
          }}>
          {#each sizes as [w, h]}<option value={`${w}x${h}`}>{w}×{h}</option>{/each}
        </select>
      </div>
      <div class="row column">
        <span class="text"><span>{t("set.rotation")}</span><span class="hint">{t("set.rotation.hint")}</span></span>
        <Segmented
          label={t("set.rotation")}
          value={s.rotation}
          onchange={(r) => api.updateSettings({ rotation: r })}
          options={[0, 90, 180, 270].map((r) => ({ value: r as 0 | 90 | 180 | 270, label: `${r}°` }))} />
      </div>
    </div>
  </section>

  <section>
    <h2>{t("set.video")}</h2>
    <div class="card">
      <Toggle
        label={`${t("set.bitrate")}: ${t("set.bitrate.auto")}`}
        hint={t("set.bitrate.hint")}
        checked={s.bitrateMbps === null}
        onchange={(auto) => api.updateSettings({ bitrateMbps: auto ? null : 12 })} />
      {#if s.bitrateMbps !== null}
        <div class="row slider">
          <input
            type="range"
            min="4"
            max="40"
            step="1"
            value={bitrate}
            aria-label={t("set.bitrate")}
            oninput={(e) => (bitrateDraft = Number(e.currentTarget.value))}
            onchange={(e) => {
              api.updateSettings({ bitrateMbps: Number(e.currentTarget.value) });
              bitrateDraft = null;
            }} />
          <span class="value">{t("set.mbps", { v: bitrate })}</span>
        </div>
      {/if}
    </div>
  </section>

  <section>
    <h2>{t("set.startup")}</h2>
    <div class="card">
      <Toggle label={t("set.launchAtLogin")} checked={s.launchAtLogin} onchange={(v) => api.updateSettings({ launchAtLogin: v })} />
      <Toggle
        label={t("set.closeToTray")}
        hint={t("set.closeToTray.hint")}
        checked={s.closeToTray}
        onchange={(v) => api.updateSettings({ closeToTray: v })} />
      <Toggle label={t("set.autoStart")} checked={s.autoStartCamera} onchange={(v) => api.updateSettings({ autoStartCamera: v })} />
    </div>
  </section>

  <section>
    <h2>{t("set.about")}</h2>
    <div class="card">
      <div class="row"><span>Plugcam</span><span class="hint">{t("set.version", { v: __APP_VERSION__ })}</span></div>
      <button class="row link" onclick={onwizard}>{t("set.rerunWizard")}</button>
    </div>
  </section>

  <p class="note">{t("set.restartNote")}</p>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 12px 16px 16px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h2 {
    margin: 0 4px;
    font-size: 13px;
    font-weight: 600;
  }
  .card {
    padding: 0;
  }
  .card > :global(* + *) {
    border-top: 1px solid var(--stroke);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 16px;
  }
  .row.column {
    flex-direction: column;
    align-items: stretch;
    gap: 8px;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hint {
    font-size: 12px;
    color: var(--text-2);
  }
  select {
    min-height: 32px;
    padding: 0 8px;
    border-radius: 6px;
    border: 1px solid var(--stroke);
    background: var(--control);
    color: var(--text);
    font: inherit;
  }
  .slider input {
    flex: 1;
    accent-color: var(--accent);
  }
  .value {
    min-width: 80px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .link {
    width: 100%;
    border: 0;
    background: none;
    color: var(--accent-text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .link:hover {
    background: var(--control-hover);
  }
  .note {
    margin: 0 4px;
    font-size: 12px;
    color: var(--text-2);
  }
</style>
