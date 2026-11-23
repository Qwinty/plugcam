<script lang="ts">
  // "Crop" row of the Picture card. The crop itself is drawn as a frame on the preview (see
  // CropOverlay); here it can be opened or taken off.
  import { RotateCcw, Crop as CropIcon } from "@lucide/svelte";
  import * as api from "./api";
  import type { Crop } from "./api";
  import { t } from "./i18n";
  import { cropEdit, startCropEdit, NO_CROP } from "./cropEdit.svelte";

  let { value, streaming }: { value: Crop; streaming: boolean } = $props();

  // While the frame is open the saved crop is off; say what the frame does.
  const shown = $derived(cropEdit.active ? cropEdit.draft : value);
  const cropped = $derived(shown.left !== 0 || shown.right !== 0 || shown.top !== 0 || shown.bottom !== 0);
</script>

<div class="row">
  <span class="row-text">
    <span>{t("ctl.crop")}</span>
    <span class="row-hint">{cropped ? t("ctl.crop.on") : t("ctl.crop.none")}</span>
  </span>
  <div class="tools">
    <button
      class="icon-btn"
      title={t("crop.edit")}
      aria-label={t("crop.edit")}
      disabled={!streaming || cropEdit.active}
      onclick={() => startCropEdit(value)}>
      <CropIcon size={20} />
    </button>
    <button
      class="icon-btn"
      title={t("ctl.color.reset")}
      aria-label={t("ctl.color.reset")}
      disabled={!cropped || cropEdit.active}
      onclick={() => api.updateSettings({ crop: NO_CROP })}>
      <RotateCcw size={20} />
    </button>
  </div>
</div>

<style>
  .tools {
    display: flex;
    gap: 4px;
  }
</style>
