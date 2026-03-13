//! Quick diagnostic to test the paste mechanism in isolation.
//!
//! Usage:
//!   1. Run: cargo run --example test_paste --manifest-path src-tauri/Cargo.toml
//!   2. Switch to a text editor within 3 seconds
//!   3. If nothing pastes, check the Accessibility permission output below

use std::os::raw::c_uchar;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> c_uchar;
}

fn main() {
    println!("=== Paste Diagnostic ===");
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
        println!("[0] Accessibility: GRANTED ✓");
    }

    // Step 1: Get frontmost PID
    let pid = careless_whisper_lib::output::paste::get_frontmost_pid();
    println!("[1] Frontmost PID (should be terminal): {:?}", pid);

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

    // Step 4: Get the NEW frontmost PID (should be the text editor)
    let target_pid = careless_whisper_lib::output::paste::get_frontmost_pid();
    println!("[3] Target PID after switch: {:?}", target_pid);

    if target_pid == pid {
        println!("    WARNING: PID didn't change — you may still be in the terminal");
    }

    // Step 5: Paste
    if let Some(pid) = target_pid {
        println!("[4] Activating PID {} and sending Cmd+V via CGEventPost...", pid);
        match careless_whisper_lib::output::paste::paste_into_pid(pid) {
            Ok(()) => println!("[5] paste_into_pid returned Ok"),
            Err(e) => println!("[5] paste_into_pid FAILED: {}", e),
        }
    } else {
        println!("[4] No frontmost PID found, cannot paste");
    }

    println!();
    if trusted == 0 {
        println!("VERDICT: Paste likely failed because Accessibility is not granted.");
        println!("         Add your terminal to Privacy & Security → Accessibility.");
    } else {
        println!("Did 'PASTE TEST OK' appear in your text editor?");
    }
}
