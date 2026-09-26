// What the phone card says: one line of status, a colored dot and what to do about it.

import type { Snapshot } from "./api";
import { t } from "./i18n";

export type Tone = "idle" | "ok" | "live" | "busy" | "warn" | "error";

export interface StatusInfo {
  tone: Tone;
  title: string;
  hint: string;
}

export function statusInfo(s: Snapshot): StatusInfo {
  if (s.problem) {
    return { tone: "error", title: `${t("status.error")}: ${s.problem}`, hint: t("status.problem.hint") };
  }
  const d = s.device;
  if (!d) return { tone: "idle", title: t("status.noPhone"), hint: t("status.noPhone.hint") };
  if (d.state === "unauthorized") {
    return { tone: "warn", title: t("status.unauthorized"), hint: t("status.unauthorized.hint") };
  }
  if (d.state !== "device") return { tone: "warn", title: t("status.offline"), hint: t("status.offline.hint") };
  if (!s.cameraOn || !s.status) return { tone: "ok", title: t("status.ready"), hint: "" };

  switch (s.status.kind) {
    case "streaming":
      // The frame rate is on the chip over the video.
      return { tone: "live", title: t("status.streaming"), hint: "" };
    case "connecting":
      return { tone: "busy", title: t("status.connecting"), hint: "" };
    case "waitingForDevice":
    case "stopped":
      return { tone: "warn", title: t("status.waiting"), hint: t("status.noPhone.hint") };
    case "noPicture":
      return { tone: "warn", title: t("status.noPicture"), hint: t("status.retry.hint") };
    case "error": {
      const m = s.status.message;
      // The camera service refuses for a while after a lens failed (docs/stage2.md).
      const busy = /not found|Too many other clients|\(none\)/i.test(m);
      return { tone: "error", title: t("status.error"), hint: busy ? t("status.cameraBusy.hint") : `${m} — ${t("status.retry.hint")}` };
    }
  }
}
