/// Platform-specific identifier for the focused application/window.
/// - macOS: process ID (pid_t)
/// - Windows: window handle (HWND as isize)
/// - Linux: X11 window ID as string, or "wayland" marker
#[cfg(target_os = "macos")]
pub type FocusTarget = i32;

#[cfg(target_os = "windows")]
pub type FocusTarget = isize;

#[cfg(target_os = "linux")]
pub type FocusTarget = String;

/// Returns the current frontmost application/window target.
#[cfg(target_os = "macos")]
pub fn get_frontmost_target() -> Option<FocusTarget> {
    use objc2::msg_send;
    use objc2::runtime::AnyClass;

    unsafe {
        let cls = AnyClass::get(c"NSWorkspace")?;
        let workspace: *mut objc2::runtime::AnyObject = msg_send![cls, sharedWorkspace];
        if workspace.is_null() {
            return None;
        }
        let app: *mut objc2::runtime::AnyObject = msg_send![workspace, frontmostApplication];
        if app.is_null() {
            return None;
        }
        let pid: i32 = msg_send![app, processIdentifier];
        Some(pid)
    }
}

/// Activates the target app and simulates Cmd+V via CoreGraphics CGEventPostToPid.
#[cfg(target_os = "macos")]
pub fn paste_into_target(target: FocusTarget) -> Result<(), String> {
    use objc2::msg_send;
    use objc2::runtime::AnyClass;
    use std::os::raw::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: *const c_void,
            virtual_key: u16,
            key_down: bool,
        ) -> *mut c_void;
        fn CGEventSetFlags(event: *mut c_void, flags: u64);
        fn CGEventPostToPid(pid: i32, event: *mut c_void);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *mut c_void);
    }

    const KCG_EVENT_FLAG_MASK_COMMAND: u64 = 1 << 20;
    const KVK_ANSI_V: u16 = 9;

    // Re-activate the target app so it's frontmost and ready to receive input.
    unsafe {
        if let Some(cls) = AnyClass::get(c"NSRunningApplication") {
            let app: *mut objc2::runtime::AnyObject =
                msg_send![cls, runningApplicationWithProcessIdentifier: target];
            if !app.is_null() {
                let _: bool = msg_send![app, activateWithOptions: 2u64];
            }
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(100));

    // Send Cmd+V directly to the target PID
    unsafe {
        let key_down = CGEventCreateKeyboardEvent(std::ptr::null(), KVK_ANSI_V, true);
        if key_down.is_null() {
            return Err("Failed to create CGEvent key-down".into());
        }
        CGEventSetFlags(key_down, KCG_EVENT_FLAG_MASK_COMMAND);
        CGEventPostToPid(target, key_down);
        CFRelease(key_down);

        std::thread::sleep(std::time::Duration::from_millis(10));

        let key_up = CGEventCreateKeyboardEvent(std::ptr::null(), KVK_ANSI_V, false);
        if key_up.is_null() {
            return Err("Failed to create CGEvent key-up".into());
        }
        CGEventSetFlags(key_up, KCG_EVENT_FLAG_MASK_COMMAND);
        CGEventPostToPid(target, key_up);
        CFRelease(key_up);
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(())
}

// ── Windows implementation ───────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub fn get_frontmost_target() -> Option<FocusTarget> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        None
    } else {
        Some(hwnd.0 as isize)
    }
}

