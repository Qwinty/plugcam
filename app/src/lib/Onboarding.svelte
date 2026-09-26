<script lang="ts">
  import * as api from "./api";
  import type { Snapshot } from "./api";
  import { t } from "./i18n";
  import { Check, Info, ExternalLink, LoaderCircle } from "@lucide/svelte";

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

<main class="layout">
  <div class="art" aria-hidden="true">
    <svg viewBox="0 0 240 150">
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
        <rect x="118" y="77" width="23" height="8" rx="3" class="btn-art" />
        <path d="M120 142 V 150" class="cable" />
      {/if}
    </svg>
  </div>

  <div class="page">
    <div class="top">
      <span class="step">{t("wiz.step", { n: step })}</span>
      <button class="text-btn" onclick={finish}>{t("wiz.skip")}</button>
    </div>

    <h1>{t(titles[step - 1])}</h1>
    <p>{t(texts[step - 1])}</p>

    {#if step === 3}
      <div class="live" class:ok={connected}>
        {#if connected}
          <Check size={16} />
          <span>{t("wiz.3.found", { name: snap.device?.name ?? snap.device?.model ?? "" })}</span>
        {:else if unauthorized}
          <Info size={16} />
          <span>{t("status.unauthorized.hint")}</span>
        {:else}
          <LoaderCircle size={16} class="spin" />
          <span>{t("status.noPhone")}</span>
        {/if}
      </div>
      {#if !connected}
        <button class="text-btn ext" onclick={() => api.openUrl("https://developer.android.com/studio/run/win-usb")}>
          {t("wiz.3.driver")} <ExternalLink size={14} />
        </button>
      {/if}
    {:else}
      <p class="why">{t("wiz.why")}</p>
    {/if}

    <div class="nav">
      {#if step > 1}
        <button class="btn" onclick={() => step--}>{t("app.back")}</button>
      {/if}
      <button class="btn accent" onclick={() => (step < 3 ? step++ : finish())}>{step < 3 ? t("wiz.next") : t("wiz.done")}</button>
    </div>
  </div>
</main>

<style>
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 360px;
    height: 100vh;
  }
  .art {
    display: grid;
    place-items: center;
    margin: 12px 0 12px 12px;
    padding: 24px;
    border-radius: 8px;
    background: var(--stage);
    outline: 1px solid var(--stage-outline);
    outline-offset: -1px;
    /* The drawing uses the dark theme's colors on the dark stage. */
    --art-body: #f3f3f3;
    --art-screen: #2d2d2d;
    --art-line: rgb(255 255 255 / 0.35);
    --art-strong: rgb(255 255 255 / 0.85);
    --art-accent: #60cdff;
  }
  .art svg {
    width: min(100%, 480px);
    height: auto;
    max-height: 100%;
  }
  .body {
    fill: var(--art-body);
  }
  .screen {
    fill: var(--art-screen);
  }
  .line {
    fill: var(--art-line);
  }
  .line.strong {
    fill: var(--art-strong);
  }
  .hl {
    fill: rgb(96 205 255 / 0.2);
  }
  .tap {
    fill: var(--art-accent);
    opacity: 0.45;
  }
  .switch,
  .btn-art {
    fill: var(--art-accent);
  }
  .knob {
    fill: #000;
  }
  .dialog {
    fill: rgb(255 255 255 / 0.1);
  }
  .cable {
    stroke: var(--art-body);
    stroke-width: 6;
    stroke-linecap: round;
  }
  .label {
    fill: var(--art-accent);
    font-size: 18px;
    font-weight: 700;
  }
  .page {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 20px 20px;
    overflow-y: auto;
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-inline-end: -6px;
  }
  .step {
    font-size: 12px;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  h1 {
    margin: 8px 0 0;
    font-size: 20px;
    font-weight: 600;
    line-height: 1.2;
    text-wrap: balance;
  }
  p {
    margin: 0;
    line-height: 1.5;
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
  .live :global(.spin) {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      rotate: 360deg;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .live :global(.spin) {
      animation: none;
    }
  }
  .ext {
    align-self: flex-start;
  }
  .nav {
    margin-top: auto;
    display: flex;
    gap: 8px;
  }
  .nav .btn {
    flex: 1;
  }
  @media (max-width: 719px) {
    .layout {
      grid-template-columns: 1fr;
      height: auto;
    }
    .art {
      margin: 12px 12px 0;
      aspect-ratio: 16 / 9;
    }
  }
</style>
