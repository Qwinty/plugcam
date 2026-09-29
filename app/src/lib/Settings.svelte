<script lang="ts">
  // Settings in the side panel; the video stays visible next to it.
  import { ArrowLeft, ChevronRight, Languages, LoaderCircle } from "@lucide/svelte";
  import * as api from "./api";
  import type { Snapshot, UpdateView } from "./api";
  import { t, LANGUAGES } from "./i18n";
  import Select from "./Select.svelte";
  import Toggle from "./Toggle.svelte";

  let {
    snap,
    update,
    onback,
    onwizard,
    onupdate,
    onwhatsnew,
  }: {
    snap: Snapshot;
    update: UpdateView;
    onback: () => void;
    onwizard: () => void;
    onupdate: () => void;
    onwhatsnew: () => void;
  } = $props();

  const checking = $derived(update.phase === "checking");
  const updateLine = $derived.by(() => {
    switch (update.phase) {
      case "checking":
        return t("upd.checking");
      case "upToDate":
        return t("upd.upToDate");
      case "available":
      case "downloading":
      case "installing":
        return t("upd.available.short", { v: update.version ?? "" });
      case "error":
        return t("upd.checkFailed");
      default:
        return t("set.version", { v: __APP_VERSION__ });
    }
  });

  let removing = $state(false);
  async function removeCamera() {
    removing = true;
    try {
      await api.setCameraRegistered(false);
    } catch {
      // Declined at the Windows prompt: nothing changed.
    } finally {
      removing = false;
    }
  }

  let reportState = $state<"idle" | "saving" | "saved" | "failed">("idle");
  let reportName = $state("");
  async function saveReport() {
    reportState = "saving";
    try {
      const path = await api.saveReport();
      reportName = path.split(/[\\/]/).pop() ?? path;
      reportState = "saved";
    } catch {
      reportState = "failed";
    }
  }
  const reportLine = $derived(
    reportState === "saved"
      ? t("set.report.saved", { name: reportName })
      : reportState === "failed"
        ? t("set.report.failed")
        : t("set.report.hint"),
  );

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

  const systemName = $derived(LANGUAGES.find((l) => l.code === snap.systemLanguage)?.name ?? "English");
  const languageOptions = $derived([
    { value: "", label: `${t("set.language.system")} · ${systemName}` },
    ...LANGUAGES.map((l) => ({ value: l.code, label: l.name })),
  ]);
</script>

