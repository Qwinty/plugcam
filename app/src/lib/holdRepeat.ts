// Button action: runs `fn` on press and keeps running it while the button is held,
// like the zoom buttons of a camera app. Keyboard activation (Enter/Space) runs it once.

const DELAY_MS = 400;
const INTERVAL_MS = 80;

export function holdRepeat(node: HTMLButtonElement, fn: () => void) {
  let run = fn;
  let delay: ReturnType<typeof setTimeout> | undefined;
  let repeat: ReturnType<typeof setInterval> | undefined;

  const stop = () => {
    clearTimeout(delay);
    clearInterval(repeat);
    delay = repeat = undefined;
  };
  const down = (e: PointerEvent) => {
    if (e.button !== 0 || node.disabled) return;
    node.setPointerCapture(e.pointerId);
    run();
    delay = setTimeout(() => {
      repeat = setInterval(() => (node.disabled ? stop() : run()), INTERVAL_MS);
    }, DELAY_MS);
  };
  // Pointer presses are handled above; a click with detail 0 comes from the keyboard.
  const click = (e: MouseEvent) => {
    if (e.detail === 0) run();
  };

  node.addEventListener("pointerdown", down);
  node.addEventListener("pointerup", stop);
  node.addEventListener("pointercancel", stop);
  node.addEventListener("lostpointercapture", stop);
  node.addEventListener("click", click);
  return {
    update(next: () => void) {
      run = next;
    },
    destroy() {
      stop();
      node.removeEventListener("pointerdown", down);
      node.removeEventListener("pointerup", stop);
      node.removeEventListener("pointercancel", stop);
      node.removeEventListener("lostpointercapture", stop);
      node.removeEventListener("click", click);
    },
  };
}
