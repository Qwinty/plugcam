<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "./lib/api";
  import type { Snapshot } from "./lib/api";
  import { setLanguage, t } from "./lib/i18n";
  import { statusInfo } from "./lib/status";
  import Controls from "./lib/Controls.svelte";
  import Preview from "./lib/Preview.svelte";
  import Settings from "./lib/Settings.svelte";
  import Onboarding from "./lib/Onboarding.svelte";

  let snap = $state<Snapshot | null>(null);
  let screen = $state<"main" | "settings" | "wizard">("main");

  function apply(s: Snapshot) {
    setLanguage(s.language);
    document.documentElement.classList.toggle("mica", s.mica);
    if (!snap && !s.settings.onboardingDone) screen = "wizard";
    snap = s;
  }

  onMount(() => {
    const unlisten = api.onState(apply);
    api.getState().then(apply);
    return () => {
      unlisten.then((f) => f());
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
            <Settings {snap} onback={() => (screen = "main")} onwizard={() => (screen = "wizard")} />
          {:else}
            <Controls {snap} onsettings={() => (screen = "settings")} />
          {/if}
        </aside>
      </main>
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
