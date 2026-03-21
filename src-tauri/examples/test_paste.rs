//! Quick diagnostic to test the paste mechanism in isolation.
//!
//! Usage (macOS only):
//!   1. Run: cargo run --example test_paste --manifest-path src-tauri/Cargo.toml
//!   2. Switch to a text editor within 3 seconds
//!   3. If nothing pastes, check the Accessibility permission output below
//!
//! On Linux:
//!   1. Run: cargo run --example test_paste --manifest-path src-tauri/Cargo.toml
//!   2. Switch to a text editor within 3 seconds
//!   3. Requires xdotool (X11) or wtype (Wayland) installed

fn main() {
    #[cfg(target_os = "macos")]
    macos_main();

    #[cfg(target_os = "linux")]
    linux_main();

    #[cfg(target_os = "windows")]
    println!("This test example is not implemented for Windows. Use the app directly.");
}

#[cfg(target_os = "macos")]
fn macos_main() {
    use std::os::raw::c_uchar;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> c_uchar;
    }

    println!("=== Paste Diagnostic (macOS) ===");
    println!();

    // Step 0: Check Accessibility permission for THIS process
    let trusted = unsafe { AXIsProcessTrusted() };
    if trusted == 0 {
        println!("[0] *** ACCESSIBILITY NOT GRANTED for this process ***");
        println!("    Go to: System Settings → Privacy & Security → Accessibility");
        println!("    Add your terminal app (Terminal.app, iTerm2, Warp, etc.)");
        println!("    Then re-run this test.");
        println!();
        println!("    (The full Careless Whisper.app has its own entry — this test");
        println!("     runs from Terminal which needs its OWN permission.)");
        println!();
    } else {
        println!("[0] Accessibility: GRANTED");
    }

    // Step 1: Get frontmost target
    let target = careless_whisper_lib::output::paste::get_frontmost_target();
    println!("[1] Frontmost target (should be terminal): {:?}", target);

    // Step 2: Copy test text to clipboard
    let text = "PASTE TEST OK";
    match careless_whisper_lib::output::clipboard::copy_to_clipboard(text) {
        Ok(()) => println!("[2] Clipboard set to: {:?}", text),
        Err(e) => {
            println!("[2] FAILED to set clipboard: {}", e);
            return;
        }
    }

    // Step 3: Give user time to focus the target app
    println!();
    println!(">>> Switch to a text editor NOW. Pasting in 3 seconds...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Step 4: Get the NEW frontmost target (should be the text editor)
    let new_target = careless_whisper_lib::output::paste::get_frontmost_target();
    println!("[3] Target after switch: {:?}", new_target);

    if new_target == target {
        println!("    WARNING: Target didn't change — you may still be in the terminal");
    }

    // Step 5: Paste
    if let Some(t) = new_target {
        println!("[4] Activating target and sending Cmd+V...");
        match careless_whisper_lib::output::paste::paste_into_target(t) {
            Ok(()) => println!("[5] paste_into_target returned Ok"),
            Err(e) => println!("[5] paste_into_target FAILED: {}", e),
        }
    } else {
        println!("[4] No frontmost target found, cannot paste");
    }

    println!();
    if trusted == 0 {
        println!("VERDICT: Paste likely failed because Accessibility is not granted.");
        println!("         Add your terminal to Privacy & Security → Accessibility.");
    } else {
        println!("Did 'PASTE TEST OK' appear in your text editor?");
    }
}

#[cfg(target_os = "linux")]
fn linux_main() {
    println!("=== Paste Diagnostic (Linux) ===");
    println!();

    // Step 1: Get frontmost target
    let target = careless_whisper_lib::output::paste::get_frontmost_target();
    println!("[1] Frontmost target: {:?}", target);

    // Step 2: Copy test text to clipboard
    let text = "PASTE TEST OK";
    match careless_whisper_lib::output::clipboard::copy_to_clipboard(text) {
        Ok(()) => println!("[2] Clipboard set to: {:?}", text),
        Err(e) => {
            println!("[2] FAILED to set clipboard: {}", e);
            return;
        }
    }

    // Step 3: Give user time to focus the target app
    println!();
    println!(">>> Switch to a text editor NOW. Pasting in 3 seconds...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Step 4: Get the NEW frontmost target (should be the text editor)
    let new_target = careless_whisper_lib::output::paste::get_frontmost_target();
    println!("[3] Target after switch: {:?}", new_target);

    // Step 5: Paste
    if let Some(t) = new_target {
        println!("[4] Sending Ctrl+V...");
        match careless_whisper_lib::output::paste::paste_into_target(t) {
            Ok(()) => println!("[5] paste_into_target returned Ok"),
            Err(e) => println!("[5] paste_into_target FAILED: {}", e),
        }
    } else {
        println!("[4] No frontmost target found, cannot paste");
    }

    println!();
    println!("Did 'PASTE TEST OK' appear in your text editor?");
}