/// Activates the target window and simulates Ctrl+V via SendInput.
#[cfg(target_os = "windows")]
pub fn paste_into_target(target: FocusTarget) -> Result<(), String> {
    use std::mem;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
    };

    let hwnd = HWND(target as *mut _);

    // Validate the target window is still alive
    if unsafe { IsWindow(hwnd) }.0 == 0 {
        return Err(format!(
            "Target window (HWND {}) is no longer valid — it may have been closed",
            target
        ));
    }

    unsafe {
        let our_thread = GetCurrentThreadId();
        let fg_hwnd = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg_hwnd, None);
        let target_thread = GetWindowThreadProcessId(hwnd, None);

        log::info!(
            "[paste] our_thread={}, fg_thread={}, target_thread={}, target_hwnd={}",
            our_thread, fg_thread, target_thread, target
        );

        // Attach our thread to the foreground window's thread so we gain
        // permission to call SetForegroundWindow reliably.
        let attached_fg = if our_thread != fg_thread && fg_thread != 0 {
            AttachThreadInput(our_thread, fg_thread, true).0 != 0
        } else {
            false
        };
        let attached_target = if our_thread != target_thread && target_thread != fg_thread && target_thread != 0 {
            AttachThreadInput(our_thread, target_thread, true).0 != 0
        } else {
            false
        };

        // AttachThreadInput above gives us permission to call SetForegroundWindow
        // without the old Alt-key hack (which activated menu bars in apps like Notepad).
        let fg_ok = SetForegroundWindow(hwnd);
        if fg_ok.0 == 0 {
            log::warn!("[paste] SetForegroundWindow failed for HWND {}", target);
        } else {
            log::info!("[paste] SetForegroundWindow succeeded for HWND {}", target);
        }

        // Detach threads
        if attached_fg {
            let _ = AttachThreadInput(our_thread, fg_thread, false);
        }
        if attached_target {
            let _ = AttachThreadInput(our_thread, target_thread, false);
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(150));

    // Verify the target is actually foreground before sending Ctrl+V
    let actual_fg = unsafe { GetForegroundWindow() };
    if actual_fg != hwnd {
        log::warn!(
            "[paste] target HWND {} is not foreground (actual={}), Ctrl+V may go to wrong window",
            target,
            actual_fg.0 as isize
        );
    }

    // Send Ctrl+V
    unsafe {
        let mut inputs: [INPUT; 4] = mem::zeroed();

        // Ctrl down
        inputs[0].r#type = INPUT_KEYBOARD;
        inputs[0].Anonymous.ki.wVk = VK_CONTROL;

        // V down
        inputs[1].r#type = INPUT_KEYBOARD;
        inputs[1].Anonymous.ki.wVk = VK_V;

        // V up
        inputs[2].r#type = INPUT_KEYBOARD;
        inputs[2].Anonymous.ki.wVk = VK_V;
        inputs[2].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

        // Ctrl up
        inputs[3].r#type = INPUT_KEYBOARD;
        inputs[3].Anonymous.ki.wVk = VK_CONTROL;
        inputs[3].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

        let sent = SendInput(&inputs, mem::size_of::<INPUT>() as i32);
        if sent != 4 {
            return Err(format!("SendInput sent {} of 4 events", sent));
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(())
}

// ── Linux implementation ────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false)
}

