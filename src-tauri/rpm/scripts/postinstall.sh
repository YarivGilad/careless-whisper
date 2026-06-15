#!/bin/sh
# RPM post-install scriptlet for Careless Whisper (runs as root after unpack).
# Refreshes desktop + icon caches so the menu entry and tray icon appear
# immediately, without waiting for a relogin. Every step is guarded and the
# script always exits 0 so a missing helper never aborts the dnf transaction.

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications >/dev/null 2>&1 || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi

cat <<'EOF'
Careless Whisper installed.

  • Launch it from your application menu — it lives in the system tray.
  • On first run, pick and download a Whisper model in Settings.

Wayland note (KDE/GNOME): global hotkeys are limited under Wayland, so the
app listens on a per-user FIFO instead. Bind a custom shortcut to:

  echo toggle > ~/.local/share/careless-whisper/careless-whisper.sock

(The auth token is created automatically on first launch.)

Pasting on Wayland: KWin/Mutter block synthetic keystrokes, so paste uses
ydotool (kernel uinput). Enable the daemon once, per user:

  systemctl --user enable --now ydotoold

(Requires access to /dev/uinput — usually granted to your active login
session automatically. On X11 this is not needed; xdotool is used instead.)

KDE Plasma users need kdotool (installed as a dependency) so paste can
refocus the original window after the recording overlay hides.
EOF

exit 0
