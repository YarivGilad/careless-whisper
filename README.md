<p align="center">
  <img width="512" height="512" alt="careless-whisper" src="https://github.com/user-attachments/assets/bde6e505-9564-4267-ae16-1880e9ca269f" />
</p>


# Careless Whisper

Careless Whisper is a lightweight, always-on desktop app for local voice-to-text transcription. It lives in the system tray / menu bar, records from a global hotkey, transcribes locally with Whisper, and keeps your audio on your machine.

The original stop-to-transcribe workflow is still here: press the hotkey, speak, press it again, and the final transcript is pasted into your focused app. Optional realtime mode shows partial transcription text in the overlay while you are still recording.

No cloud. No accounts. No data leaves your machine.

Supports **macOS** and **Windows**.

## Realtime Dictation

- **Realtime transcription mode** - optional partial transcript updates while recording.
- **Live overlay text** - the recording overlay can show the growing transcript instead of only a timer.
- **Classic mode preserved** - final transcription and paste still use the reliable batch path.
- **Local-first experiments** - realtime POC notes, tuning docs, and test scripts are kept in `docs/` and `poc/`.

Realtime mode is experimental. It currently works by transcribing short chunks while capture continues, so it is closer to near-realtime dictation than character-by-character streaming. Batch mode remains the dependable fallback.

**Website:** [yarivgilad.github.io/careless-whisper](https://yarivgilad.github.io/careless-whisper/)

## Download

Get the latest version from the [Releases](https://github.com/YarivGilad/careless-whisper/releases/latest) page:

| Platform | File |
|---|---|
| macOS (Intel + Apple Silicon) | `.dmg` |
| Windows | `.exe` installer or `.msi` |
| Linux | `.deb` or `.AppImage` |

---

## Install

### macOS

1. Download the `.dmg` file above.
2. Open it and drag **Careless Whisper** to your **Applications** folder.
3. Launch from Applications (or Spotlight).

> The app has no Dock icon — it lives in the **menu bar** (top-right of your screen).

#### "Careless Whisper is damaged and can't be opened"

Don't worry — the app is perfectly fine! macOS shows this warning for apps that aren't code-signed with Apple's $99/year Developer certificate. This is standard for open-source projects that are trying to be given away for free and avoid the Apple penalty for creative generosities. Until this project gets funded (don't hold your breath — it's a weekend side project), macOS users are welcome to run this one-time fix in Terminal:

If you dragged the app to Applications:

```sh
xattr -cr "/Applications/Careless Whisper.app"
```

If you're running it straight from the DMG:

```sh
xattr -cr "/Volumes/Careless Whisper/Careless Whisper.app"
```

After that, the app will open normally.

### Windows

1. Download the installer from the [Releases](https://github.com/YarivGilad/careless-whisper/releases) page.
2. Run the installer and follow the prompts.

> The app lives in the **system tray** (bottom-right of your screen).

### First launch

The Settings window will open automatically because no model is downloaded yet.

1. Pick a model and click **Download** (the `base` model is a good starting point — ~142 MB, fast).
2. Wait for the download to finish.
3. Your OS will ask for **Microphone** access the first time you record — allow it.
4. **macOS only:** Go to **System Settings → Privacy & Security → Accessibility** and enable Careless Whisper so it can paste text into other apps.

### Classic dictation

1. Click into any text field in any app (your target).
2. Press the hotkey (default: **Cmd+Shift+Space** on macOS, **Ctrl+Shift+Space** on Windows) — a small recording indicator appears.
3. Speak.
4. Press the hotkey again to stop — the transcribed text is pasted directly where your cursor was.

Click the menu-bar icon to open **Settings**. Secondary-click the icon to open
the app menu with language and quit actions. Settings save automatically as you
change them.

The Settings **Start Recording** button hides Settings briefly before recording
starts. Focus the destination text field when Settings closes so the app can
capture the target cursor. The global hotkey is still the fastest and most
reliable way to start from an already-focused text field.

### Realtime transcription

Enable **Realtime transcription** in Settings to show partial transcript text in
the overlay while recording. The recording bubble also shows the current mode
and lets you arm or disable realtime mode while a recording is active. The
bubble can be dragged if it appears in an awkward spot.

When **Auto-paste after transcription** is also enabled, realtime chunks are
typed into the focused app during recording through the captured hotkey target.
When you stop, realtime mode skips the old full batch transcription path because
the text was already produced during recording. Very short realtime recordings
that stop before any chunk returns fall back to the classic final pass.

Platform note: realtime capture and Whisper chunking are cross-platform Rust
paths, but the direct live-typing path is currently macOS-specific. Windows and
Linux keep the clipboard-plus-paste fallback for realtime chunks and need
dedicated QA before claiming parity, especially on Linux Wayland sessions.

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

## Development Docs

Realtime dictation work is tracked separately so the POC, plan, and open
questions stay discoverable.

| Path | Purpose |
|---|---|
| `docs/realtime_whisper_discussion.md` | Original realtime dictation architecture discussion and feasibility notes. |
| `docs/realtime_whisper/plan.md` | Phase plan for realtime dictation, from standalone POC through app integration. |
| `docs/realtime_whisper/implementation_tracker.md` | POC status, completed realtime experiments, and next test commands. |
| `docs/realtime_whisper/current_issues.md` | Active risks, resolved findings, and tuning questions. |
| `poc/README.md` | POC runbook and file map. Start here for local realtime experiments. |
| `poc/realtime_mic_poc.py` | Continuous/chunked ffmpeg capture runner that invokes local `whisper.cpp`. |
| `poc/run_whisper_stream.sh` | Wrapper for the `whisper-stream` comparison path. |
| `poc/test_samples.md` | Fixed English and Hebrew read-aloud samples for repeatable comparisons. |
| `docs/security/audit-2026-03-22.md` | Security audit notes. |
| `docs/index.html` and `docs/assets/` | Static project website assets. |

Generated POC artifacts live under `poc/audio/`, `poc/models/`, and
`poc/vendor/`; they are documented in `poc/README.md` and ignored by git.

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
