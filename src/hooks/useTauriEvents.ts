import { useEffect, useEffectEvent } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type AppEvent =
  | {
      type: "recording-started";
      realtime: boolean;
      autoPaste: boolean;
      targetCaptured: boolean;
    }
  | { type: "recording-stopped"; finalizing: boolean; realtime: boolean }
  | { type: "transcription-complete"; text: string }
  | { type: "realtime-transcription"; text: string; fullText: string }
  | { type: "realtime-mode-updated"; armed: boolean; active: boolean }
  | { type: "transcription-error"; message: string }
  | { type: "download-progress"; model: string; percent: number }
  | { type: "hotkey-start" }
  | { type: "hotkey-stop" }
  | { type: "backend-error"; message: string };

type Handler = (event: AppEvent) => void;

export function useTauriEvents(handler: Handler) {
  const onEvent = useEffectEvent(handler);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    const setup = async () => {
      const subscriptions = await Promise.all([
        listen<{ realtime?: boolean; auto_paste?: boolean; target_captured?: boolean }>(
          "recording-started",
          (e) =>
            onEvent({
              type: "recording-started",
              realtime: Boolean(e.payload?.realtime),
              autoPaste: Boolean(e.payload?.auto_paste),
              targetCaptured: Boolean(e.payload?.target_captured),
            })
        ),
        listen<{ finalizing?: boolean; realtime?: boolean }>("recording-stopped", (e) =>
          onEvent({
            type: "recording-stopped",
            finalizing: e.payload?.finalizing ?? true,
            realtime: Boolean(e.payload?.realtime),
          })
        ),
        listen<{ text: string }>("transcription-complete", (e) =>
          onEvent({ type: "transcription-complete", text: e.payload.text })
        ),
        listen<{ text: string; full_text: string }>("realtime-transcription", (e) =>
          onEvent({
            type: "realtime-transcription",
            text: e.payload.text,
            fullText: e.payload.full_text,
          })
        ),
        listen<{ armed: boolean; active: boolean }>("realtime-mode-updated", (e) =>
          onEvent({
            type: "realtime-mode-updated",
            armed: e.payload.armed,
            active: e.payload.active,
          })
        ),
        listen<{ message: string }>("transcription-error", (e) =>
          onEvent({ type: "transcription-error", message: e.payload.message })
        ),
        listen<{ model: string; percent: number }>("download-progress", (e) =>
          onEvent({
            type: "download-progress",
            model: e.payload.model,
            percent: e.payload.percent,
          })
        ),
        listen("hotkey-start", () => onEvent({ type: "hotkey-start" })),
        listen("hotkey-stop", () => onEvent({ type: "hotkey-stop" })),
        listen<{ message: string }>("backend-error", (e) =>
          onEvent({ type: "backend-error", message: e.payload.message })
        ),
      ]);

      if (cancelled) {
        subscriptions.forEach((unsubscribe) => unsubscribe());
        return;
      }

      unlisteners.push(...subscriptions);
    };

    void setup();

    return () => {
      cancelled = true;
      unlisteners.forEach((unsubscribe) => unsubscribe());
    };
  }, [onEvent]);
}
