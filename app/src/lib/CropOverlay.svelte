<script lang="ts">
  // The crop frame over the preview, as in OBS: drag it to move, drag a corner to resize. It
  // keeps the camera frame's aspect ratio, so what is kept fills the frame without bars; Shift
  // frees it. Positions are fractions of the picture as shown (the preview shows the whole
  // picture while this is open).
  import { onMount } from "svelte";
  import { cropEdit, finishCropEdit } from "./cropEdit.svelte";
  import { t } from "./i18n";

  let {
    frameWidth,
    frameHeight,
    pictureWidth,
    pictureHeight,
  }: {
    /** The camera frame (the preview JPEG has its shape). */
    frameWidth: number;
    frameHeight: number;
    /** The phone's picture as shown, i.e. after rotation; 0 while unknown. */
    pictureWidth: number;
    pictureHeight: number;
  } = $props();

  /** Smallest frame: a tenth of the picture. */
  const MIN = 0.1;

  let boxWidth = $state(0);
  let boxHeight = $state(0);

  // Where the picture is on screen: the frame is letterboxed in the stage, and the picture
  // in the frame.
  const area = $derived.by(() => {
    if (!boxWidth || !boxHeight || !frameWidth || !frameHeight) return null;
    const scale = Math.min(boxWidth / frameWidth, boxHeight / frameHeight);
    const fx = (boxWidth - frameWidth * scale) / 2;
    const fy = (boxHeight - frameHeight * scale) / 2;
    const pw = pictureWidth || frameWidth;
    const ph = pictureHeight || frameHeight;
    const fit = Math.min(frameWidth / pw, frameHeight / ph);
    const w = pw * fit * scale;
    const h = ph * fit * scale;
    return { x: fx + (frameWidth * scale - w) / 2, y: fy + (frameHeight * scale - h) / 2, w, h };
  });

  /** Height of the frame in picture fractions per unit of width, at the frame's aspect ratio. */
  const k = $derived(
    pictureWidth && pictureHeight ? (pictureWidth * frameHeight) / (pictureHeight * frameWidth) : 1,
  );

  type Rect = { x0: number; y0: number; x1: number; y1: number };
  const rect = $derived<Rect>({
    x0: cropEdit.draft.left / 100,
    y0: cropEdit.draft.top / 100,
    x1: 1 - cropEdit.draft.right / 100,
    y1: 1 - cropEdit.draft.bottom / 100,
  });

  function save(r: Rect) {
    const c = (v: number) => Math.min(100, Math.max(0, v * 100));
    cropEdit.draft = { left: c(r.x0), top: c(r.y0), right: c(1 - r.x1), bottom: c(1 - r.y1) };
  }

  /** The widest frame of the right shape that fits, keeping the old one's center. */
  function locked(r: Rect): Rect {
    const maxW = Math.min(1, 1 / k);
    const w = Math.min(maxW, Math.max(MIN, r.x1 - r.x0));
    const h = w * k;
    const cx = Math.min(1 - w / 2, Math.max(w / 2, (r.x0 + r.x1) / 2));
    const cy = Math.min(1 - h / 2, Math.max(h / 2, (r.y0 + r.y1) / 2));
    return { x0: cx - w / 2, x1: cx + w / 2, y0: cy - h / 2, y1: cy + h / 2 };
  }

  // A crop made with the sliders may have any shape: start from one of the frame's shape,
  // unless it is already close. Again whenever the picture changes shape (rotation).
  let shapedFor = 0;
  $effect(() => {
    if (shapedFor === k || !area || !pictureWidth) return;
    shapedFor = k;
    const r = rect;
    const w = r.x1 - r.x0;
    const h = r.y1 - r.y0;
    if (Math.abs(h - w * k) > 0.01 * Math.max(h, w * k)) save(locked(r));
  });

  type Drag = { kind: "move" | "corner"; startX: number; startY: number; start: Rect; ax: number; ay: number; sx: number; sy: number };
  let drag: Drag | null = null;

  /** Pointer position in picture fractions. */
  function at(e: PointerEvent) {
    const a = area!;
    const box = (e.currentTarget as HTMLElement).closest(".crop-overlay")!.getBoundingClientRect();
    return { x: (e.clientX - box.left - a.x) / a.w, y: (e.clientY - box.top - a.y) / a.h };
  }

  function down(e: PointerEvent, kind: "move" | "corner", sx = 0, sy = 0) {
    if (!area || e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const p = at(e);
    const r = rect;
    // The corner opposite to the one dragged stays put.
    const ax = sx > 0 ? r.x0 : r.x1;
    const ay = sy > 0 ? r.y0 : r.y1;
    drag = { kind, startX: p.x, startY: p.y, start: { ...r }, ax, ay, sx, sy };
  }

  function move(e: PointerEvent) {
    if (!drag || !area) return;
    const p = at(e);
    const d = drag;
    if (d.kind === "move") {
      const w = d.start.x1 - d.start.x0;
      const h = d.start.y1 - d.start.y0;
      const x0 = Math.min(1 - w, Math.max(0, d.start.x0 + p.x - d.startX));
      const y0 = Math.min(1 - h, Math.max(0, d.start.y0 + p.y - d.startY));
      save({ x0, y0, x1: x0 + w, y1: y0 + h });
      return;
    }
    // Room from the fixed corner towards the dragged one.
    const roomX = d.sx > 0 ? 1 - d.ax : d.ax;
    const roomY = d.sy > 0 ? 1 - d.ay : d.ay;
    let w = Math.min(roomX, Math.max(MIN, (p.x - d.ax) * d.sx));
    let h = Math.min(roomY, Math.max(MIN, (p.y - d.ay) * d.sy));
    if (!e.shiftKey) {
      // Follow whichever way the pointer went further, at the frame's shape.
      w = Math.max(w, h / k);
      w = Math.max(MIN, Math.min(w, roomX, roomY / k));
      h = w * k;
    }
    const x0 = d.sx > 0 ? d.ax : d.ax - w;
    const y0 = d.sy > 0 ? d.ay : d.ay - h;
    save({ x0, y0, x1: x0 + w, y1: y0 + h });
  }

  function up() {
    drag = null;
  }

  function fullFrame() {
    save(locked({ x0: 0, y0: 0, x1: 1, y1: 1 }));
  }

  onMount(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") finishCropEdit(false);
      else if (e.key === "Enter") finishCropEdit(true);
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });

  const CORNERS: { sx: number; sy: number; name: string }[] = [
    { sx: -1, sy: -1, name: "nw" },
    { sx: 1, sy: -1, name: "ne" },
    { sx: -1, sy: 1, name: "sw" },
    { sx: 1, sy: 1, name: "se" },
  ];
