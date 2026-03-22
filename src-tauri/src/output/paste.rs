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
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    // Bring the target window back to the foreground.
    // SendInput an Alt press first to satisfy SetForegroundWindow restrictions.
    unsafe {
        let mut alt_inputs: [INPUT; 2] = mem::zeroed();
        alt_inputs[0].r#type = INPUT_KEYBOARD;
        alt_inputs[0].Anonymous.ki.wVk = VK_MENU;
        alt_inputs[1].r#type = INPUT_KEYBOARD;
        alt_inputs[1].Anonymous.ki.wVk = VK_MENU;
        alt_inputs[1].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
        SendInput(&alt_inputs, mem::size_of::<INPUT>() as i32);

        let hwnd = HWND(target as *mut _);
        let _ = SetForegroundWindow(hwnd);
    }

    std::thread::sleep(std::time::Duration::from_millis(100));

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

/// Returns the current frontmost window target.
/// - On X11: runs `xdotool getactivewindow` and returns the window ID.
/// - On Wayland: tries `xdotool getactivewindow` via XWayland first (works on
///   GNOME/Ubuntu), falls back to a "wayland" marker if that fails.
#[cfg(target_os = "linux")]
pub fn get_frontmost_target() -> Option<FocusTarget> {
    // Try xdotool first — it works on pure X11 and on Wayland+XWayland (GNOME).
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

    if is_wayland() {
        // Wayland without XWayland access — return a marker.
        // paste_into_target will use the Wayland paste path.
        return Some("wayland".to_string());
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
    paste_x11(&target)
}

#[cfg(target_os = "linux")]
fn paste_x11(window_id: &str) -> Result<(), String> {
    // Validate window_id is a plain integer before passing to xdotool
    if !window_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("Invalid window ID '{}': expected digits only", window_id));
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

#[cfg(target_os = "linux")]
fn paste_wayland() -> Result<(), String> {
    // On Wayland the previously focused window regains focus automatically
    // after our overlay hides, so we just need to simulate the keystroke.
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Strategy 1: Try ydotool (uses uinput, works on all Wayland compositors)
    if let Ok(status) = std::process::Command::new("ydotool")
        .args(["key", "29:1", "47:1", "47:0", "29:0"]) // Ctrl down, V down, V up, Ctrl up
        .status()
    {
        if status.success() {
            std::thread::sleep(std::time::Duration::from_millis(50));
            return Ok(());
        }
        log::warn!("ydotool failed with {status}, trying next method");
    }

    // Strategy 2: Try wtype (works on wlroots-based compositors like Sway)
    if let Ok(status) = std::process::Command::new("wtype")
        .args(["-M", "ctrl", "-k", "v", "-m", "ctrl"])
        .status()
    {
        if status.success() {
            std::thread::sleep(std::time::Duration::from_millis(50));
            return Ok(());
        }
        log::warn!("wtype failed with {status}, trying next method");
    }

    // Strategy 3: Fall back to xdotool via XWayland (works on GNOME/Ubuntu)
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

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(())
}
