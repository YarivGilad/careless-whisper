#!/bin/sh
# RPM post-remove scriptlet for Careless Whisper (runs as root after removal).
# Refreshes desktop + icon caches so the stale menu entry / icon disappears.
# Guarded and always exits 0 so it never aborts the dnf transaction.
# Per-user data (~/.local/share/careless-whisper, config, models) is left in
# place on purpose — uninstalling should not delete a user's downloaded models.

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications >/dev/null 2>&1 || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi

exit 0
