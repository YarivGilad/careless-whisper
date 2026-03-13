import { useEffect } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

export type AppEvent =
  | { type: "recording-started" }
  | { type: "recording-stopped" }
  | { type: "transcription-complete"; text: string }
  | { type: "transcription-error"; message: string }
  | { type: "download-progress"; model: string; percent: number }
  | { type: "hotkey-start" }
  | { type: "hotkey-stop" };

type Handler = (event: AppEvent) => void;

export function useTauriEvents(handler: Handler) {
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    const setup = async () => {
      unlisteners.push(
        await listen("recording-started", () =>
          handler({ type: "recording-started" })
        )
      );
      unlisteners.push(
        await listen("recording-stopped", () =>
          handler({ type: "recording-stopped" })
        )
      );
      unlisteners.push(
        await listen<{ text: string }>("transcription-complete", (e) =>
          handler({ type: "transcription-complete", text: e.payload.text })
        )
      );
      unlisteners.push(
        await listen<{ message: string }>("transcription-error", (e) =>
          handler({ type: "transcription-error", message: e.payload.message })
        )
      );
      unlisteners.push(
        await listen<{ model: string; percent: number }>(
          "download-progress",
          (e) =>
            handler({
              type: "download-progress",
              model: e.payload.model,
              percent: e.payload.percent,
            })
        )
      );
      unlisteners.push(
        await listen("hotkey-start", () => handler({ type: "hotkey-start" }))
      );
      unlisteners.push(
        await listen("hotkey-stop", () => handler({ type: "hotkey-stop" }))
      );
    };

    setup();
    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, []);
}
