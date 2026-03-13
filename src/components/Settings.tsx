import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Settings {
  hotkey: string;
  recording_mode: "push_to_talk" | "toggle";
  active_model: string;
  language: string;
  auto_paste: boolean;
  max_recording_seconds: number;
  launch_at_login: boolean;
  overlay_position: "top_center" | "bottom_center" | "top_left" | "top_right";
}

export function Settings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings);
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
          <span style={{ color: "#8e8e93" }}>
            Launch at login
            <span
              style={{
                fontSize: 11,
                marginLeft: 6,
                background: "#3a3a3c",
                padding: "1px 6px",
                borderRadius: 4,
              }}
            >
              coming soon
            </span>
          </span>
          <input type="checkbox" disabled checked={settings.launch_at_login} />
        </div>
      </div>

      <button className="btn-primary" onClick={save} disabled={saving}>
        {saving ? "Saving…" : saved ? "Saved!" : "Save Settings"}
      </button>
    </div>
  );
}