</script>

<div class="crop-overlay" bind:clientWidth={boxWidth} bind:clientHeight={boxHeight}>
  {#if area}
    <div class="picture" style:left="{area.x}px" style:top="{area.y}px" style:width="{area.w}px" style:height="{area.h}px">
      <div class="shade">
        <div
          class="hole"
          style:left="{rect.x0 * 100}%"
          style:top="{rect.y0 * 100}%"
          style:width="{(rect.x1 - rect.x0) * 100}%"
          style:height="{(rect.y1 - rect.y0) * 100}%">
        </div>
      </div>
      <div
        class="frame"
        role="presentation"
        style:left="{rect.x0 * 100}%"
        style:top="{rect.y0 * 100}%"
        style:width="{(rect.x1 - rect.x0) * 100}%"
        style:height="{(rect.y1 - rect.y0) * 100}%"
        onpointerdown={(e) => down(e, "move")}
        onpointermove={move}
        onpointerup={up}
        onpointercancel={up}>
        <span class="third v1"></span><span class="third v2"></span>
        <span class="third h1"></span><span class="third h2"></span>
        {#each CORNERS as c (c.name)}
          <span
            class="handle {c.name}"
            role="presentation"
            onpointerdown={(e) => down(e, "corner", c.sx, c.sy)}
            onpointermove={move}
            onpointerup={up}
            onpointercancel={up}></span>
        {/each}
      </div>
    </div>
  {/if}

  <div class="bar">
    <span class="hint">{t("crop.hint")}</span>
    <button class="btn" onclick={fullFrame}>{t("crop.full")}</button>
    <button class="btn" onclick={() => finishCropEdit(false)}>{t("crop.cancel")}</button>
    <button class="btn primary" onclick={() => finishCropEdit(true)}>{t("crop.done")}</button>
  </div>
</div>

<style>
  .crop-overlay {
    position: absolute;
    inset: 0;
    z-index: 2;
    touch-action: none;
    user-select: none;
  }
  .picture {
    position: absolute;
  }
  /* Darkens the picture outside the frame; the frame itself is not clipped, so its handles
     stay whole at the picture's edges. */
  .shade {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
  }
  .hole {
    position: absolute;
    box-shadow: 0 0 0 9999px rgb(0 0 0 / 0.55);
  }
  .frame {
    position: absolute;
    box-sizing: border-box;
    border: 2px solid #fff;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 0.5);
    cursor: move;
  }
  .third {
    position: absolute;
    background: rgb(255 255 255 / 0.35);
    pointer-events: none;
  }
  .v1,
  .v2 {
    top: 0;
    bottom: 0;
    width: 1px;
  }
  .v1 {
    left: 33.333%;
  }
  .v2 {
    left: 66.667%;
  }
  .h1,
  .h2 {
    left: 0;
    right: 0;
    height: 1px;
  }
  .h1 {
    top: 33.333%;
  }
  .h2 {
    top: 66.667%;
  }
  .handle {
    position: absolute;
    /* Above the buttons, which are above the frame. */
    z-index: 2;
    width: 14px;
    height: 14px;
    border: 2px solid var(--accent);
    border-radius: 3px;
    background: #fff;
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.5);
  }
  /* A bigger invisible target than the square shown. */
  .handle::after {
    content: "";
    position: absolute;
    inset: -10px;
  }
  .nw {
    left: -2px;
    top: -2px;
    cursor: nwse-resize;
  }
  .se {
    right: -2px;
    bottom: -2px;
    cursor: nwse-resize;
  }
  .ne {
    right: -2px;
    top: -2px;
    cursor: nesw-resize;
  }
  .sw {
    left: -2px;
    bottom: -2px;
    cursor: nesw-resize;
  }
  .bar {
    position: absolute;
    z-index: 1;
    inset-inline: 12px;
    inset-block-end: 12px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    pointer-events: none;
  }
  .hint {
    flex: 1 1 200px;
    min-width: 0;
    padding: 4px 10px;
    border-radius: 12px;
    background: rgb(0 0 0 / 0.55);
    backdrop-filter: blur(8px);
    color: #fff;
    font-size: 12px;
    text-wrap: pretty;
  }
  .btn {
    pointer-events: auto;
    height: 30px;
    padding: 0 14px;
    border: 1px solid rgb(255 255 255 / 0.25);
    border-radius: 15px;
    background: rgb(0 0 0 / 0.6);
    backdrop-filter: blur(8px);
    color: #fff;
    font: inherit;
    font-size: 13px;
    cursor: pointer;
    transition-property: scale, background-color;
    transition-duration: 150ms;
  }
  .btn:hover {
    background: rgb(40 40 40 / 0.8);
  }
  .btn:active {
    scale: 0.96;
  }
  .btn.primary {
    border-color: transparent;
    background: var(--accent);
    color: var(--on-accent);
    font-weight: 600;
  }
  .hint {
    pointer-events: none;
  }
</style>
