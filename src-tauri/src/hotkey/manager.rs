use tauri::{App, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::AppState;

pub fn register_hotkey(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let hotkey = {
        let state = app.state::<AppState>();
        let guard = state.settings.lock().unwrap();
        guard.hotkey.clone()
    };

    let handle = app.handle().clone();

    app.global_shortcut().on_shortcut(hotkey.as_str(), move |_app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            let state = handle.state::<AppState>();
            let is_recording = state.recording.lock().unwrap().is_some();

            if is_recording {
                let _ = handle.emit("hotkey-stop", ());
            } else {
                // Capture the frontmost app's PID now — before any overlay appears
                let pid = crate::output::paste::get_frontmost_pid();
                eprintln!("[hotkey] captured target_pid = {:?}", pid);
                *state.target_pid.lock().unwrap() = pid;
                let _ = handle.emit("hotkey-start", ());
            }
        }
    })?;

    Ok(())
}

pub fn re_register_hotkey(
    app: &tauri::AppHandle,
    old_hotkey: &str,
    new_hotkey: &str,
) -> Result<(), String> {
    let shortcuts = app.global_shortcut();

    if shortcuts.is_registered(old_hotkey) {
        shortcuts.unregister(old_hotkey).map_err(|e| e.to_string())?;
    }

    let handle = app.clone();
    shortcuts
        .on_shortcut(new_hotkey, move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let state = handle.state::<AppState>();
                let is_recording = state.recording.lock().unwrap().is_some();

                if is_recording {
                    let _ = handle.emit("hotkey-stop", ());
                } else {
                    // Capture the frontmost app's PID now — before any overlay appears
                    let pid = crate::output::paste::get_frontmost_pid();
                    *state.target_pid.lock().unwrap() = pid;
                    let _ = handle.emit("hotkey-start", ());
                }
            }
        })
        .map_err(|e| e.to_string())
}
