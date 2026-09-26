<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "./lib/api";
  import type { Snapshot } from "./lib/api";
  import { setLanguage } from "./lib/i18n";
  import Main from "./lib/Main.svelte";
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
</script>

{#if snap}
  {#key snap.language}
    {#if screen === "wizard"}
      <Onboarding {snap} ondone={() => (screen = "main")} />
    {:else if screen === "settings"}
      <Settings {snap} onback={() => (screen = "main")} onwizard={() => (screen = "wizard")} />
    {:else}
      <Main {snap} onsettings={() => (screen = "settings")} />
    {/if}
  {/key}
{/if}
