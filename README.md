<p align="center">
  <img src="src-tauri/custom-icons/big-icon-cw.png" alt="Careless Whisper" width="200" />
</p>

# Careless Whisper

A lightweight, always-on desktop app for local voice-to-text transcription. Lives in the system tray / menu bar, records on a global hotkey, transcribes locally with Whisper, and pastes the result into your focused app. No cloud. No data leaves your machine.

Supports **macOS** and **Windows**.

## Download

| Platform | Link |
|---|---|
| macOS (Apple Silicon) | [Download .dmg](https://github.com/yarivgilad/careless-whisper/releases/latest/download/Careless.Whisper_0.1.0_aarch64.dmg) |
| Windows | Coming soon |

> All downloads are on the [Releases](https://github.com/yarivgilad/careless-whisper/releases) page.

---

## Install

### macOS

1. Download the `.dmg` file above.
2. Open it and drag **Careless Whisper** to your **Applications** folder.
3. Launch from Applications (or Spotlight).

> The app has no Dock icon — it lives in the **menu bar** (top-right of your screen).

### Windows

1. Download the installer from the [Releases](https://github.com/yarivgilad/careless-whisper/releases) page.
2. Run the installer and follow the prompts.

> The app lives in the **system tray** (bottom-right of your screen).

### First launch

The Settings window will open automatically because no model is downloaded yet.

1. Pick a model and click **Download** (the `base` model is a good starting point — ~142 MB, fast).
2. Wait for the download to finish.
3. Your OS will ask for **Microphone** access the first time you record — allow it.
4. **macOS only:** Go to **System Settings → Privacy & Security → Accessibility** and enable Careless Whisper so it can paste text into other apps.

### Record and transcribe

1. Click into any text field in any app (your target).
2. Press the hotkey (default: **Cmd+Shift+Space** on macOS, **Ctrl+Shift+Space** on Windows) — a small recording indicator appears.
3. Speak.
4. Press the hotkey again to stop — the transcribed text is pasted directly where your cursor was.

The hotkey, language, and other options can be changed from **Settings** in the tray menu.

## Default Hotkey

`Cmd+Shift+Space` (macOS) / `Ctrl+Shift+Space` (Windows) — press to start recording, press again to stop, transcribe, and paste.

## Whisper Models

On first launch the app will prompt you to download a model. Models are stored locally on your machine.

| Model | Size | Speed |
|---|---|---|
| tiny | ~75 MB | Fastest |
| base | ~142 MB | Fast (recommended) |
| small | ~466 MB | Moderate |
| medium | ~1.5 GB | Slow |
| large-v3 | ~3 GB | Slowest |

## Permissions

### macOS

- **Microphone** — to record your voice
- **Accessibility** — to paste transcribed text into other apps (System Settings → Privacy & Security → Accessibility)

### Windows

- No special permissions needed.

---

## Building from Source

<details>
<summary>For developers who want to build the app themselves</summary>

### Prerequisites

- Rust (via rustup)
- Node.js + pnpm
- macOS: Xcode Command Line Tools
- Windows: Visual Studio Build Tools (C++ workload)

### Development

```sh
pnpm install
pnpm tauri dev
```

On Windows, disable the Metal feature (macOS-only GPU acceleration):
```sh
pnpm tauri dev -- --no-default-features
```

### Production Build

```sh
pnpm tauri build
```

### Tech Stack

- **Tauri v2** — Desktop framework (system tray, global hotkeys, IPC)
- **Rust** — Backend (audio, transcription, clipboard, OS integration)
- **whisper-rs** — Local Whisper inference via whisper.cpp bindings (Metal GPU on macOS, CPU on Windows)
- **cpal** — Cross-platform audio capture
- **React + TypeScript** — Frontend (overlay, settings)
- **Vite** — Frontend bundler

### Project Structure

```
src-tauri/src/
├── audio/          # Mic capture + resampling
├── transcribe/     # whisper-rs wrapper
├── hotkey/         # Global hotkey registration
├── output/         # Clipboard + paste simulation
├── models/         # Model download & management
├── config/         # Settings persistence
├── tray.rs         # System tray setup
└── commands.rs     # Tauri IPC handlers

src/
├── components/     # Overlay, Settings, ModelManager
├── hooks/          # Tauri event subscriptions
└── styles/         # CSS
```

</details>
