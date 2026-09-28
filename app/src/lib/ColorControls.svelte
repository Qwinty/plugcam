<script lang="ts">
  // "Color" row of the Picture card: four sliders applied on the PC, live while streaming.
  import { ChevronDown, RotateCcw } from "@lucide/svelte";
  import * as api from "./api";
  import type { ColorAdjust } from "./api";
  import { t } from "./i18n";
  import type { Key } from "./locales/en";

  let { value, language }: { value: ColorAdjust; language: string } = $props();

  const NEUTRAL: ColorAdjust = { brightness: 0, contrast: 0, saturation: 0, warmth: 0 };
  const SLIDERS: { key: keyof ColorAdjust; label: Key }[] = [
    { key: "brightness", label: "ctl.color.brightness" },
    { key: "contrast", label: "ctl.color.contrast" },
    { key: "saturation", label: "ctl.color.saturation" },
    { key: "warmth", label: "ctl.color.warmth" },
  ];

  let open = $state(false);
  // What the sliders show: the saved values, or the one being dragged while it is sent.
  let local = $state<ColorAdjust>({ ...NEUTRAL });
  let sending = false;
  let queued: ColorAdjust | null = null;

  $effect(() => {
    const v = value;
    if (!sending) local = { ...v };
  });

  const adjusted = $derived(SLIDERS.some((s) => local[s.key] !== 0));
  const format = $derived(new Intl.NumberFormat(language, { signDisplay: "exceptZero" }));

  // Every change is saved; while one is on its way only the latest waits behind it.
  async function set(next: ColorAdjust) {
    local = next;
    if (sending) {
      queued = next;
      return;
    }
    sending = true;
    try {
      await api.updateSettings({ color: next });
      while (queued) {
        const q: ColorAdjust = queued;
        queued = null;
        await api.updateSettings({ color: q });
      }
    } catch {
      // Not saved: show what is.
      queued = null;
      local = { ...value };
    } finally {
      sending = false;
    }
  }
</script>

<div class="row color">
  <button class="head" aria-expanded={open} onclick={() => (open = !open)}>
    <span class="row-text">
      <span>{t("ctl.color")}</span>
      <span class="row-hint">{adjusted ? t("ctl.color.adjusted") : t("ctl.color.original")}</span>
    </span>
    <span class="chevron" class:open><ChevronDown size={18} /></span>
  </button>

  {#if open}
    <div class="sliders">
      {#each SLIDERS as s (s.key)}
        <label class="slider">
          <span class="name">{t(s.label)}</span>
          <input
            type="range"
            min="-100"
            max="100"
            step="1"
            value={local[s.key]}
            oninput={(e) => set({ ...local, [s.key]: Number(e.currentTarget.value) })}
            ondblclick={() => set({ ...local, [s.key]: 0 })} />
          <output>{format.format(local[s.key])}</output>
        </label>
      {/each}
      <button class="reset" disabled={!adjusted} onclick={() => set({ ...NEUTRAL })}>
        <RotateCcw size={14} />
        {t("ctl.color.reset")}
      </button>
    </div>
  {/if}
</div>

<style>
  .color {
    flex-direction: column;
    align-items: stretch;
    gap: 0;
    padding: 0;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 44px;
    padding: 4px 12px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  .head:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: -2px;
    border-radius: 8px;
  }
  .chevron {
    display: grid;
    color: var(--text-2);
    transition-property: rotate;
    transition-duration: 150ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
  }
  .chevron.open {
    rotate: 180deg;
  }
  .sliders {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 12px 10px;
  }
  /* Name and value on top, the slider full width below: long names in some languages. */
  .slider {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    grid-template-areas: "name value" "range range";
    align-items: center;
    column-gap: 8px;
    font-size: 13px;
  }
  .name {
    grid-area: name;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-2);
  }
  input {
    grid-area: range;
    width: 100%;
    min-width: 0;
    height: 20px;
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }
  output {
    grid-area: value;
    font-variant-numeric: tabular-nums;
  }
  .reset {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    align-self: flex-end;
    height: 28px;
    margin-top: 4px;
    padding: 0 10px;
    border: 1px solid var(--stroke);
    border-radius: 14px;
    background: var(--control);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
    transition-property: scale, background-color;
    transition-duration: 150ms;
  }
  .reset:hover:not(:disabled) {
    background: var(--control-hover);
  }
  .reset:active:not(:disabled) {
    scale: 0.96;
  }
  .reset:disabled {
    color: var(--text-3);
    cursor: default;
  }
</style>