<div class="panel">
  <header>
    <button class="icon-btn" aria-label={t("app.back")} title={t("app.back")} onclick={onback}><ArrowLeft size={20} /></button>
    <h1>{t("app.settings")}</h1>
  </header>

  <section class="section">
    <h2>{t("set.language")}</h2>
    <Select
      label={t("set.language")}
      value={s.language ?? ""}
      onchange={(v) => api.updateSettings({ language: v || null })}
      options={languageOptions}>
      {#snippet icon()}<Languages size={16} />{/snippet}
    </Select>
  </section>

  <section class="section">
    <h2>{t("set.vcam")}</h2>
    <div class="card">
      <div class="row stacked">
        <span class="row-text"><span>{t("set.resolution")}</span><span class="row-hint">{t("set.resolution.hint")}</span></span>
        <Select
          label={t("set.resolution")}
          value={`${s.vcamWidth}x${s.vcamHeight}`}
          onchange={(v) => {
            const [w, h] = v.split("x").map(Number);
            api.updateSettings({ vcamWidth: w, vcamHeight: h });
          }}
          options={sizes.map(([w, h]) => ({ value: `${w}x${h}`, label: `${w} × ${h}` }))} />
      </div>
    </div>
  </section>

  <section class="section">
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

  <section class="section">
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

  <section class="section">
    <h2>{t("set.updates")}</h2>
    <div class="card">
      <Toggle
        label={t("set.checkUpdates")}
        hint={t("set.checkUpdates.hint")}
        checked={s.checkUpdates}
        onchange={(v) => api.updateSettings({ checkUpdates: v })} />
      <div class="row">
        <span class="row-text">
          <span>{t("set.version", { v: __APP_VERSION__ })}{snap.portable ? ` · ${t("set.portable")}` : ""}</span>
          <span class="row-hint" class:accent={update.version && update.phase !== "error"} title={update.error ?? undefined}>{updateLine}</span>
        </span>
        {#if update.version && update.phase !== "checking"}
          <button class="btn accent small" onclick={onupdate}>{t("upd.install")}</button>
        {:else}
          <button class="btn small" disabled={checking} onclick={() => api.checkForUpdates()}>
            {#if checking}<LoaderCircle size={16} class="spin" />{/if}
            {t("upd.checkNow")}
          </button>
        {/if}
      </div>
    </div>
  </section>

  <section class="section">
    <h2>{t("set.diag")}</h2>
    <div class="card">
      <Toggle
        label={t("set.detailedLog")}
        hint={t("set.detailedLog.hint")}
        checked={s.detailedLog}
        onchange={(v) => api.updateSettings({ detailedLog: v })} />
      <div class="row">
        <span class="row-text">
          <span>{t("set.report")}</span>
          <span class="row-hint" class:accent={reportState === "saved"} class:error={reportState === "failed"}>{reportLine}</span>
        </span>
        <button class="btn small" disabled={reportState === "saving"} onclick={saveReport}>
          {#if reportState === "saving"}<LoaderCircle size={16} class="spin" />{/if}
          {t("set.report.save")}
        </button>
      </div>
      <button class="row link" onclick={() => api.openLogFolder()}>
        <span>{t("set.logFolder")}</span>
        <ChevronRight size={16} />
      </button>
    </div>
  </section>

  <section class="section">
    <h2>{t("set.about")}</h2>
    <div class="card">
      <button class="row link first" onclick={onwhatsnew}>
        <span>{t("set.whatsNew")}</span>
        <ChevronRight size={16} />
      </button>
      {#if snap.portable && !snap.cameraMissing}
        <button class="row link middle" disabled={removing} onclick={removeCamera}>
          <span class="row-text">
            <span>{t("set.removeCamera")}</span>
            <span class="row-hint">{t("set.removeCamera.hint")}</span>
          </span>
          {#if removing}<LoaderCircle size={16} class="spin" />{:else}<ChevronRight size={16} />{/if}
        </button>
      {/if}
      <button class="row link" onclick={onwizard}>
        <span>{t("set.rerunWizard")}</span>
        <ChevronRight size={16} />
      </button>
    </div>
  </section>

  <p class="note">{t("set.restartNote")}</p>
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
    gap: 8px;
    margin-inline-start: -6px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    line-height: 1.2;
  }
  .row.stacked {
    flex-direction: column;
    align-items: stretch;
    gap: 8px;
    padding: 12px;
  }
  .slider {
    padding-inline-end: 12px;
  }
  .slider input {
    flex: 1;
    min-width: 0;
    accent-color: var(--accent);
  }
  .value {
    min-width: 76px;
    text-align: end;
    font-variant-numeric: tabular-nums;
  }
  .link {
    width: 100%;
    border: 0;
    border-radius: 0 0 7px 7px;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: start;
    cursor: pointer;
    padding-inline-end: 12px;
    transition-property: background-color;
    transition-duration: 150ms;
  }
  .link.first {
    border-radius: 7px 7px 0 0;
  }
  .link.middle {
    border-radius: 0;
  }
  .link:hover:not(:disabled) {
    background: var(--control-hover);
  }
  .row-hint.accent {
    color: var(--accent-text);
  }
  .row-hint.error {
    color: var(--error);
  }
  .btn.small {
    flex: none;
    min-height: 30px;
    padding: 0 12px;
    font-weight: 400;
  }
  .btn.accent.small {
    font-weight: 600;
  }
  .row :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      rotate: 360deg;
    }
  }
  .link :global(svg) {
    color: var(--text-2);
  }
  .note {
    margin: 0 2px;
    font-size: 12px;
    color: var(--text-2);
    text-wrap: pretty;
  }
</style>
