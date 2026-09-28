<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "./lib/api";
  import type { Snapshot, UpdateView } from "./lib/api";
  import { setLanguage, t } from "./lib/i18n";
  import { statusInfo } from "./lib/status";
  import Controls from "./lib/Controls.svelte";
  import Preview from "./lib/Preview.svelte";
  import Settings from "./lib/Settings.svelte";
  import Onboarding from "./lib/Onboarding.svelte";
  import UpdateDialog from "./lib/UpdateDialog.svelte";
  import { releasesSince, type Release } from "./lib/releaseNotes";

  let snap = $state<Snapshot | null>(null);
  let screen = $state<"main" | "settings" | "wizard">("main");
  let update = $state<UpdateView>({ phase: "idle", version: null, notes: null, progress: null, error: null, portable: false });
  let dialog = $state<{ mode: "update" } | { mode: "whatsNew"; releases: Release[] } | null>(null);

  function apply(s: Snapshot) {
    setLanguage(s.language);
    document.documentElement.classList.toggle("mica", s.mica);
    if (!snap) firstSnapshot(s);
    snap = s;
  }

  function firstSnapshot(s: Snapshot) {
    if (!s.settings.onboardingDone) screen = "wizard";
    const seen = s.settings.lastSeenVersion;
    if (seen === __APP_VERSION__) return;
    // A fresh install has nothing to compare with; an update shows what changed since.
    if (seen) {
      const releases = releasesSince(seen, __APP_VERSION__, s.language);
      if (releases.length) dialog = { mode: "whatsNew", releases };
    }
    api.updateSettings({ lastSeenVersion: __APP_VERSION__ });
  }

  function showWhatsNew() {
    if (!snap) return;
    const releases = releasesSince("0.0.0", __APP_VERSION__, snap.language).slice(0, 5);
    dialog = { mode: "whatsNew", releases };
  }

  onMount(() => {
    const unlisten = [api.onState(apply), api.onUpdate((u) => (update = u))];
    api.getState().then(apply);
    api.updateState().then((u) => (update = u));
    return () => {
      for (const u of unlisten) u.then((f) => f());
    };
  });

  // With the camera off the stage repeats what to do next, so the empty space is useful.
  function emptyStage(s: Snapshot) {
    if (s.device?.state === "device" && !s.problem) return { title: t("preview.off"), hint: t("preview.off.hint") };
    const info = statusInfo(s);
    return { title: info.title, hint: info.hint };
  }

  function waitingStage(s: Snapshot) {
    const kind = s.status?.kind;
    if (s.device?.state === "device" && kind !== "error" && kind !== "noPicture") return { title: t("preview.waiting"), hint: "" };
    const info = statusInfo(s);
    return { title: info.title, hint: info.hint };
  }
</script>

{#if snap}
  {#key snap.language}
    {#if screen === "wizard"}
      <Onboarding {snap} ondone={() => (screen = "main")} />
    {:else}
      <main class="layout">
        <div class="video">
          <Preview
            cameraOn={snap.cameraOn}
            streaming={snap.cameraOn && snap.status?.kind === "streaming"}
            live={snap.fps > 0 ? t("live.fps", { fps: Math.round(snap.fps) }) : t("live")}
            empty={emptyStage(snap)}
            waiting={waitingStage(snap)} />
          <p class="apps">{t("preview.inApps")}</p>
        </div>
        <aside>
          {#if screen === "settings"}
            <Settings
              {snap}
              {update}
              onback={() => (screen = "main")}
              onwizard={() => (screen = "wizard")}
              onupdate={() => (dialog = { mode: "update" })}
              onwhatsnew={showWhatsNew} />
          {:else}
            <Controls {snap} {update} onsettings={() => (screen = "settings")} onupdate={() => (dialog = { mode: "update" })} />
          {/if}
        </aside>
      </main>
    {/if}
    {#if dialog}
      <UpdateDialog
        mode={dialog.mode}
        {update}
        releases={dialog.mode === "whatsNew" ? dialog.releases : []}
        language={snap.language}
        onclose={() => (dialog = null)} />
    {/if}
  {/key}
{/if}

<style>
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 320px;
    height: 100vh;
  }
  .video {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    padding: 12px 0 12px 12px;
  }
  .video > :global(.stage) {
    flex: 1;
  }
  .apps {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
    text-align: center;
    text-wrap: balance;
  }
  aside {
    overflow-y: auto;
    padding: 16px 16px 16px 20px;
    scrollbar-gutter: stable;
  }
  /* Narrow window: video on top, controls below. */
  @media (max-width: 719px) {
    .layout {
      grid-template-columns: 1fr;
      height: auto;
    }
    .video {
      padding: 12px 12px 0;
    }
    .video > :global(.stage) {
      flex: none;
      aspect-ratio: 16 / 9;
      height: auto;
    }
    aside {
      overflow: visible;
      padding: 16px 12px;
    }
  }
</style>
