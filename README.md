<p align="center">
  <img width="512" height="512" alt="careless-whisper" src="https://github.com/user-attachments/assets/bde6e505-9564-4267-ae16-1880e9ca269f" />
</p>


# Careless Whisper

A lightweight, always-on desktop app for local voice-to-text transcription. Lives in the system tray / menu bar, records on a global hotkey, transcribes locally with Whisper, and pastes the result into your focused app. No cloud. No data leaves your machine.

Supports **macOS** and **Windows**.

**Website:** [yarivgilad.github.io/careless-whisper](https://yarivgilad.github.io/careless-whisper/)

## Download

Get the latest version from the [Releases](https://github.com/YarivGilad/careless-whisper/releases/latest) page:

| Platform | File |
|---|---|
| macOS (Intel + Apple Silicon) | `.dmg` |
| Windows | `.exe` installer or `.msi` |
| Linux (Debian/Ubuntu) | `.deb` or `.AppImage` |
| Linux (Fedora/RHEL) | `.rpm` or `.AppImage` |

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

1. Download the installer from the [Releases](https://github.com/yarivgilad/careless-whisper/releases) page.
2. Run the installer and follow the prompts.

> The app lives in the **system tray** (bottom-right of your screen).

### Linux

**Fedora / RHEL (`.rpm`)** — `dnf` pulls in every dependency automatically:

```sh
sudo dnf install ./CarelessWhisper-*.x86_64.rpm
```

**Debian / Ubuntu (`.deb`)**:

```sh
sudo apt install ./Careless\ Whisper_*_amd64.deb
```

**Any distro (`.AppImage`)** — no install, just run:

```sh
chmod +x Careless\ Whisper_*_amd64.AppImage
./Careless\ Whisper_*_amd64.AppImage
```

> The app lives in the **system tray**.
>
> **Wayland (KDE/GNOME):**
>
> 1. **Global hotkey** is restricted, so the app listens on a per-user FIFO instead. Bind a custom keyboard shortcut to:
>    ```sh
>    echo toggle > ~/.local/share/careless-whisper/careless-whisper.sock
>    ```
> 2. **Paste** uses `ydotool` (compositors block other synthetic-input methods). Enable its daemon once:
>    ```sh
>    systemctl --user enable --now ydotoold
>    ```
> 3. **KDE Plasma only:** install `kdotool` so paste can refocus the right window (KWin doesn't return focus to the original app after the recording overlay hides):
>    ```sh
>    sudo dnf install kdotool   # Fedora
>    ```
>
> On **X11** none of this is needed — the global hotkey works and paste uses `xdotool`.

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
