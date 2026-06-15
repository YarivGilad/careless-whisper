import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Settings } from "./components/Settings";
import { ModelManager } from "./components/ModelManager";
import { Overlay } from "./components/Overlay";
import { Toast } from "./components/Toast";
import { TitleBar } from "./components/TitleBar";
import { useTauriEvents } from "./hooks/useTauriEvents";

// Native window decorations don't work on Linux/Wayland (KWin), so we render a
// custom titlebar there. macOS/Windows keep their native decorations.
const isLinux = navigator.userAgent.includes("Linux");

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      metadata?: {
        currentWindow?: {
          label?: string;
        };
      };
    };
  }
}

function SettingsWindow() {
  const [activeModel, setActiveModel] = useState("base");
  const [toastMessage, setToastMessage] = useState("");
  const [toastVisible, setToastVisible] = useState(false);

  useEffect(() => {
    void invoke<{ active_model: string }>("get_settings").then((s) =>
      setActiveModel(s.active_model)
    );
  }, []);

  useTauriEvents((event) => {
    if (event.type === "backend-error" || event.type === "transcription-error") {
      setToastMessage(event.message);
      setToastVisible(true);
      return;
    }

    if (event.type === "transcription-complete") {
      setToastMessage("Transcription copied to clipboard");
      setToastVisible(true);
    }
  });

  const dismissToast = useCallback(() => setToastVisible(false), []);

  return (
    <div className="window-root">
      {isLinux && <TitleBar />}
      <div className="settings-root">
        <Settings />
        <ModelManager activeModel={activeModel} />
        <Toast
          message={toastMessage}
          visible={toastVisible}
          onDismiss={dismissToast}
        />
      </div>
    </div>
  );
}

function OverlayWindow() {
  useTauriEvents((event) => {
    if (event.type === "hotkey-start") {
      void invoke("start_recording").catch(console.error);
    } else if (event.type === "hotkey-stop") {
      void invoke("stop_recording").catch(console.error);
    }
  });

  return <Overlay />;
}

function App() {
  const label = window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label;

  if (label === "overlay") {
    return <OverlayWindow />;
  }

  return <SettingsWindow />;
}

export default App;
