import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Settings } from "./components/Settings";
import { ModelManager } from "./components/ModelManager";
import { Overlay } from "./components/Overlay";
import { useTauriEvents } from "./hooks/useTauriEvents";

function SettingsWindow() {
  const [activeModel, setActiveModel] = useState("base");

  useEffect(() => {
    invoke<{ active_model: string }>("get_settings").then((s) =>
      setActiveModel(s.active_model)
    );
  }, []);

  return (
    <div className="settings-root">
      <Settings />
      <ModelManager activeModel={activeModel} />
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
