# SuperWhisper Clone — Product Requirements Document

## Overview

A lightweight, always-on macOS desktop application for local voice-to-text transcription. The app runs in the system tray, listens for a global hotkey, records speech, transcribes it locally using Whisper, and pastes the result into whatever application is focused. All processing happens on-device — no cloud APIs, no data leaves the machine.

**Codename:** Whisper Tap (or whatever you'd like — rename freely)

---

## Tech Stack

| Layer | Technology | Notes |
|---|---|---|
| Desktop framework | **Tauri v2** | System tray, global hotkeys, window management, IPC |
| Backend language | **Rust** | Audio capture, transcription, clipboard, OS integration |
| Transcription engine | **whisper-rs** (whisper.cpp bindings) | Local, on-device inference. Ships with or downloads models |
| Audio capture | **cpal** | Cross-platform mic input capture |
| Frontend | **React + TypeScript** | Minimal UI: settings panel, transcription overlay |
| Styling | **Tailwind CSS** | Utility-first, keep it simple |
| Build tooling | **Vite** | Frontend bundler (Tauri v2 default) |
| Package manager | **pnpm** | For the frontend workspace |

---

## Architecture

```
┌─────────────────────────────────────────────────┐
│                   macOS                          │
│                                                  │
│  ┌──────────────┐     ┌──────────────────────┐  │
│  │  System Tray  │     │   Overlay Window      │  │
│  │  (Tauri)      │     │   (React/TS)          │  │
│  └──────┬───────┘     └──────────▲───────────┘  │
│         │                        │ IPC           │
│  ┌──────▼────────────────────────┴───────────┐  │
│  │            Rust Backend (Tauri Core)        │  │
│  │                                             │  │
│  │  ┌───────────┐  ┌───────────┐  ┌────────┐  │  │
│  │  │ Audio     │  │ Whisper   │  │ Paste  │  │  │
│  │  │ Capture   │→ │ Transcribe│→ │ Output │  │  │
│  │  │ (cpal)    │  │ (whisper- │  │ (clip- │  │  │
│  │  │           │  │  rs)      │  │  board) │  │  │
│  │  └───────────┘  └───────────┘  └────────┘  │  │
│  │                                             │  │
│  │  ┌───────────┐  ┌───────────┐              │  │
│  │  │ Hotkey    │  │ Model     │              │  │
│  │  │ Manager   │  │ Manager   │              │  │
│  │  └───────────┘  └───────────┘              │  │
│  └─────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

### Data Flow

1. User presses global hotkey (e.g., `Cmd+Shift+Space`)
2. App shows a small recording indicator overlay
3. Audio is captured from the default input device via `cpal`
4. Audio is buffered in memory as f32 PCM samples at 16kHz (Whisper's expected format)
5. User releases the hotkey (or presses it again to stop)
6. Audio buffer is sent to `whisper-rs` for transcription
7. Transcribed text is placed on the system clipboard
8. App simulates `Cmd+V` paste into the previously focused application
9. Overlay dismisses

### Key Design Decisions

- **Push-to-talk model**: Hold the hotkey to record, release to transcribe. Also support toggle mode (press to start, press to stop) as a setting.
- **Overlay, not a window**: The recording indicator should be a small floating overlay (not a full window) that doesn't steal focus from the user's current application.
- **Focus preservation**: The app must remember which application was focused before recording started and restore focus + paste into it.
- **Model management**: On first launch, prompt the user to download a Whisper model. Support multiple model sizes (tiny, base, small, medium, large). Store models in `~/Library/Application Support/whisper-tap/models/`.

---

## Features — MVP (v0.1)

These are the minimum features for a working, usable tool:

### 1. System Tray Integration
- App lives in the macOS menu bar with a simple icon
- Tray menu with: current status, settings, quit
- No dock icon (LSUIElement or Tauri equivalent)

### 2. Global Hotkey
- Default: `Cmd+Shift+Space` (configurable)
- Works regardless of which app is focused
- Two modes: push-to-talk (hold to record) and toggle (press to start/stop)

### 3. Audio Recording
- Capture from default system input device
- Record as f32 PCM at 16kHz mono
- If the input device sample rate differs, resample to 16kHz (use `rubato` or `dasp_sample` crate)
- Show recording duration in the overlay
- Set a max recording duration (default: 120 seconds) to prevent accidental endless recordings

### 4. Local Transcription
- Use `whisper-rs` (Rust bindings for whisper.cpp)
- Run transcription on a background thread — never block the UI
- Support model selection (tiny → large)
- Default to `base` model for balance of speed and quality
- Language: auto-detect, with option to pin a language in settings

### 5. Text Output
- Place transcribed text on clipboard
- Simulate `Cmd+V` to paste into the focused app
- Optionally, just copy to clipboard without auto-paste (setting)

### 6. Recording Overlay
- Small, floating, non-focusable window
- Shows: recording indicator (pulsing dot or waveform), elapsed time
- Positioned at top-center of screen (configurable)
- Dismisses automatically after paste

### 7. Settings Panel
- Accessible from tray menu
- Settings to expose:
  - Hotkey configuration
  - Recording mode (push-to-talk vs toggle)
  - Whisper model selection (with download/delete controls)
  - Language (auto or specific)
  - Auto-paste on/off
  - Max recording duration
  - Launch at login on/off
- Persist settings to a JSON config file in `~/Library/Application Support/whisper-tap/config.json`

### 8. Model Management
- On first launch, detect if no model is present and prompt download
- Download models from Hugging Face (ggml format for whisper.cpp)
- Show download progress
- Allow switching between downloaded models
- Display model size and estimated VRAM/RAM usage

---

## Features — Post-MVP

These are NOT in scope for v0.1 but should be kept in mind architecturally:

- **Transcription history**: Searchable log of past transcriptions with timestamps
- **Per-app profiles**: Different settings (model, language) for different apps
- **Audio preprocessing**: Noise suppression before transcription (e.g., via `nnnoiseless` crate)
- **Streaming transcription**: Show partial results as they're generated (requires whisper.cpp streaming support)
- **Custom vocabulary / prompt**: Pass a prompt to Whisper to improve accuracy for domain-specific terms
- **Windows / Linux support**: Tauri v2 supports all three — platform-specific code should be isolated behind traits/abstractions
- **Multiple input devices**: Let the user pick a specific mic
- **Text post-processing**: Auto-capitalization, punctuation cleanup, custom find-replace rules
- **AI post-processing**: Optional pass through a local LLM to clean up / reformat transcription

---

## Project Structure

```
whisper-tap/
├── src-tauri/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs              # Tauri entry point
│   │   ├── lib.rs               # Module exports
│   │   ├── audio/
│   │   │   ├── mod.rs
│   │   │   ├── capture.rs       # Mic recording via cpal
│   │   │   └── resample.rs      # Sample rate conversion
│   │   ├── transcribe/
│   │   │   ├── mod.rs
│   │   │   └── whisper.rs       # whisper-rs wrapper
│   │   ├── hotkey/
│   │   │   ├── mod.rs
│   │   │   └── manager.rs       # Global hotkey registration
│   │   ├── output/
│   │   │   ├── mod.rs
│   │   │   ├── clipboard.rs     # Clipboard write
│   │   │   └── paste.rs         # Simulate Cmd+V
│   │   ├── models/
│   │   │   ├── mod.rs
│   │   │   └── downloader.rs    # Model download + management
│   │   ├── config/
│   │   │   ├── mod.rs
│   │   │   └── settings.rs      # Settings persistence
│   │   ├── tray.rs              # System tray setup
│   │   └── commands.rs          # Tauri IPC command handlers
│   ├── tauri.conf.json
│   ├── capabilities/            # Tauri v2 permissions
│   └── icons/
├── src/                         # Frontend (React + TS)
│   ├── App.tsx
│   ├── main.tsx
│   ├── components/
│   │   ├── Overlay.tsx          # Recording indicator
│   │   ├── Settings.tsx         # Settings panel
│   │   └── ModelManager.tsx     # Model download UI
│   ├── hooks/
│   │   └── useTauriEvents.ts    # Listen for backend events
│   └── styles/
│       └── globals.css          # Tailwind imports
├── package.json
├── pnpm-lock.yaml
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.js
└── README.md
```

---

## Rust Crate Dependencies (key ones)

```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon", "global-shortcut"] }
whisper-rs = "0.12"             # whisper.cpp bindings
cpal = "0.15"                   # audio capture
rubato = "0.15"                 # resampling
arboard = "3"                   # clipboard (cross-platform)
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["stream"] } # model downloads
dirs = "5"                      # platform dirs (app support path)
```

> **Note**: Version numbers are approximate. Use the latest compatible versions at project creation time. Check that `whisper-rs` and `cpal` versions are compatible with the Tauri v2 async runtime.

---

## macOS-Specific Concerns

### Permissions
- **Microphone access**: The app must request mic permission. Add `NSMicrophoneUsageDescription` to `Info.plist` (Tauri config).
- **Accessibility access**: Simulating `Cmd+V` requires Accessibility permissions. Add `NSAppleEventsUsageDescription`. Guide the user to enable it in System Settings → Privacy & Security → Accessibility.
- Handle permission denial gracefully — show a clear message explaining what's needed and why.

### Focus Management
- Before starting a recording, capture the frontmost application (using `NSWorkspace` or AppleScript via `std::process::Command`).
- After transcription, re-activate that application before pasting.
- The overlay window must use `NSPanel` behavior (or Tauri equivalent): always on top, non-activating, no shadow in the dock.

### Launch at Login
- Use `SMAppService` (modern macOS) or a LaunchAgent plist to register as a login item.
- Expose this as a setting.

### Code Signing
- For personal use, ad-hoc signing is fine (`codesign -s -`)
- For distribution, will need a Developer ID certificate
- Notarization required for distribution outside the App Store

---

## IPC Commands (Tauri)

The frontend communicates with the Rust backend via Tauri commands:

| Command | Direction | Purpose |
|---|---|---|
| `start_recording` | Frontend → Backend | Begin audio capture |
| `stop_recording` | Frontend → Backend | Stop capture, trigger transcription |
| `get_settings` | Frontend → Backend | Load current settings |
| `update_settings` | Frontend → Backend | Save updated settings |
| `list_models` | Frontend → Backend | Get available + downloaded models |
| `download_model` | Frontend → Backend | Start model download |
| `delete_model` | Frontend → Backend | Remove a downloaded model |
| `set_active_model` | Frontend → Backend | Switch the active model |

### Events (Backend → Frontend)

| Event | Purpose |
|---|---|
| `recording-started` | Overlay should appear |
| `recording-stopped` | Overlay shows "transcribing…" |
| `transcription-complete` | Includes text result, overlay dismisses |
| `transcription-error` | Error message to display |
| `download-progress` | Model download percentage |

---

## Configuration Schema

```json
{
  "hotkey": "CmdOrCtrl+Shift+Space",
  "recording_mode": "push_to_talk",
  "active_model": "base",
  "language": "auto",
  "auto_paste": true,
  "max_recording_seconds": 120,
  "launch_at_login": false,
  "overlay_position": "top_center"
}
```

---

## Model Registry

Supported models with approximate specs:

| Model | Params | Disk Size | RAM Usage | Relative Speed |
|---|---|---|---|---|
| tiny | 39M | ~75 MB | ~390 MB | Fastest |
| base | 74M | ~142 MB | ~500 MB | Fast |
| small | 244M | ~466 MB | ~1 GB | Moderate |
| medium | 769M | ~1.5 GB | ~2.6 GB | Slow |
| large-v3 | 1550M | ~3 GB | ~5 GB | Slowest |

Download URLs follow the pattern:
`https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{model}.bin`

---

## Success Criteria for MVP

The app is "done" for MVP when:

1. It can be launched, appears only in the menu bar (no dock icon)
2. Pressing the global hotkey starts recording with a visible overlay
3. Releasing the hotkey (or pressing again) stops recording
4. Audio is transcribed locally using the selected Whisper model
5. Transcribed text is pasted into the previously focused application
6. User can change settings (hotkey, model, language, mode) via a settings window
7. User can download and switch between Whisper models
8. The app handles mic permission and accessibility permission gracefully
9. Transcription of a 30-second clip completes in under 10 seconds on Apple Silicon using the `base` model
10. Memory usage stays under 300 MB while idle (excluding model size)
