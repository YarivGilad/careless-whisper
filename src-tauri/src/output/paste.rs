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
pub fn paste_into_target(target: FocusTarget, _text: &str) -> Result<(), String> {
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

/// Activates the target window and injects the transcribed text.
///
/// Uses `KEYEVENTF_UNICODE` to inject each character as a unicode code point
/// rather than synthesizing Ctrl+V. Ctrl+V via SendInput fails when the
/// foreground thread's keyboard layout is non-Latin (e.g. Hebrew) because
/// some apps don't accept Ctrl+V at all under those layouts — even a
/// human pressing Ctrl+V on the physical keyboard does nothing in that
/// state. Unicode injection bypasses all keyboard-shortcut handling and
/// just delivers the characters as if they had been typed (or pasted by
/// an IME), so it works regardless of the active layout.
#[cfg(target_os = "windows")]
pub fn paste_into_target(target: FocusTarget, text: &str) -> Result<(), String> {
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

    // Verify the target is actually foreground before injecting input
    let actual_fg = unsafe { GetForegroundWindow() };
    if actual_fg != hwnd {
        log::warn!(
            "[paste] target HWND {} is not foreground (actual={}), input may go to wrong window",
            target,
            actual_fg.0 as isize
        );
    }

    // Inject each character as a unicode code point. Each char becomes a
    // (key-down, key-up) pair; surrogate pairs are sent as two events for
    // the high and low surrogates (UTF-16 gives us this naturally).
    // SendInput accepts the whole batch in one syscall regardless of length.
    let utf16: Vec<u16> = text.encode_utf16().collect();
    if utf16.is_empty() {
        return Ok(());
    }

    let mut inputs: Vec<INPUT> = Vec::with_capacity(utf16.len() * 2);
    for &unit in &utf16 {
        let mut down: INPUT = unsafe { mem::zeroed() };
        down.r#type = INPUT_KEYBOARD;
        down.Anonymous.ki.wScan = unit;
        down.Anonymous.ki.dwFlags = KEYEVENTF_UNICODE;

        let mut up: INPUT = unsafe { mem::zeroed() };
        up.r#type = INPUT_KEYBOARD;
        up.Anonymous.ki.wScan = unit;
        up.Anonymous.ki.dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;

        inputs.push(down);
        inputs.push(up);
    }

    let expected = inputs.len() as u32;
    let sent = unsafe { SendInput(&inputs, mem::size_of::<INPUT>() as i32) };
    if sent != expected {
        return Err(format!(
            "SendInput sent {} of {} events ({} UTF-16 code units)",
            sent, expected, utf16.len()
        ));
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
pub fn paste_into_target(target: FocusTarget, _text: &str) -> Result<(), String> {
    if target == "wayland" {
        return paste_wayland();
    }
    paste_x11(&target)
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
