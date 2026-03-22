import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";

interface Settings {
  hotkey: string;
  recording_mode: "push_to_talk" | "toggle";
  active_model: string;
  language: string;
  auto_paste: boolean;
  max_recording_seconds: number;
  launch_at_login: boolean;
  overlay_position: "top_center" | "bottom_center" | "top_left" | "top_right";
  lower_volume_while_recording: boolean;
}

export function Settings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [launchAtLogin, setLaunchAtLogin] = useState(false);
  const [accessibilityGranted, setAccessibilityGranted] = useState<boolean | null>(null);
  const [micStatus, setMicStatus] = useState<string | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);
  const [logsCopied, setLogsCopied] = useState(false);

  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings);
    invoke<boolean>("get_launch_at_login").then(setLaunchAtLogin).catch(() => {});
    invoke<boolean>("check_accessibility").then(setAccessibilityGranted).catch(() => {});
    invoke<string>("check_microphone").then(setMicStatus).catch(() => {});
  }, []);

  // Listen for backend errors
  useEffect(() => {
    const unlisten = listen<{ message: string }>("backend-error", (e) => {
      setLastError(e.payload.message);
    });
    const unlisten2 = listen<{ message: string }>("transcription-error", (e) => {
      setLastError(e.payload.message);
    });
    return () => {
      unlisten.then((fn) => fn());
      unlisten2.then((fn) => fn());
    };
  }, []);

  const reportIssue = async () => {
    const logs = await invoke<string>("get_recent_logs");
    await navigator.clipboard.writeText(logs);
    setLogsCopied(true);
    setTimeout(() => setLogsCopied(false), 3000);
    await openUrl(
      "https://github.com/YarivGilad/careless-whisper/issues/new?title=Bug+Report&body=%0A%0A---%0APaste+your+logs+here+(already+copied+to+clipboard)"
    );
  };

  // Re-check permissions when window regains focus (user may have toggled them in System Settings)
  useEffect(() => {
    const onFocus = () => {
      invoke<boolean>("check_accessibility").then(setAccessibilityGranted).catch(() => {});
      invoke<string>("check_microphone").then(setMicStatus).catch(() => {});
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  const save = async () => {
    if (!settings) return;
    setSaving(true);
    try {
      await invoke("update_settings", { settings });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } finally {
      setSaving(false);
    }
  };

  if (!settings) return <div>Loading…</div>;

  return (
    <div>
      <h2 style={{ margin: "0 0 20px", fontSize: 18, fontWeight: 700 }}>
        Careless Whisper
      </h2>

      {accessibilityGranted === false && (
        <div className="accessibility-banner">
          <div style={{ marginBottom: 8 }}>
            <strong>Accessibility Permission Required</strong>
          </div>
          <p style={{ margin: "0 0 10px", fontSize: 13, lineHeight: 1.5 }}>
            Careless Whisper needs Accessibility access to paste transcribed text
            into your apps. Without it, text will only be copied to the clipboard.
          </p>
          <button
            className="btn-secondary"
            onClick={() => {
              invoke("request_accessibility").then(() => {
                // Re-check after a short delay (user needs time to toggle)
                setTimeout(() => {
                  invoke<boolean>("check_accessibility").then(setAccessibilityGranted);
                }, 1000);
              });
            }}
          >
            Open System Settings
          </button>
        </div>
      )}

      {micStatus && micStatus !== "authorized" && (
        <div className="accessibility-banner">
          <div style={{ marginBottom: 8 }}>
            <strong>Microphone Permission {micStatus === "denied" ? "Denied" : "Required"}</strong>
          </div>
          <p style={{ margin: "0 0 10px", fontSize: 13, lineHeight: 1.5 }}>
            {micStatus === "denied"
              ? "Microphone access was denied. Please enable it in System Settings > Privacy & Security > Microphone."
              : "Careless Whisper needs microphone access to record your voice for transcription."}
          </p>
          <button
            className="btn-secondary"
            onClick={() => {
              if (micStatus === "denied") {
                openUrl("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
              } else {
                invoke<string>("request_microphone").then((status) => {
                  setMicStatus(status);
                });
              }
            }}
          >
            {micStatus === "denied" ? "Open System Settings" : "Grant Microphone Access"}
          </button>
        </div>
      )}

      {lastError && (
        <div className="error-banner">
          <div style={{ marginBottom: 8 }}>
            <strong>Something went wrong</strong>
          </div>
          <p style={{ margin: "0 0 10px", fontSize: 13, lineHeight: 1.5 }}>
            {lastError}
          </p>
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <button className="btn-secondary" onClick={reportIssue}>
              {logsCopied ? "Logs copied! Paste in the issue" : "Report Issue"}
            </button>
            <button
              className="btn-secondary"
              onClick={() => setLastError(null)}
              style={{ padding: "6px 10px" }}
            >
              Dismiss
            </button>
          </div>
        </div>
      )}

      <div className="settings-section">
        <label className="settings-label">Recording Hotkey</label>
        <input
          className="settings-input"
          value={settings.hotkey}
          onChange={(e) =>
            setSettings({ ...settings, hotkey: e.target.value })
          }
          placeholder="e.g. CmdOrCtrl+Shift+Space"
        />
      </div>

      <div className="settings-section">
        <label className="settings-label">Recording Mode</label>
        <select
          className="settings-select"
          value={settings.recording_mode}
          onChange={(e) =>
            setSettings({
              ...settings,
              recording_mode: e.target.value as Settings["recording_mode"],
            })
          }
        >
          <option value="toggle">Toggle (press to start / press to stop)</option>
          <option value="push_to_talk">Push to Talk (hold to record)</option>
        </select>
      </div>

      <div className="settings-section">
        <label className="settings-label">Language</label>
        <select
          className="settings-select"
          value={settings.language}
          onChange={(e) =>
            setSettings({ ...settings, language: e.target.value })
          }
        >
          <option value="auto">Auto-detect</option>
          <option value="en">English</option>
          <option value="he">Hebrew</option>
          <option value="es">Spanish</option>
          <option value="fr">French</option>
          <option value="de">German</option>
          <option value="ja">Japanese</option>
          <option value="zh">Chinese</option>
          <option value="pt">Portuguese</option>
          <option value="ru">Russian</option>
          <option value="ko">Korean</option>
          <option value="ar">Arabic</option>
          <option value="it">Italian</option>
          <option value="nl">Dutch</option>
          <option value="hi">Hindi</option>
          <option value="tr">Turkish</option>
          <option value="pl">Polish</option>
          <option value="uk">Ukrainian</option>
        </select>
      </div>

      <div className="settings-section">
        <label className="settings-label">Overlay Position</label>
        <select
          className="settings-select"
          value={settings.overlay_position}
          onChange={(e) =>
            setSettings({
              ...settings,
              overlay_position: e.target.value as Settings["overlay_position"],
            })
          }
        >
          <option value="top_center">Top Center</option>
          <option value="bottom_center">Bottom Center</option>
          <option value="top_left">Top Left</option>
          <option value="top_right">Top Right</option>
        </select>
      </div>

      <div className="settings-section">
        <label className="settings-label">Max Recording Duration (seconds)</label>
        <input
          className="settings-input"
          type="number"
          min={10}
          max={600}
          value={settings.max_recording_seconds}
          onChange={(e) =>
            setSettings({
              ...settings,
              max_recording_seconds: parseInt(e.target.value) || 120,
            })
          }
        />
      </div>

      <div className="settings-section">
        <div className="settings-toggle">
          <span>Auto-paste after transcription</span>
          <input
            type="checkbox"
            checked={settings.auto_paste}
            onChange={(e) =>
              setSettings({ ...settings, auto_paste: e.target.checked })
            }
          />
        </div>
        <div className="settings-toggle">
          <span>Lower volume while recording</span>
          <input
            type="checkbox"
            checked={settings.lower_volume_while_recording}
            onChange={(e) =>
              setSettings({ ...settings, lower_volume_while_recording: e.target.checked })
            }
          />
        </div>
        <div className="settings-toggle">
          <span>Launch at login</span>
          <input
            type="checkbox"
            checked={launchAtLogin}
            onChange={async (e) => {
              const enabled = e.target.checked;
              try {
                await invoke("set_launch_at_login", { enabled });
                setLaunchAtLogin(enabled);
              } catch (err) {
                console.error("Failed to set launch at login:", err);
              }
            }}
          />
        </div>
      </div>

      <button className="btn-primary" onClick={save} disabled={saving}>
        {saving ? "Saving…" : saved ? "Saved!" : "Save Settings"}
      </button>
    </div>
  );
}
