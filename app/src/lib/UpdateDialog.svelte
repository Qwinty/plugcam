<script lang="ts">
  // A modal over the window: either "an update is available" (notes, install, progress) or
  // "what's new" after an update (the notes of every version since the one seen last).
  import { onMount } from "svelte";
  import { Download, LoaderCircle, PartyPopper, CircleAlert } from "@lucide/svelte";
  import * as api from "./api";
  import type { UpdateView } from "./api";
  import { t } from "./i18n";
  import type { Release } from "./releaseNotes";
  import { notesFor } from "./releaseNotes";
  import Markdown from "./Markdown.svelte";

  let {
    mode,
    update,
    releases = [],
    language,
    onclose,
  }: { mode: "update" | "whatsNew"; update: UpdateView; releases?: Release[]; language: string; onclose: () => void } =
    $props();

  let dialog: HTMLDialogElement;
  onMount(() => dialog.showModal());

  const working = $derived(update.phase === "downloading" || update.phase === "installing");
  const percent = $derived(update.progress === null ? null : Math.round(update.progress * 100));

  function close() {
    if (!working) onclose();
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="update-title"
  oncancel={(e) => {
    e.preventDefault();
    close();
  }}>
  {#if mode === "whatsNew"}
    <header>
      <span class="badge"><PartyPopper size={20} /></span>
      <h2 id="update-title">{t("upd.whatsNew.title", { v: __APP_VERSION__ })}</h2>
    </header>
    <div class="notes">
      {#each releases as r, i}
        {#if releases.length > 1}<h3 class="version" class:first={i === 0}>{t("set.version", { v: r.version })}</h3>{/if}
        <Markdown source={r.notes} />
      {/each}
    </div>
    <footer>
      <button class="btn accent" onclick={onclose}>{t("upd.gotIt")}</button>
    </footer>
  {:else}
    <header>
      <span class="badge"><Download size={20} /></span>
      <h2 id="update-title">{t("upd.available.title", { v: update.version ?? "" })}</h2>
    </header>
    <div class="notes">
      {#if update.notes}
        <Markdown source={notesFor(update.notes, language)} />
      {:else}
        <p class="muted">{t("upd.noNotes")}</p>
      {/if}
    </div>
    {#if update.phase === "error"}
      <p class="error" role="alert"><CircleAlert size={16} /><span>{t("upd.failed", { message: update.error ?? "" })}</span></p>
    {/if}
    {#if working}
      <div class="progress" role="status">
        <div class="bar" class:indeterminate={percent === null || update.phase === "installing"}>
          <span style:width={percent === null ? null : `${percent}%`}></span>
        </div>
        <span class="muted">
          {#if update.phase === "installing"}{t(update.portable ? "upd.restarting" : "upd.installing")}{:else if percent !== null}{t(
              "upd.downloadingPct",
              { p: percent },
            )}{:else}{t("upd.downloading")}{/if}
        </span>
      </div>
    {:else if !update.portable}
      <p class="muted">{t("upd.adminNote")}</p>
    {/if}
    <footer>
      <button class="btn" disabled={working} onclick={close}>{t("upd.later")}</button>
      <button class="btn accent" disabled={working} onclick={() => api.installUpdate().catch(() => {})}>
        {#if working}<LoaderCircle size={18} class="spin" />{/if}
        {t(update.phase === "error" ? "upd.retry" : "upd.install")}
      </button>
    </footer>
  {/if}
</dialog>

<style>
  dialog {
    width: min(520px, calc(100vw - 32px));
    max-height: min(640px, calc(100vh - 48px));
    padding: 0;
    border: 1px solid var(--stroke);
    border-radius: 8px;
    background: var(--card-solid);
    color: var(--text);
    box-shadow: 0 16px 48px var(--shadow);
    overflow: hidden;
  }
  dialog[open] {
    display: flex;
    flex-direction: column;
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 0.3);
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 20px 24px 12px;
  }
  .badge {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent-text);
    flex: none;
  }
  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    line-height: 1.25;
    text-wrap: balance;
  }
  .notes {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 24px 12px;
  }
  .version {
    margin: 18px 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }
  .version.first {
    margin-top: 0;
  }
  .muted {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .error {
    display: flex;
    gap: 8px;
    margin: 0 24px 8px;
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--error-soft);
    color: var(--error);
    font-size: 12px;
  }
  .error :global(svg) {
    flex: none;
    margin-top: 1px;
  }
  .progress,
  dialog > .muted {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 24px 8px;
  }
  .bar {
    position: relative;
    height: 4px;
    border-radius: 2px;
    background: var(--control-hover);
    overflow: hidden;
  }
  .bar span {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 2px;
    background: var(--accent);
    transition: width 150ms linear;
  }
  .bar.indeterminate span {
    width: 30%;
    animation: slide 1.2s cubic-bezier(0.4, 0, 0.2, 1) infinite;
  }
  @keyframes slide {
    from {
      left: -30%;
    }
    to {
      left: 100%;
    }
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 16px 24px 20px;
    border-top: 1px solid var(--divider);
    background: var(--bg);
  }
  footer .btn {
    min-width: 96px;
  }
  footer :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      rotate: 360deg;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .bar.indeterminate span,
    footer :global(.spin) {
      animation-duration: 3s;
    }
  }
</style>
