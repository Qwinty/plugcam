<script lang="ts">
  // Small live copy of what "Plugcam Camera" shows. Frames come as JPEG over a Tauri channel,
  // and only while the window is visible.
  import { onMount } from "svelte";
  import { subscribePreview, onPreviewClear, setPreviewActive } from "./api";
  import { t } from "./i18n";
  import Icon from "./Icon.svelte";

  let { streaming, cameraOn }: { streaming: boolean; cameraOn: boolean } = $props();

  let canvas: HTMLCanvasElement;
  let hasFrame = $state(false);
  let drawing = false;

  onMount(() => {
    const ctx = canvas.getContext("2d")!;
    subscribePreview(async (jpeg) => {
      if (drawing) return; // drop a frame rather than queue them
      drawing = true;
      try {
        const bitmap = await createImageBitmap(new Blob([jpeg], { type: "image/jpeg" }));
        if (canvas.width !== bitmap.width || canvas.height !== bitmap.height) {
          canvas.width = bitmap.width;
          canvas.height = bitmap.height;
        }
        ctx.drawImage(bitmap, 0, 0);
        bitmap.close();
        hasFrame = true;
      } finally {
        drawing = false;
      }
    });
    const unlisten = onPreviewClear(() => {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      hasFrame = false;
    });
    const onVisibility = () => setPreviewActive(!document.hidden);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      unlisten.then((f) => f());
    };
  });

  $effect(() => {
    if (!cameraOn) hasFrame = false;
  });
</script>

<div class="frame" class:dim={cameraOn && !streaming && hasFrame}>
  <canvas bind:this={canvas} width="640" height="360" class:hidden={!hasFrame}></canvas>
  {#if !hasFrame}
    <div class="empty">
      <Icon name="video" size={28} />
      <span>{cameraOn ? t("preview.waiting") : t("preview.off")}</span>
    </div>
  {/if}
</div>

<style>
  .frame {
    position: relative;
    aspect-ratio: 16 / 9;
    border-radius: 8px;
    overflow: hidden;
    background: var(--preview-bg);
    border: 1px solid var(--stroke);
  }
  canvas {
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
    transition: opacity 0.2s;
  }
  .hidden {
    visibility: hidden;
  }
  .dim canvas {
    opacity: 0.35;
  }
  .empty {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 16px;
    text-align: center;
    color: var(--text-2);
    font-size: 13px;
  }
</style>
