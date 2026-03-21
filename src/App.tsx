import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Settings } from "./components/Settings";
import { ModelManager } from "./components/ModelManager";
import { Overlay } from "./components/Overlay";
import { Toast } from "./components/Toast";
import { useTauriEvents } from "./hooks/useTauriEvents";

function SettingsWindow() {
  const [activeModel, setActiveModel] = useState("base");
  const [toastMessage, setToastMessage] = useState("");
  const [toastVisible, setToastVisible] = useState(false);

  useEffect(() => {
    invoke<{ active_model: string }>("get_settings").then((s) =>
      setActiveModel(s.active_model)
    );
  }, []);

  useTauriEvents((event) => {
    if (event.type === "backend-error" || event.type === "transcription-error") {
      setToastMessage(event.message);
      setToastVisible(true);
    }
  });

  const dismissToast = useCallback(() => setToastVisible(false), []);

  return (
    <div className="settings-root">
      <Settings />
      <ModelManager activeModel={activeModel} />
      <Toast
        message={toastMessage}
        visible={toastVisible}
        onDismiss={dismissToast}
      />
    </div>
  );
}

function OverlayWindow() {
  useTauriEvents((event) => {
    if (event.type === "hotkey-start") {
      invoke("start_recording").catch(console.error);
    } else if (event.type === "hotkey-stop") {
      invoke("stop_recording").catch(console.error);
    }
  });

  return <Overlay />;
}

function App() {
  const label = (window as any).__TAURI_INTERNALS__?.metadata?.currentWindow
    ?.label as string | undefined;

  if (label === "overlay") {
    return <OverlayWindow />;
  }

  return <SettingsWindow />;
}

export default App;
