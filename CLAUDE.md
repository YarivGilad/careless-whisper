# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

Careless Whisper is a desktop app for local voice-to-text transcription using OpenAI Whisper. On macOS it lives in the menu bar (no dock icon — `LSUIElement = true`); on Windows it lives in the system tray. Press a hotkey → speak → transcribed text is pasted into the focused app. Supports macOS and Windows.

## Development Commands

```bash
# Run in dev mode (starts Vite + Tauri watcher)
pnpm tauri dev

# Build for production
pnpm tauri build

# Frontend only (no Rust)
pnpm dev

# Type-check frontend
pnpm build
```

Rust is built via Cargo through the Tauri CLI — there are no standalone `cargo` commands needed for normal development. To iterate on Rust only, you can run `cargo build` inside `src-tauri/`.

## Architecture

**Two-process model:** Vite/React frontend renders in Tauri webview windows; all system work (audio, transcription, file I/O) happens in Rust.

**IPC boundary** — frontend calls Rust via `invoke()`, Rust pushes events back via Tauri's event system:
- Commands (frontend → Rust): `start_recording`, `stop_recording`, `get_settings`, `update_settings`, `list_models`, `download_model`, `delete_model`, `set_active_model`
- Events (Rust → frontend): `recording-started`, `recording-stopped`, `transcription-complete`, `transcription-error`, `download-progress`

**Two windows** (both start hidden):
- `settings` — 600×500, standard decorations, shown from tray menu
- `overlay` — 280×80, transparent, always-on-top, no decorations; shown during recording

**Rust module layout** (`src-tauri/src/`):
- `commands.rs` — all `#[tauri::command]` handlers (the IPC boundary)
- `config/settings.rs` — `Settings` struct (serde, persisted to `~/Library/Application Support/careless-whisper/config.json`)
- `audio/capture.rs` — cpal recording, f32 PCM mono at 16 kHz
- `audio/resample.rs` — rubato resampling to match whisper's expected format
- `transcribe/whisper.rs` — whisper-rs inference (runs on background thread)
- `models/downloader.rs` — reqwest streaming download of ggml models from Hugging Face → `~/Library/Application Support/careless-whisper/models/`
- `output/clipboard.rs` + `output/paste.rs` — arboard clipboard write + platform-specific paste simulation (CoreGraphics on macOS, SendInput on Windows)
- `hotkey/manager.rs` — tauri-plugin-global-shortcut registration
- `tray.rs` — tray icon + menu (Settings / Quit)

**Frontend** (`src/`):
- `App.tsx` — entry point (currently Tauri scaffold placeholder)
- `components/Settings.tsx` — settings UI
- `components/Overlay.tsx` — recording indicator overlay
- `components/ModelManager.tsx` — model download/delete UI
- `hooks/useTauriEvents.ts` — subscribes to all backend events

## macOS Entitlements

The app requires **Microphone** permission (for audio capture) and **Accessibility** permission (for simulated paste via Apple Events). Both are declared in `src-tauri/Info.plist`. During development on a real device, macOS will prompt on first use.

## Key Constraints

- Supports macOS and Windows; platform-specific code is isolated behind `#[cfg(target_os)]` in `output/paste.rs` and `lib.rs`
- whisper-rs links against whisper.cpp at compile time; the first `cargo build` will compile whisper.cpp from source (slow)
- Metal GPU acceleration is enabled by default (macOS only); on Windows build with `--no-default-features` for CPU-only
- Models are ggml format; downloaded from Hugging Face, not bundled with the app
- Minimum macOS version: 12.0 (Monterey)
