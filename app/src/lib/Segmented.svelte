<script lang="ts" generics="T extends string | number">
  // A row of mutually exclusive options (radio group).
  let {
    options,
    value,
    label,
    onchange,
  }: {
    options: { value: T; label: string; title?: string; disabled?: boolean }[];
    value: T;
    label: string;
    onchange: (v: T) => void;
  } = $props();
</script>

<div class="seg" role="radiogroup" aria-label={label}>
  {#each options as o (o.value)}
    <button
      type="button"
      role="radio"
      aria-checked={o.value === value}
      class:on={o.value === value}
      disabled={o.disabled}
      title={o.title}
      onclick={() => o.value !== value && onchange(o.value)}>{o.label}</button>
  {/each}
</div>

<style>
  .seg {
    display: flex;
    padding: 2px;
    gap: 2px;
    border-radius: 8px;
    background: var(--control);
    border: 1px solid var(--stroke);
  }
  button {
    flex: 1;
    min-height: 32px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }
  button:hover:not(:disabled):not(.on) {
    background: var(--control-hover);
  }
  button.on {
    background: var(--card-solid);
    box-shadow: 0 1px 2px var(--shadow);
    font-weight: 600;
  }
  button:disabled {
    color: var(--text-3);
    cursor: default;
  }
  button:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: -2px;
  }
</style>
