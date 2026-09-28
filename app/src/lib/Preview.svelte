<script lang="ts">
  // Live copy of what "Plugcam Camera" shows. Frames come as JPEG over a Tauri channel,
  // and only while the window is visible.
  import { onMount } from "svelte";
  import { Video, LoaderCircle } from "@lucide/svelte";
  import { subscribePreview, onPreviewClear, setPreviewActive, setPreviewWidth } from "./api";

  let {
    streaming,
    cameraOn,
    live,
    empty,
    waiting,
  }: {
    streaming: boolean;
    cameraOn: boolean;
    /** Chip over the picture while streaming, e.g. "30 fps". */
    live: string;
    /** What the stage says with no picture and the camera off. */
    empty: { title: string; hint: string };
    /** What it says while the camera is on but no frame has come yet. */
    waiting: { title: string; hint: string };
  } = $props();

  let canvas: HTMLCanvasElement;
  let hasFrame = $state(false);
  let drawing = false;

  onMount(() => {
    // A CPU-backed canvas: with GPU decoding of 40 JPEGs a second WebView2's GPU process holds
    // about 500 MB more memory for nothing.
    const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
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

    // Ask for JPEGs as wide as the picture is on screen (it is letterboxed inside the canvas).
    let sentWidth = 0;
    const measure = () => {
      const aspect = canvas.width / canvas.height || 16 / 9;
      const shown = Math.min(canvas.clientWidth, canvas.clientHeight * aspect) * devicePixelRatio;
      const width = Math.ceil(shown / 16) * 16;
      if (width > 0 && width !== sentWidth) {
        sentWidth = width;
        setPreviewWidth(width);
      }
    };
    const sizer = new ResizeObserver(measure);
    sizer.observe(canvas);
    // Moving to a monitor with other scaling keeps the CSS size but changes the pixels.
    let scale = matchMedia(`(resolution: ${devicePixelRatio}dppx)`);
    const onScale = () => {
      scale.removeEventListener("change", onScale);
      scale = matchMedia(`(resolution: ${devicePixelRatio}dppx)`);
      scale.addEventListener("change", onScale);
      measure();
    };
    scale.addEventListener("change", onScale);
    return () => {
      sizer.disconnect();
      scale.removeEventListener("change", onScale);
      document.removeEventListener("visibilitychange", onVisibility);
      unlisten.then((f) => f());
    };
  });

  $effect(() => {
    if (!cameraOn) hasFrame = false;
  });
</script>

<div class="stage" class:dim={cameraOn && !streaming && hasFrame}>
  <canvas bind:this={canvas} width="960" height="540" class:hidden={!hasFrame}></canvas>

  {#if !hasFrame}
    <div class="empty">
      {#if cameraOn}
        <LoaderCircle size={28} class="spin" aria-hidden="true" />
        <span class="title">{waiting.title}</span>
        {#if waiting.hint}<span class="hint">{waiting.hint}</span>{/if}
      {:else}
        <Video size={32} aria-hidden="true" />
        <span class="title">{empty.title}</span>
        {#if empty.hint}<span class="hint">{empty.hint}</span>{/if}
      {/if}
    </div>
  {/if}

  {#if streaming && hasFrame}
    <div class="chip" aria-hidden="true"><span class="dot"></span>{live}</div>
  {/if}
</div>

<style>
  .stage {
    position: relative;
    height: 100%;
    min-height: 0;
    border-radius: 8px;
    overflow: hidden;
    background: var(--stage);
    outline: 1px solid var(--stage-outline);
    outline-offset: -1px;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
    transition-property: opacity;
    transition-duration: 200ms;
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
    padding: 24px;
    text-align: center;
    color: var(--stage-text);
  }
  .title {
    margin-top: 4px;
    font-size: 16px;
    font-weight: 600;
    color: #fff;
    text-wrap: balance;
  }
  .hint {
    max-width: 36ch;
    font-size: 13px;
    text-wrap: pretty;
  }
  .empty :global(.spin) {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      rotate: 360deg;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .empty :global(.spin) {
      animation: none;
    }
  }
  .chip {
    position: absolute;
    inset-block-start: 12px;
    inset-inline-start: 12px;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 10px 0 8px;
    border-radius: 12px;
    background: rgb(0 0 0 / 0.55);
    backdrop-filter: blur(8px);
    color: #fff;
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #f03a2e;
  }
</style>
