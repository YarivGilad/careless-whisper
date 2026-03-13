/// Platform-specific identifier for the focused application/window.
/// - macOS: process ID (pid_t)
/// - Windows: window handle (HWND as isize)
#[cfg(target_os = "macos")]
pub type FocusTarget = i32;

#[cfg(target_os = "windows")]
pub type FocusTarget = isize;

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
    if hwnd.0 == 0 {
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
