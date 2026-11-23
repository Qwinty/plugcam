// Drawing the crop on the preview. While it is open the crop is taken off, so the preview (and
// the camera) show the whole picture with a frame over it; Done saves the frame as the crop,
// Cancel puts the old crop back.

import * as api from "./api";
import type { Crop } from "./api";

export const NO_CROP: Crop = { left: 0, right: 0, top: 0, bottom: 0 };

export const cropEdit = $state<{ active: boolean; draft: Crop; saved: Crop }>({
  active: false,
  draft: { ...NO_CROP },
  saved: { ...NO_CROP },
});

export async function startCropEdit(current: Crop) {
  if (cropEdit.active) return;
  cropEdit.saved = { ...current };
  cropEdit.draft = { ...current };
  cropEdit.active = true;
  try {
    await api.updateSettings({ crop: NO_CROP });
  } catch {
    cropEdit.active = false;
  }
}

export async function finishCropEdit(apply: boolean) {
  if (!cropEdit.active) return;
  const round = (v: number) => Math.round(v * 100) / 100;
  const d = cropEdit.draft;
  const crop = apply
    ? { left: round(d.left), right: round(d.right), top: round(d.top), bottom: round(d.bottom) }
    : cropEdit.saved;
  cropEdit.active = false;
  await api.updateSettings({ crop });
}