/// Returns the current frontmost window target so paste can refocus it.
/// - On Wayland (KDE): `kdotool getactivewindow` returns the KWin window id; we
///   store it prefixed with `kwin:`. Keystrokes still go through ydotool
///   (uinput), but we reactivate this window first because the focus does NOT
///   reliably return to the original app after our overlay hides on KWin. If
///   kdotool is unavailable we fall back to the `wayland` marker (blind
///   ydotool). We must NOT use `xdotool getactivewindow` here: it succeeds on
///   KWin but returns an XWayland id that Ctrl+V can never reach.
/// - On X11: `xdotool getactivewindow` returns the window id for refocus+paste.
#[cfg(target_os = "linux")]
pub fn get_frontmost_target() -> Option<FocusTarget> {
    if is_wayland() {
        if let Ok(output) = std::process::Command::new("kdotool")
            .arg("getactivewindow")
            .output()
        {
            if output.status.success() {
                let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !id.is_empty() {
                    log::info!("[paste] captured KWin window id: {id}");
                    return Some(format!("kwin:{id}"));
                }
            }
        }
        log::warn!("[paste] kdotool getactivewindow unavailable — paste will be focus-blind");
        return Some("wayland".to_string());
    }

    // X11: capture the active window so paste can refocus it.
    if let Ok(output) = std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output()
    {
        if output.status.success() {
            let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }

    None
}

/// Activates the target window and simulates Ctrl+V.
/// - If we have a real window ID (works on X11 and Wayland+XWayland), uses
///   `xdotool` to refocus the window and send the keystroke.
/// - If we only have the "wayland" marker, uses the Wayland paste fallback
///   chain (ydotool → wtype → xdotool).
#[cfg(target_os = "linux")]
pub fn paste_into_target(target: FocusTarget) -> Result<(), String> {
    if target == "wayland" {
        return paste_wayland();
    }
    if let Some(kwin_id) = target.strip_prefix("kwin:") {
        return paste_kwin(kwin_id);
    }
    paste_x11(&target)
}

/// Inject Ctrl+V via ydotool (kernel uinput — the only synthetic-input method
/// KWin/Mutter accept). `-d 25` spaces the key events so fast compositors don't
/// drop them. Requires `ydotoold` running.
#[cfg(target_os = "linux")]
fn ydotool_ctrl_v() -> Result<(), String> {
    let status = std::process::Command::new("ydotool")
        .args(["key", "-d", "25", "29:1", "47:1", "47:0", "29:0"]) // Ctrl+V
        .status()
        .map_err(|e| format!("Failed to run ydotool (is ydotoold running?): {e}"))?;
    if !status.success() {
        return Err(format!(
            "ydotool key ctrl+v failed with {status} — is ydotoold running? \
             (`systemctl --user enable --now ydotoold`)"
        ));
    }
    Ok(())
}

/// KDE Wayland: reactivate the captured target window (focus does not reliably
/// return after the overlay hides on KWin), then inject Ctrl+V via ydotool.
#[cfg(target_os = "linux")]
fn paste_kwin(window_id: &str) -> Result<(), String> {
    let activate = std::process::Command::new("kdotool")
        .args(["windowactivate", window_id])
        .status();
    match activate {
        Ok(s) if s.success() => log::info!("[paste] kdotool reactivated window {window_id}"),
        Ok(s) => log::warn!("[paste] kdotool windowactivate {window_id} exited {s}"),
        Err(e) => log::warn!("[paste] kdotool windowactivate failed: {e}"),
    }

    // Give KWin a moment to apply focus before injecting the keystroke.
    std::thread::sleep(std::time::Duration::from_millis(150));

    ydotool_ctrl_v()?;
    log::info!("[paste] ydotool Ctrl+V sent (KWin path)");
    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(())
}

#[cfg(target_os = "linux")]
fn paste_x11(window_id: &str) -> Result<(), String> {
    // Validate window_id is a plain integer before passing to xdotool
    if !window_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "Invalid window ID '{}': expected digits only",
            window_id
        ));
    }

    // Re-focus the original window
    let focus_status = std::process::Command::new("xdotool")
        .args(["windowactivate", "--sync", window_id])
        .status()
        .map_err(|e| format!("Failed to run xdotool windowactivate: {e}"))?;

    if !focus_status.success() {
        log::warn!("xdotool windowactivate exited with {focus_status}");
    }

    std::thread::sleep(std::time::Duration::from_millis(100));

    // Simulate Ctrl+V
    let paste_status = std::process::Command::new("xdotool")
        .args(["key", "--clearmodifiers", "ctrl+v"])
        .status()
        .map_err(|e| format!("Failed to run xdotool key: {e}"))?;

    if !paste_status.success() {
        return Err(format!("xdotool key ctrl+v failed with {paste_status}"));
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(())
}

/// Focus-blind Wayland paste, used when kdotool isn't available to capture and
/// reactivate the target window. Relies on the target keeping keyboard focus
/// after the overlay hides (true on wlroots/GNOME, unreliable on KWin — hence
/// the kdotool path above).
#[cfg(target_os = "linux")]
fn paste_wayland() -> Result<(), String> {
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Strategy 1: ydotool (kernel uinput — works on KWin/Mutter/wlroots).
    match ydotool_ctrl_v() {
        Ok(()) => {
            log::info!("[paste] ydotool Ctrl+V sent (focus-blind Wayland path)");
            std::thread::sleep(std::time::Duration::from_millis(50));
            return Ok(());
        }
        Err(e) => log::warn!("[paste] {e}; trying wtype"),
    }

    // Strategy 2: wtype (wlroots-only — Sway/Hyprland; rejected by KWin/Mutter).
    if let Ok(status) = std::process::Command::new("wtype")
        .args(["-M", "ctrl", "-k", "v", "-m", "ctrl"])
        .status()
    {
        if status.success() {
            log::info!("[paste] wtype Ctrl+V sent");
            std::thread::sleep(std::time::Duration::from_millis(50));
            return Ok(());
        }
        log::warn!("[paste] wtype failed with {status}, trying xdotool");
    }

    // Strategy 3: xdotool via XWayland (only reaches XWayland apps).
    let status = std::process::Command::new("xdotool")
        .args(["key", "--clearmodifiers", "ctrl+v"])
        .status()
        .map_err(|e| format!("All paste methods failed. Last error (xdotool): {e}"))?;

    if !status.success() {
        return Err(format!(
            "All paste methods failed (ydotool, wtype, xdotool). \
             xdotool exited with {status}"
        ));
    }

    log::info!("[paste] xdotool Ctrl+V sent (XWayland)");
    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(())
}
