<script lang="ts">
  // Windows-style switch as a card row, with its label and an optional hint line.
  let {
    checked,
    label,
    hint = "",
    disabled = false,
    onchange,
  }: { checked: boolean; label: string; hint?: string; disabled?: boolean; onchange: (v: boolean) => void } = $props();
</script>

<label class="row" class:disabled>
  <span class="row-text">
    <span>{label}</span>
    {#if hint}<span class="row-hint">{hint}</span>{/if}
  </span>
  <input type="checkbox" role="switch" {checked} {disabled} onchange={(e) => onchange(e.currentTarget.checked)} />
</label>

<style>
  label {
    cursor: pointer;
    padding-inline-end: 12px;
  }
  .disabled {
    color: var(--text-3);
    cursor: default;
  }
  input {
    appearance: none;
    flex: none;
    width: 40px;
    height: 20px;
    margin: 0;
    border-radius: 10px;
    border: 1px solid var(--text-2);
    background: transparent;
    position: relative;
    cursor: pointer;
    transition-property: background-color, border-color;
    transition-duration: 150ms;
  }
  input::after {
    content: "";
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text-2);
    transition-property: translate, background-color;
    transition-duration: 150ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
  }
  input:checked {
    background: var(--accent);
    border-color: var(--accent);
  }
  input:checked::after {
    translate: 20px 0;
    background: var(--on-accent);
  }
  input:disabled {
    border-color: var(--text-3);
    cursor: default;
  }
  input:disabled::after {
    background: var(--text-3);
  }
  input:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
</style>
