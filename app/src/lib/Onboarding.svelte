<script lang="ts">
  import * as api from "./api";
  import type { Snapshot } from "./api";
  import { t } from "./i18n";
  import Icon from "./Icon.svelte";

  let { snap, ondone }: { snap: Snapshot; ondone: () => void } = $props();

  let step = $state(1);
  const connected = $derived(snap.device?.state === "device");
  const unauthorized = $derived(snap.device?.state === "unauthorized");
  const titles = ["wiz.1.title", "wiz.2.title", "wiz.3.title"] as const;
  const texts = ["wiz.1.text", "wiz.2.text", "wiz.3.text"] as const;

  function finish() {
    api.updateSettings({ onboardingDone: true });
    ondone();
  }
</script>

<div class="page">
  <div class="top">
    <span class="step">{t("wiz.step", { n: step })}</span>
    <button class="text-btn" onclick={finish}>{t("wiz.skip")}</button>
  </div>

  <div class="art" aria-hidden="true">
    <svg viewBox="0 0 240 150" width="240" height="150">
      <!-- phone -->
      <rect x="84" y="8" width="72" height="134" rx="12" class="body" />
      <rect x="90" y="18" width="60" height="114" rx="5" class="screen" />
      {#if step === 1}
        {#each [0, 1, 2, 3] as i}
          <rect x="95" y={26 + i * 14} width="50" height="8" rx="2" class="line" />
        {/each}
        <rect x="93" y="84" width="54" height="14" rx="3" class="hl" />
        <rect x="96" y="89" width="34" height="4" rx="2" class="line strong" />
        <circle cx="136" cy="91" r="9" class="tap" />
        <text x="170" y="96" class="label">×7</text>
      {:else if step === 2}
        {#each [0, 1, 2] as i}
          <rect x="95" y={26 + i * 14} width="50" height="8" rx="2" class="line" />
        {/each}
        <rect x="93" y="70" width="54" height="16" rx="3" class="hl" />
        <rect x="96" y="76" width="26" height="4" rx="2" class="line strong" />
        <rect x="126" y="73" width="17" height="10" rx="5" class="switch" />
        <circle cx="138" cy="78" r="3.5" class="knob" />
        <rect x="95" y="94" width="50" height="8" rx="2" class="line" />
      {:else}
        <rect x="95" y="52" width="50" height="38" rx="5" class="dialog" />
        <rect x="100" y="59" width="40" height="4" rx="2" class="line strong" />
        <rect x="100" y="67" width="30" height="4" rx="2" class="line" />
        <rect x="118" y="77" width="23" height="8" rx="3" class="btn" />
        <path d="M120 142 V 150" class="cable" />
      {/if}
    </svg>
  </div>

  <h1>{t(titles[step - 1])}</h1>
  <p>{t(texts[step - 1])}</p>

  {#if step === 3}
    <div class="live" class:ok={connected}>
      {#if connected}
        <Icon name="check" />
        <span>{t("wiz.3.found", { name: snap.device?.name ?? snap.device?.model ?? "" })}</span>
      {:else if unauthorized}
        <Icon name="info" />
        <span>{t("status.unauthorized.hint")}</span>
      {:else}
        <span class="spinner"></span>
        <span>{t("status.noPhone")}</span>
      {/if}
    </div>
    {#if !connected}
      <button class="text-btn ext" onclick={() => api.openUrl("https://developer.android.com/studio/run/win-usb")}>
        {t("wiz.3.driver")} <Icon name="external" size={14} />
      </button>
    {/if}
  {:else}
    <p class="why">{t("wiz.why")}</p>
  {/if}

  <div class="nav">
    {#if step > 1}
      <button class="secondary" onclick={() => step--}>{t("app.back")}</button>
    {/if}
    <button class="primary" onclick={() => (step < 3 ? step++ : finish())}>{step < 3 ? t("wiz.next") : t("wiz.done")}</button>
  </div>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    min-height: 100vh;
    box-sizing: border-box;
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .step {
    font-size: 12px;
    color: var(--text-2);
  }
  .art {
    display: grid;
    place-items: center;
    padding: 12px 0;
    border-radius: 8px;
    background: var(--card);
    border: 1px solid var(--stroke);
  }
  .body {
    fill: var(--text);
    opacity: 0.85;
  }
  .screen {
    fill: var(--card-solid);
  }
  .line {
    fill: var(--text-3);
    opacity: 0.6;
  }
  .line.strong {
    fill: var(--text);
    opacity: 0.8;
  }
  .hl {
    fill: var(--accent-soft);
  }
  .tap {
    fill: var(--accent);
    opacity: 0.35;
  }
  .switch,
  .btn {
    fill: var(--accent);
  }
  .knob {
    fill: var(--on-accent);
  }
  .dialog {
    fill: var(--control-hover);
  }
  .cable {
    stroke: var(--text);
    stroke-width: 6;
    stroke-linecap: round;
    opacity: 0.85;
  }
  .label {
    fill: var(--accent-text);
    font-size: 18px;
    font-weight: 700;
  }
  h1 {
    margin: 4px 0 0;
    font-size: 20px;
    font-weight: 600;
  }
  p {
    margin: 0;
    line-height: 1.45;
    text-wrap: pretty;
  }
  .why {
    font-size: 12px;
    color: var(--text-2);
  }
  .live {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--control);
    border: 1px solid var(--stroke);
  }
  .live.ok {
    background: var(--ok-soft);
    border-color: transparent;
  }
  .spinner {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--text-3);
    border-top-color: var(--accent);
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
    }
  }
  .ext {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .nav {
    margin-top: auto;
    display: flex;
    gap: 8px;
  }
  .nav button {
    flex: 1;
  }
  .primary,
  .secondary {
    min-height: 40px;
    border-radius: 8px;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .primary {
    border: 0;
    background: var(--accent);
    color: var(--on-accent);
  }
  .primary:hover {
    background: var(--accent-hover);
  }
  .secondary {
    border: 1px solid var(--stroke);
    background: var(--control);
    color: var(--text);
  }
  .secondary:hover {
    background: var(--control-hover);
  }
</style>
