# Implementation Plan

Implementation order follows the dependency graph: foundational Rust modules first, IPC wiring next, frontend last.

---

## Phase 1 — Config & Settings

**`config/settings.rs`**
- Implement `Settings::load()` — read `config.json` from `dirs::data_dir()/careless-whisper/config.json`; fall back to `Settings::default()` if missing
- Implement `Settings::save()` — serialize and write to same path, creating dirs as needed
- Wire into `get_settings` and `update_settings` commands

**`lib.rs`**
- Add a `AppState` struct (wrapped in `Mutex`) holding the loaded `Settings` and a slot for the active `WhisperContext`; register it with `.manage()` in the Tauri builder

---

## Phase 2 — Model Management

**`models/downloader.rs`**
- Define a `ModelInfo` struct: name, disk size, RAM estimate, download URL (`https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{name}.bin`), `is_downloaded` flag
- Implement `list_models()` — returns the static registry (tiny/base/small/medium/large-v3) annotated with which are present on disk
- Implement `download_model(app, name)` — streaming `reqwest` download to a `.part` temp file, emit `download-progress` events with `{ model, percent }`, rename to final path on completion
- Implement `delete_model(name)` — remove the model file from disk
- Wire all four into the corresponding commands

---

## Phase 3 — Audio Capture & Resampling

**`audio/capture.rs`**
- Implement `start_capture()` → returns a handle/channel; uses `cpal` default input device
- Collect f32 samples into an `Arc<Mutex<Vec<f32>>>`
- Implement `stop_capture()` → drains the buffer and returns owned `Vec<f32>`
- Enforce max recording duration: spawn a timer that auto-stops after `Settings::max_recording_seconds`

**`audio/resample.rs`**
- Implement `resample_to_16k(samples: Vec<f32>, source_rate: u32) -> Vec<f32>` using `rubato`'s `FftFixedIn` resampler
- Skip if the device already runs at 16 kHz

---

## Phase 4 — Transcription

**`transcribe/whisper.rs`**
- Implement `load_model(path: &Path) -> WhisperContext` — wrap `whisper_rs::WhisperContext::new()`
- Hold the context in `AppState` so it is only loaded once (reload when model changes)
- Implement `transcribe(ctx, samples: &[f32], language: &str) -> Result<String, String>` — run on a `tokio::task::spawn_blocking` thread so the async runtime isn't stalled
- Map `language = "auto"` to `""` (whisper.cpp auto-detect)

---

## Phase 5 — Output

**`output/clipboard.rs`**
- Implement `copy_to_clipboard(text: &str)` using `arboard::Clipboard`

**`output/paste.rs`**
- Capture the frontmost app _before_ recording starts using `NSWorkspace` via `objc` crate or an `osascript` subprocess: `osascript -e 'tell application "System Events" to get name of first process whose frontmost is true'`
- Re-activate that app with a second `osascript` call, then simulate `Cmd+V` via `CGEvent` or `osascript`
- Wrap in `auto_paste(app_name: &str, text: &str)` which does: clipboard write → re-activate app → key event

---

## Phase 6 — Hotkey Manager

**`hotkey/manager.rs`**
- Register `tauri-plugin-global-shortcut` with the hotkey from `Settings`
- Add `tauri-plugin-global-shortcut` to `lib.rs` builder (`.plugin(tauri_plugin_global_shortcut::Builder::new().build())`)
- On hotkey press:
  - **Push-to-talk mode**: start recording on `keydown`, stop on `keyup` (Tauri global shortcut only fires press events — use a flag + a second press to simulate hold, _or_ implement with `rdev` crate which supports keydown/keyup)
  - **Toggle mode**: first press starts, second press stops
- On hotkey change (settings update): unregister old shortcut, register new one

> **Note on push-to-talk**: `tauri-plugin-global-shortcut` doesn't expose keyup events. True push-to-talk requires `rdev` crate for raw key events, or ship toggle-only for MVP and add push-to-talk post-MVP.

---

## Phase 7 — IPC Command Wiring

**`commands.rs`** — connect all phases together:

```
start_recording:
  1. Capture frontmost app name (store in AppState)
  2. Start audio capture
  3. Emit `recording-started` event to frontend

stop_recording:
  1. Stop audio capture → raw samples
  2. Resample to 16kHz if needed
  3. Emit `recording-stopped` (overlay shows "transcribing…")
  4. Spawn blocking task → transcribe(samples)
  5. On success: copy_to_clipboard + auto_paste (if enabled) → emit `transcription-complete { text }`
  6. On error: emit `transcription-error { message }`
```

- `set_active_model`: update `AppState.active_model`, reload `WhisperContext` from new model path

---

## Phase 8 — Overlay (Frontend)

**`src/components/Overlay.tsx`**
- Listens to `recording-started`, `recording-stopped`, `transcription-complete`, `transcription-error` via `useTauriEvents`
- States: `idle` → `recording` → `transcribing` → `idle`
- Recording state: pulsing red dot + elapsed timer (local `setInterval`)
- Transcribing state: spinner + "Transcribing…"
- Error state: brief error message before dismissing
- Window is always mounted; show/hide by toggling CSS visibility so the Tauri window stays transparent when idle

**`src/hooks/useTauriEvents.ts`**
- Implement `listen()` subscriptions for all five backend events
- Return typed event payloads

---

## Phase 9 — Settings Panel (Frontend)

**`src/components/Settings.tsx`**
- On mount: `invoke('get_settings')` → populate form state
- Fields: hotkey input, recording mode radio, language select, auto-paste toggle, max duration slider, launch at login toggle
- On save: `invoke('update_settings', { settings })`

**`src/components/ModelManager.tsx`**
- On mount: `invoke('list_models')` → render model table with disk size, RAM, download/delete/activate buttons
- Download button triggers `invoke('download_model', { model })` + subscribes to `download-progress` for a progress bar
- Active model has a checkmark; clicking another downloaded model calls `invoke('set_active_model')`

**`src/App.tsx`**
- Route by window label (`window.__TAURI_INTERNALS__.metadata.currentWindow.label`):
  - `"settings"` → render `<Settings>` + `<ModelManager>`
  - `"overlay"` → render `<Overlay>`

---

## Phase 10 — First Launch & Permissions

- On app start (`lib.rs` setup): check if any model file exists in the models dir
  - If none found: show the settings window automatically so the user can download one
- Microphone permission: `cpal` triggers the macOS permission prompt automatically on first `build_input_stream` call; handle the `PermissionDenied` error and emit a `transcription-error` with a clear message
- Accessibility permission (for paste): before the first paste attempt, check if the process has Accessibility access; if not, open `System Settings → Privacy & Security → Accessibility` using `tauri-plugin-opener` and show an in-app message

---

## Capabilities / Permissions File

Update `src-tauri/capabilities/default.json` to include:
- `core:event:allow-listen` and `core:event:allow-emit`
- `global-shortcut:allow-register` / `allow-unregister`
- `opener:allow-open-url` (for opening System Settings)

---

## Deferred (Post-MVP)

- True push-to-talk keyup detection (`rdev` crate)
- Launch at login (`SMAppService` via `objc` crate)
- Transcription history
- Per-app profiles
- Streaming partial results
