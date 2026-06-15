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
- `settings` — 600×500, shown from tray menu. Native decorations on macOS/Windows; on Linux the native decorations are dropped (`set_decorations(false)` in `tray.rs`) and a custom in-webview titlebar (`components/TitleBar.tsx`) is rendered instead — KWin/Wayland doesn't deliver native titlebar button clicks to the WebKitGTK surface, so the native min/close buttons are dead. The custom buttons call Tauri's JS window API (`minimize()`, `hide()`), and a `data-tauri-drag-region` makes the bar draggable.
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
- `components/TitleBar.tsx` — custom window titlebar, rendered by `App.tsx` only on Linux (`navigator.userAgent` check); needs `core:window` `allow-minimize`/`allow-hide`/`allow-start-dragging` in `capabilities/default.json`
- `hooks/useTauriEvents.ts` — subscribes to all backend events

## macOS Entitlements

The app requires **Microphone** permission (for audio capture) and **Accessibility** permission (for simulated paste via Apple Events). Both are declared in `src-tauri/Info.plist`. During development on a real device, macOS will prompt on first use.

## Key Constraints

- Supports macOS and Windows; platform-specific code is isolated behind `#[cfg(target_os)]` in `output/paste.rs` and `lib.rs`
- whisper-rs links against whisper.cpp at compile time; the first `cargo build` will compile whisper.cpp from source (slow)
- Metal GPU acceleration is enabled by default (macOS only); on Windows build with `--no-default-features` for CPU-only
- Models are ggml format; downloaded from Hugging Face, not bundled with the app
- Minimum macOS version: 12.0 (Monterey)

## Linux (Fedora) Dev Setup

Verified working on Fedora 44 / KDE (Wayland). Two flags are **required** for every Linux build/run:

```bash
# dev
WHISPER_DONT_GENERATE_BINDINGS=1 pnpm tauri dev -- --no-default-features
# plain cargo
WHISPER_DONT_GENERATE_BINDINGS=1 cargo build --no-default-features
```

- `--no-default-features` drops the `metal` feature (macOS-only GPU); Linux runs CPU inference. Without it the build fails on `ggml_backend_metal_log_set_callback`.
- `WHISPER_DONT_GENERATE_BINDINGS=1` makes `whisper-rs-sys` copy its prebuilt `src/bindings.rs` instead of running bindgen. Fedora's clang (22.x) is too new for whisper-rs-sys 0.10's bindgen, which otherwise emits opaque (`_address`-only) structs → 72 `unknown field` errors. If you change whisper crate versions, `cargo clean -p whisper-rs-sys -p whisper-rs` before rebuilding (env changes alone don't trigger a build-script rerun).

System deps (dnf): `webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel openssl-devel alsa-lib-devel cmake clang clang-devel gcc-c++ xdotool patchelf file`. Global hotkey is limited under Wayland — app falls back to a FIFO socket at `~/.local/share/careless-whisper/careless-whisper.sock` (token in `fifo.token`).

**Paste backends** (`output/paste.rs`): on X11, `xdotool` captures the active window and sends Ctrl+V. On Wayland the keystroke must go through **ydotool** (kernel `/dev/uinput`) — `wtype`'s virtual-keyboard protocol is wlroots-only and KWin/Mutter reject it, and `xdotool` only reaches XWayland apps. ydotool needs `ydotoold` running: `systemctl --user enable --now ydotoold` (the active login session gets a `/dev/uinput` ACL automatically).

Focus is the subtle part: despite the overlay being `focus: false`, keyboard focus does **not** reliably return to the original app after the overlay hides on KWin, so a blind ydotool Ctrl+V lands nowhere. Fix: on KDE, `get_frontmost_target()` captures the focused window with `kdotool getactivewindow` (stored as `kwin:<uuid>`); before pasting, `paste_kwin()` runs `kdotool windowactivate <uuid>` to refocus it, then injects Ctrl+V. `kdotool` drives KWin's scripting API over DBus — it works on Wayland where xdotool can't. If kdotool is absent we fall back to focus-blind `paste_wayland()` (ydotool → wtype → xdotool), which is correct on GNOME/wlroots but flaky on KWin. So KDE Wayland needs **both** `ydotoold` running and `kdotool` installed.

**Clipboard** (`output/clipboard.rs`): arboard relinquishes the Wayland clipboard offer when its `Clipboard` object drops, so a set-then-return leaves the selection alive only momentarily (a clipboard manager / kdeconnect can read it, but it's gone before our paste keystroke fires — symptom: text reaches a synced phone but the local paste is empty). On Wayland we therefore set/read via `wl-copy`/`wl-paste` (from `wl-clipboard`), which fork a daemon that holds the selection. arboard is still used on X11/macOS/Windows.

### RPM packaging (Fedora/RHEL)

The `rpm` bundle target is enabled in `tauri.conf.json` (`bundle.targets` + `bundle.linux.rpm`). Runtime deps are declared in Fedora package names (`alsa-lib`, `webkit2gtk4.1`, `gtk3`, `libappindicator-gtk3`, `xdotool`, `wtype`) so `sudo dnf install ./*.rpm` pulls everything. Post-install/remove scriptlets live in `src-tauri/rpm/scripts/` (refresh desktop + icon caches; user models are preserved on uninstall).

```bash
WHISPER_DONT_GENERATE_BINDINGS=1 pnpm tauri build --bundles rpm -- --no-default-features
# output: src-tauri/target/release/bundle/rpm/*.rpm
```

Tauri's rpm bundler is pure-Rust (no `rpmbuild` needed), so CI builds the `.rpm` on the existing `ubuntu-22.04` runner. The release workflow strips the space from the filename (`Careless Whisper-…` → `CarelessWhisper-…`) and attaches it to every `v*` release.
