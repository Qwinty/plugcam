<script lang="ts">
  // Native select dressed as a Windows combo box, with an optional leading icon.
  import type { Snippet } from "svelte";
  import { ChevronDown } from "@lucide/svelte";

  let {
    value,
    options,
    label,
    icon,
    onchange,
  }: {
    value: string;
    options: { value: string; label: string }[];
    label: string;
    icon?: Snippet;
    onchange: (v: string) => void;
  } = $props();
</script>

<label class="select" title={label}>
  {#if icon}<span class="lead">{@render icon()}</span>{/if}
  <select aria-label={label} {value} onchange={(e) => onchange(e.currentTarget.value)}>
    {#each options as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
  </select>
  <ChevronDown size={16} class="chevron" aria-hidden="true" />
</label>

<style>
  .select {
    position: relative;
    display: flex;
    align-items: center;
    min-width: 0;
    height: 32px;
    border-radius: 6px;
    border: 1px solid var(--stroke);
    background: var(--control);
    color: var(--text-2);
    transition-property: background-color;
    transition-duration: 150ms;
  }
  .select:hover {
    background: var(--control-hover);
  }
  .select:focus-within {
    outline: 2px solid var(--text);
    outline-offset: 1px;
  }
  .lead {
    position: absolute;
    inset-inline-start: 10px;
    display: grid;
    pointer-events: none;
  }
  select {
    appearance: none;
    flex: 1;
    min-width: 0;
    height: 100%;
    padding-inline: 12px 32px;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    text-overflow: ellipsis;
    cursor: pointer;
    outline: none;
  }
  .lead + select {
    padding-inline-start: 36px;
  }
  option {
    background: var(--card-solid);
    color: var(--text);
  }
  .select :global(.chevron) {
    position: absolute;
    inset-inline-end: 10px;
    pointer-events: none;
  }
</style>
