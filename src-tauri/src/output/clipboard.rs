use arboard::Clipboard;

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    // On Wayland, arboard relinquishes the clipboard offer as soon as its
    // `Clipboard` object is dropped, so a set-then-return leaves the selection
    // alive only briefly — long enough for a clipboard manager / kdeconnect to
    // read it, but gone before our paste keystroke fires. `wl-copy` forks a
    // daemon that holds the selection until it's replaced, so it persists.
    #[cfg(target_os = "linux")]
    if is_wayland() {
        return wl_copy(text);
    }

    // Retry up to 3 times — on Windows the clipboard can be transiently locked
    // by other applications (browsers, password managers, clipboard managers).
    let mut last_err = String::new();
    for attempt in 0..3 {
        match try_copy(text) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = e;
                if attempt < 2 {
                    log::warn!(
                        "[clipboard] attempt {} failed: {}, retrying...",
                        attempt + 1,
                        last_err
                    );
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
    }
    Err(format!("Clipboard failed after 3 attempts: {}", last_err))
}

fn try_copy(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}

pub fn read_clipboard() -> Option<String> {
    #[cfg(target_os = "linux")]
    if is_wayland() {
        return wl_paste();
    }

    Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())
}

#[cfg(target_os = "linux")]
fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false)
}

/// Set the Wayland clipboard via `wl-copy`, which forks a background process to
/// serve the selection so it survives after this call returns.
#[cfg(target_os = "linux")]
fn wl_copy(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run wl-copy: {e}"))?;

    child
        .stdin
        .take()
        .ok_or("wl-copy stdin unavailable")?
        .write_all(text.as_bytes())
        .map_err(|e| format!("Failed to write to wl-copy: {e}"))?;

    // wl-copy reads stdin, then daemonizes to hold the selection; the
    // foreground process exits 0 once the offer is established.
    let status = child
        .wait()
        .map_err(|e| format!("wl-copy did not complete: {e}"))?;
    if !status.success() {
        return Err(format!("wl-copy exited with {status}"));
    }
    Ok(())
}

/// Read the Wayland clipboard via `wl-paste`. Returns None when the clipboard is
/// empty (wl-paste exits non-zero) or unavailable.
#[cfg(target_os = "linux")]
fn wl_paste() -> Option<String> {
    let output = std::process::Command::new("wl-paste")
        .arg("--no-newline")
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}
