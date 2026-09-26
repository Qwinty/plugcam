<script lang="ts">
  // Windows-style switch with its label and an optional hint line.
  let {
    checked,
    label,
    hint = "",
    onchange,
  }: { checked: boolean; label: string; hint?: string; onchange: (v: boolean) => void } = $props();
</script>

<label class="row">
  <span class="text">
    <span>{label}</span>
    {#if hint}<span class="hint">{hint}</span>{/if}
  </span>
  <input type="checkbox" role="switch" {checked} onchange={(e) => onchange(e.currentTarget.checked)} />
</label>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 16px;
    cursor: pointer;
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
    transition: background 0.15s, border-color 0.15s;
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
    transition: transform 0.15s, background 0.15s;
  }
  input:checked {
    background: var(--accent);
    border-color: var(--accent);
  }
  input:checked::after {
    transform: translateX(20px);
    background: var(--on-accent);
  }
  input:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
</style>
