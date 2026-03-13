use objc2::msg_send;
use objc2::runtime::AnyClass;
use std::os::raw::c_void;

// Direct CoreGraphics FFI — bypasses enigo entirely.
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreateKeyboardEvent(
        source: *const c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> *mut c_void;
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    // fn CGEventPost(tap: u32, event: *mut c_void);
    fn CGEventPostToPid(pid: i32, event: *mut c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *mut c_void);
}

// const KCG_HID_EVENT_TAP: u32 = 0;
const KCG_EVENT_FLAG_MASK_COMMAND: u64 = 1 << 20;
const KVK_ANSI_V: u16 = 9; // macOS virtual keycode for 'V'

/// Returns the PID of the current frontmost application via NSWorkspace.
pub fn get_frontmost_pid() -> Option<i32> {
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

/// Activates the app with the given PID then simulates Cmd+V via CoreGraphics CGEventPostToPid.
pub fn paste_into_pid(pid: i32) -> Result<(), String> {
    // Re-activate the target app so it's frontmost and ready to receive input.
    unsafe {
        if let Some(cls) = AnyClass::get(c"NSRunningApplication") {
            let app: *mut objc2::runtime::AnyObject =
                msg_send![cls, runningApplicationWithProcessIdentifier: pid];
            if !app.is_null() {
                let _: bool = msg_send![app, activateWithOptions: 2u64];
            }
        }
    }

    // Wait for activation to take effect
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Send Cmd+V directly to the target PID
    unsafe {
        // Key down: V with Command modifier
        let key_down = CGEventCreateKeyboardEvent(std::ptr::null(), KVK_ANSI_V, true);
        if key_down.is_null() {
            return Err("Failed to create CGEvent key-down".into());
        }
        CGEventSetFlags(key_down, KCG_EVENT_FLAG_MASK_COMMAND);
        CGEventPostToPid(pid, key_down);
        CFRelease(key_down);

        // Small delay between key-down and key-up
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Key up: V with Command modifier
        let key_up = CGEventCreateKeyboardEvent(std::ptr::null(), KVK_ANSI_V, false);
        if key_up.is_null() {
            return Err("Failed to create CGEvent key-up".into());
        }
        CGEventSetFlags(key_up, KCG_EVENT_FLAG_MASK_COMMAND);
        CGEventPostToPid(pid, key_up);
        CFRelease(key_up);
    }

    // Wait for the events to be delivered before returning
    std::thread::sleep(std::time::Duration::from_millis(50));

    Ok(())
}
