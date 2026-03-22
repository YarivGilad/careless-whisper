use tauri::{App, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::config::settings::RecordingMode;
use crate::AppState;

pub fn register_hotkey(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let hotkey = {
        let state = app.state::<AppState>();
        let guard = state.settings.lock().unwrap();
        guard.hotkey.clone()
    };

    let handle = app.handle().clone();

    app.global_shortcut().on_shortcut(hotkey.as_str(), move |_app, _shortcut, event| {
        let state = handle.state::<AppState>();
        let is_recording = state.recording.lock().unwrap().is_some();
        let mode = state.settings.lock().unwrap().recording_mode.clone();

        match event.state {
            ShortcutState::Pressed => {
                if matches!(mode, RecordingMode::PushToTalk) {
                    if !is_recording {
                        let target = crate::output::paste::get_frontmost_target();
                        log::info!("Hotkey captured target_focus = {:?}", target);
                        *state.target_focus.lock().unwrap() = target;
                        let _ = handle.emit("hotkey-start", ());
                    }
                } else {
                    // Toggle mode: press to start, press again to stop
                    if is_recording {
                        let _ = handle.emit("hotkey-stop", ());
                    } else {
                        let target = crate::output::paste::get_frontmost_target();
                        log::info!("Hotkey captured target_focus = {:?}", target);
                        *state.target_focus.lock().unwrap() = target;
                        let _ = handle.emit("hotkey-start", ());
                    }
                }
            }
            ShortcutState::Released => {
                if matches!(mode, RecordingMode::PushToTalk) && is_recording {
                    log::info!("Push-to-talk key released, stopping recording");
                    let _ = handle.emit("hotkey-stop", ());
                }
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
            let state = handle.state::<AppState>();
            let is_recording = state.recording.lock().unwrap().is_some();
            let mode = state.settings.lock().unwrap().recording_mode.clone();

            match event.state {
                ShortcutState::Pressed => {
                    if matches!(mode, RecordingMode::PushToTalk) {
                        if !is_recording {
                            let target = crate::output::paste::get_frontmost_target();
                            *state.target_focus.lock().unwrap() = target;
                            let _ = handle.emit("hotkey-start", ());
                        }
                    } else {
                        if is_recording {
                            let _ = handle.emit("hotkey-stop", ());
                        } else {
                            let target = crate::output::paste::get_frontmost_target();
                            *state.target_focus.lock().unwrap() = target;
                            let _ = handle.emit("hotkey-start", ());
                        }
                    }
                }
                ShortcutState::Released => {
                    if matches!(mode, RecordingMode::PushToTalk) && is_recording {
                        log::info!("Push-to-talk key released, stopping recording");
                        let _ = handle.emit("hotkey-stop", ());
                    }
                }
            }
        })
        .map_err(|e| e.to_string())
}
