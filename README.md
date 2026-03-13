# Careless Whisper

A lightweight, always-on macOS desktop app for local voice-to-text transcription. Lives in the menu bar, records on a global hotkey, transcribes locally with Whisper, and pastes the result into your focused app. No cloud. No data leaves your machine.

---

## Using the App

### Install

1. Build the app: `pnpm tauri build`
2. Open `src-tauri/target/release/bundle/dmg/` — you'll find a `.dmg` file there.
3. Open the DMG and drag **Careless Whisper.app** to your **Applications** folder.
4. Launch it from Applications (or Spotlight).

> The app has no Dock icon — it lives entirely in the **menu bar** (top-right of your screen). Look for the microphone icon.

### First launch

The Settings window will open automatically because no model is downloaded yet.

1. Pick a model and click **Download** (the `base` model is a good starting point — ~142 MB, fast).
2. Wait for the download to finish.
3. macOS will ask for **Microphone** access the first time you record — allow it.
4. Go to **System Settings → Privacy & Security → Accessibility** and enable Careless Whisper so it can paste text into other apps.

### Record and transcribe

1. Click into any text field in any app (your target).
2. Press **Cmd+Shift+Space** — a small recording indicator appears at the top of the screen.
3. Speak.
4. Press **Cmd+Shift+Space** again to stop — the transcribed text is pasted directly where your cursor was.

The hotkey, language, and other options can be changed from **Settings** in the menu bar menu.

---

## Tech Stack

- **Tauri v2** — Desktop framework (system tray, global hotkeys, IPC)
- **Rust** — Backend (audio, transcription, clipboard, OS integration)
- **whisper-rs** — Local Whisper inference via whisper.cpp bindings
- **cpal** — Cross-platform audio capture
- **React + TypeScript** — Minimal frontend (overlay, settings)
- **Tailwind CSS** — Styling
- **Vite** — Frontend bundler
- **pnpm** — Package manager

## Prerequisites

- Rust (via rustup)
- Node.js + pnpm
- Xcode Command Line Tools (macOS)

## Setup

```sh
pnpm install
pnpm tauri dev
```

## macOS Permissions

The app requires two permissions:
- **Microphone** — to record your voice
- **Accessibility** — to paste transcribed text into other apps (System Settings → Privacy & Security → Accessibility)

## Project Structure

```
careless-whisper/
├── src-tauri/              # Rust backend
│   └── src/
│       ├── audio/          # Mic capture (cpal) + resampling (rubato)
│       ├── transcribe/     # whisper-rs wrapper
│       ├── hotkey/         # Global hotkey registration
│       ├── output/         # Clipboard write + Cmd+V simulation
│       ├── models/         # Model download & management
│       ├── config/         # Settings persistence (JSON)
│       ├── tray.rs         # System tray setup
│       └── commands.rs     # Tauri IPC handlers
└── src/                    # React frontend
    ├── components/
    │   ├── Overlay.tsx     # Recording indicator
    │   ├── Settings.tsx    # Settings panel
    │   └── ModelManager.tsx
    ├── hooks/
    │   └── useTauriEvents.ts
    └── styles/
        └── globals.css
```

## Default Hotkey

`Cmd+Shift+Space` — hold to record (push-to-talk), release to transcribe and paste.

## Whisper Models

On first launch the app will prompt you to download a model. Models are stored in `~/Library/Application Support/careless-whisper/models/`.

| Model | Size | Speed |
|---|---|---|
| tiny | ~75 MB | Fastest |
| base | ~142 MB | Fast (default) |
| small | ~466 MB | Moderate |
| medium | ~1.5 GB | Slow |
| large-v3 | ~3 GB | Slowest |
