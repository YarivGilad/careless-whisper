import { useEffect, useRef, useState } from "react";
import { useTauriEvents } from "../hooks/useTauriEvents";

type OverlayState = "idle" | "recording" | "transcribing" | "error";

export function Overlay() {
  const [state, setState] = useState<OverlayState>("idle");
  const [elapsed, setElapsed] = useState(0);
  const [errorMsg, setErrorMsg] = useState("");
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useTauriEvents((event) => {
    if (event.type === "recording-started") {
      setState("recording");
      setElapsed(0);
    } else if (event.type === "recording-stopped") {
      setState("transcribing");
      if (timerRef.current) clearInterval(timerRef.current);
    } else if (event.type === "transcription-complete") {
      setState("idle");
    } else if (event.type === "transcription-error") {
      setErrorMsg(event.message);
      setState("error");
      setTimeout(() => setState("idle"), 3000);
    }
  });

  useEffect(() => {
    if (state === "recording") {
      timerRef.current = setInterval(() => setElapsed((s) => s + 1), 1000);
    } else {
      if (timerRef.current) clearInterval(timerRef.current);
    }
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [state]);

  const formatTime = (s: number) =>
    `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;

  if (state === "idle") return null;

  return (
    <div className="overlay-root">
      {state === "recording" && (
        <div className="overlay-pill">
          <span className="recording-dot" />
          <span className="overlay-text">{formatTime(elapsed)}</span>
        </div>
      )}
      {state === "transcribing" && (
        <div className="overlay-pill">
          <span className="spinner" />
          <span className="overlay-text">Transcribing…</span>
        </div>
      )}
      {state === "error" && (
        <div className="overlay-pill overlay-error">
          <span className="overlay-text">{errorMsg}</span>
        </div>
      )}
    </div>
  );
}
