import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";

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
  translate_to_english: boolean;
  realtime_transcription: boolean;
}

const AUDIO_FILE_FILTERS = [
  {
    name: "Audio",
    extensions: ["mp3", "wav", "m4a", "mp4", "aac", "flac", "ogg", "oga"],
  },
];

const AUTOSAVE_DEBOUNCE_MS = 600;

export function Settings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [launchAtLogin, setLaunchAtLogin] = useState(false);
  const [accessibilityGranted, setAccessibilityGranted] = useState<boolean | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);
  const [logsCopied, setLogsCopied] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const [selectedAudioPath, setSelectedAudioPath] = useState<string | null>(null);
  const [transcribingFile, setTranscribingFile] = useState(false);
  const [recording, setRecording] = useState(false);
  const [finalizingRecording, setFinalizingRecording] = useState(false);
  const settingsRef = useRef<Settings | null>(null);
  const saveTimerRef = useRef<number | null>(null);
  const savedTimerRef = useRef<number | null>(null);
  const saveRevisionRef = useRef(0);
  const inFlightSavesRef = useRef(0);

  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  useEffect(() => {
    return () => {
      if (saveTimerRef.current !== null) {
        window.clearTimeout(saveTimerRef.current);
      }
      if (savedTimerRef.current !== null) {
        window.clearTimeout(savedTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    void getVersion().then(setAppVersion).catch(() => {});
    void invoke<Settings>("get_settings").then(setSettings);
    void invoke<boolean>("get_launch_at_login").then(setLaunchAtLogin).catch(() => {});
    void invoke<boolean>("check_accessibility").then(setAccessibilityGranted).catch(() => {});
  }, []);

  useEffect(() => {
    const unlistenError = listen<{ message: string }>("backend-error", (e) => {
      setLastError(e.payload.message);
    });
    const unlistenTranscriptionError = listen<{ message: string }>("transcription-error", (e) => {
      setLastError(e.payload.message);
      setTranscribingFile(false);
      setFinalizingRecording(false);
    });
    const unlistenComplete = listen<{ text: string }>("transcription-complete", () => {
      setTranscribingFile(false);
      setFinalizingRecording(false);
    });
    const unlistenRecordingStarted = listen("recording-started", () => {
      setRecording(true);
      setFinalizingRecording(false);
    });
    const unlistenRecordingStopped = listen<{ finalizing?: boolean }>("recording-stopped", (e) => {
      setRecording(false);
      setFinalizingRecording(e.payload?.finalizing ?? true);
    });
    const unlistenSettingsUpdated = listen<{ settings?: Settings }>("settings-updated", (e) => {
      if (saveTimerRef.current !== null || inFlightSavesRef.current > 0) {
        return;
      }

      if (e.payload?.settings) {
        settingsRef.current = e.payload.settings;
        setSettings(e.payload.settings);
        return;
      }

      void invoke<Settings>("get_settings").then((nextSettings) => {
        if (saveTimerRef.current !== null || inFlightSavesRef.current > 0) {
          return;
        }
        settingsRef.current = nextSettings;
        setSettings(nextSettings);
      });
    });

    return () => {
      void unlistenError.then((fn) => fn());
      void unlistenTranscriptionError.then((fn) => fn());
      void unlistenComplete.then((fn) => fn());
      void unlistenRecordingStarted.then((fn) => fn());
      void unlistenRecordingStopped.then((fn) => fn());
      void unlistenSettingsUpdated.then((fn) => fn());
    };
  }, []);

  const reportIssue = async () => {
    try {
      const logs = await invoke<string>("get_recent_logs");
      await navigator.clipboard.writeText(logs);
      setLogsCopied(true);
      window.setTimeout(() => setLogsCopied(false), 3000);
    } catch (error) {
      console.warn("Failed to copy logs to clipboard:", error);
    }
    await openUrl(
      "https://github.com/YarivGilad/careless-whisper/issues/new?title=Bug+Report&body=%0A%0A---%0APaste+your+logs+here+(already+copied+to+clipboard)"
    );
  };

  useEffect(() => {
    const onFocus = () => {
      void invoke<boolean>("check_accessibility").then(setAccessibilityGranted).catch(() => {});
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  const showSavedState = () => {
    setSaved(true);
    if (savedTimerRef.current !== null) {
      window.clearTimeout(savedTimerRef.current);
    }
    savedTimerRef.current = window.setTimeout(() => {
      setSaved(false);
      savedTimerRef.current = null;
    }, 1800);
  };

  const saveSettings = async (nextSettings: Settings, revision: number) => {
    inFlightSavesRef.current += 1;
    setSaving(true);
    try {
      await invoke("update_settings", { settings: nextSettings });
      if (revision === saveRevisionRef.current) {
        showSavedState();
      }
    } finally {
      inFlightSavesRef.current = Math.max(0, inFlightSavesRef.current - 1);
      if (inFlightSavesRef.current === 0) {
        setSaving(false);
      }
    }
  };

  const persistSettings = (nextSettings: Settings, previousSettings: Settings, revision: number) => {
    void saveSettings(nextSettings, revision).catch((error) => {
      if (revision !== saveRevisionRef.current) {
        return;
      }
      settingsRef.current = previousSettings;
      setSettings(previousSettings);
      setLastError(`Failed to save settings: ${String(error)}`);
    });
  };

  const updateSettings = (
    patch: Partial<Settings> | ((current: Settings) => Settings),
    options: { debounce?: boolean } = {}
  ) => {
    const previousSettings = settingsRef.current;
    if (!previousSettings) return;

    const nextSettings =
      typeof patch === "function" ? patch(previousSettings) : { ...previousSettings, ...patch };
    const revision = saveRevisionRef.current + 1;
    saveRevisionRef.current = revision;

    if (saveTimerRef.current !== null) {
      window.clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }

    settingsRef.current = nextSettings;
    setSettings(nextSettings);
    setLastError(null);
    setSaved(false);

    const runSave = () => {
      saveTimerRef.current = null;
      persistSettings(nextSettings, previousSettings, revision);
    };

    if (options.debounce) {
      saveTimerRef.current = window.setTimeout(runSave, AUTOSAVE_DEBOUNCE_MS);
    } else {
      runSave();
    }
  };

  const setRealtimeMode = (enabled: boolean) => {
    updateSettings({ realtime_transcription: enabled });
  };

  const chooseAudioFile = async () => {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: AUDIO_FILE_FILTERS,
    });

    if (typeof selected === "string") {
      setSelectedAudioPath(selected);
    }
  };

  const transcribeSelectedFile = async () => {
    if (!selectedAudioPath) {
      return;
    }

    setLastError(null);
    setTranscribingFile(true);
    try {
      await invoke("transcribe_audio_file", { path: selectedAudioPath });
    } catch (error) {
      setTranscribingFile(false);
      setLastError(String(error));
    }
  };

  const toggleRecording = async () => {
    setLastError(null);
    try {
      if (recording) {
        await invoke("stop_recording");
      } else {
        await invoke("start_recording_from_settings_with_target_delay");
      }
    } catch (error) {
      setRecording(false);
      setFinalizingRecording(false);
      setLastError(String(error));
    }
  };

  if (!settings) return <div>Loading…</div>;

  return (
    <div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", margin: "0 0 20px" }}>
        <h2 style={{ margin: 0, fontSize: 18, fontWeight: 700 }}>
          Careless Whisper
        </h2>
        <div className="settings-header-meta">
          {saving && <span className="settings-save-status">Saving...</span>}
          {!saving && saved && <span className="settings-save-status settings-save-status-saved">Saved</span>}
          {appVersion && <span>v{appVersion}</span>}
        </div>
      </div>

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
              void invoke("request_accessibility").then(() => {
                window.setTimeout(() => {
                  void invoke<boolean>("check_accessibility").then(setAccessibilityGranted);
                }, 1000);
              });
            }}
          >
            Open System Settings
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
            <button className="btn-secondary" onClick={() => void reportIssue()}>
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

      <div className="settings-section recording-control">
        <div>
          <label className="settings-label">Recording</label>
          <div className="recording-helper">
            The hotkey starts immediately. The button hides Settings first; focus the target field when it closes.
          </div>
          <div className="recording-mode-row">
            <button
              type="button"
              className={`mode-pill ${
                settings.realtime_transcription ? "mode-pill-armed" : "mode-pill-disabled"
              }`}
              onClick={() => setRealtimeMode(!settings.realtime_transcription)}
            >
              {settings.realtime_transcription ? "Realtime armed" : "Realtime disabled"}
            </button>
            {settings.auto_paste && (
              <span className="mode-pill mode-pill-armed">Auto-paste on</span>
            )}
          </div>
        </div>
        <button
          className={recording ? "btn-danger" : "btn-primary"}
          onClick={() => void toggleRecording()}
          disabled={finalizingRecording}
          title="Starts after Settings hides; focus the destination field when it closes."
        >
          {recording ? "Stop Recording" : finalizingRecording ? "Transcribing..." : "Start Recording"}
        </button>
      </div>

      <div className="settings-section">
        <label className="settings-label">Transcribe Audio File</label>
        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <button className="btn-secondary" onClick={() => void chooseAudioFile()}>
            Choose File
          </button>
          <button
            className="btn-primary"
            onClick={() => void transcribeSelectedFile()}
            disabled={!selectedAudioPath || transcribingFile}
          >
            {transcribingFile ? "Transcribing…" : "Transcribe File"}
          </button>
        </div>
        <p style={{ fontSize: 12, color: "#8e8e93", margin: "8px 0 0", wordBreak: "break-all" }}>
          {selectedAudioPath ?? "Supports MP3, WAV, M4A, MP4, AAC, FLAC, and OGG."}
        </p>
      </div>

      <div className="settings-section">
        <label className="settings-label">Recording Hotkey</label>
        <input
          className="settings-input"
          value={settings.hotkey}
          onChange={(e) =>
            updateSettings({ hotkey: e.target.value }, { debounce: true })
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
            updateSettings({
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
            updateSettings({ language: e.target.value })
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
        {settings.language === "auto" && (
          <p style={{ margin: "6px 0 0", fontSize: 12, color: "#f5a623", lineHeight: 1.4 }}>
            Tip: Auto-detect may default to English for short recordings. For best results with non-English languages, select your language above.
          </p>
        )}
      </div>

      <div className="settings-section">
        <label className="settings-label">Overlay Position</label>
        <select
          className="settings-select"
          value={settings.overlay_position}
          onChange={(e) =>
            updateSettings({
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
            updateSettings({
              max_recording_seconds: Number.parseInt(e.target.value, 10) || 120,
            }, { debounce: true })
          }
        />
      </div>

      <div className="settings-section">
        <div className="settings-toggle">
          <span>Realtime transcription</span>
          <input
            type="checkbox"
            checked={settings.realtime_transcription}
            onChange={(e) => setRealtimeMode(e.target.checked)}
          />
        </div>
        <div className="settings-toggle">
          <span>Auto-paste after transcription</span>
          <input
            type="checkbox"
            checked={settings.auto_paste}
            onChange={(e) =>
              updateSettings({ auto_paste: e.target.checked })
            }
          />
        </div>
        <div className="settings-toggle">
          <span>Lower volume while recording</span>
          <input
            type="checkbox"
            checked={settings.lower_volume_while_recording}
            onChange={(e) =>
              updateSettings({ lower_volume_while_recording: e.target.checked })
            }
          />
        </div>
        <div className="settings-toggle">
          <span>Translate to English</span>
          <input
            type="checkbox"
            checked={settings.translate_to_english}
            onChange={(e) =>
              updateSettings({ translate_to_english: e.target.checked })
            }
          />
        </div>
        <div className="settings-toggle">
          <span>Launch at login</span>
          <input
            type="checkbox"
            checked={launchAtLogin}
            onChange={(e) => {
              const enabled = e.target.checked;
              void invoke("set_launch_at_login", { enabled })
                .then(() => {
                  setLaunchAtLogin(enabled);
                })
                .catch((error) => {
                  setLastError(`Failed to set launch at login: ${String(error)}`);
                });
            }}
          />
        </div>
      </div>

      <div className="help-section">
        <p style={{ fontSize: 12, color: "#8e8e93", marginBottom: 8 }}>
          Having trouble? Copy the app logs and share them in a GitHub issue.
        </p>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn-secondary" onClick={() => void reportIssue()}>
            {logsCopied ? "Logs copied! Paste in the issue" : "Copy Logs & Report Issue"}
          </button>
        </div>
      </div>
    </div>
  );
}
