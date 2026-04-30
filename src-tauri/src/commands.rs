use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::config::settings::{OverlayPosition, Settings};
use crate::models::downloader::{self, ModelInfo};
use crate::output::paste::FocusTarget;
use crate::AppState;

fn position_overlay(app: &AppHandle, win: &tauri::WebviewWindow, position: &OverlayPosition) {
    use tauri::PhysicalPosition;

    // Find the monitor the user is actually working on (cursor's monitor).
    // Notes on Tauri 2.x + macOS coordinate quirks:
    //   - `monitor_from_point` is unreliable on macOS — we hit-test manually.
    //   - `monitor.position()` is in primary-scale logical points, but
    //     `monitor.size()` is in physical pixels. Dividing by scale gives
    //     logical size, which lines up with the cursor coordinate space.
    //   - Cursor Y values don't always line up cleanly across displays with
    //     different heights, so we do an X-only hit test — this reliably
    //     picks the right monitor for the vast majority of arrangements
    //     (side-by-side) and falls back gracefully otherwise.
    let cursor_pos = app.cursor_position().ok();
    let monitors = app.available_monitors().unwrap_or_default();
    for (i, m) in monitors.iter().enumerate() {
        log::info!(
            "[overlay] monitor[{}] origin={:?} size={:?} scale={}",
            i,
            m.position(),
            m.size(),
            m.scale_factor()
        );
    }

    // Primary monitor's scale factor is the reference for the whole "logical"
    // coordinate space — monitor positions are in primary-logical points, but
    // cursor_position() returns true physical pixels on the virtual desktop.
    // We need to convert origins to physical to hit-test correctly.
    let primary_scale = monitors
        .iter()
        .find(|m| m.position().x == 0 && m.position().y == 0)
        .map(|m| m.scale_factor())
        .or_else(|| app.primary_monitor().ok().flatten().map(|m| m.scale_factor()))
        .unwrap_or(1.0);

    // X-only hit test with a nearest-monitor fallback. Cursor coords can drift
    // a few dozen pixels outside the reported monitor bounds (bezel, rounding,
    // coordinate-system mismatches), so pick the monitor whose X range is
    // closest to the cursor if no monitor contains it exactly.
    let cursor_monitor = cursor_pos.as_ref().and_then(|pos| {
        let x_distance = |m: &tauri::Monitor| -> f64 {
            let left = m.position().x as f64 * primary_scale;
            let right = left + m.size().width as f64;
            if pos.x < left {
                left - pos.x
            } else if pos.x >= right {
                pos.x - right + 1.0
            } else {
                0.0
            }
        };
        monitors
            .iter()
            .min_by(|a, b| {
                x_distance(a)
                    .partial_cmp(&x_distance(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    });

    log::info!(
        "[overlay] cursor={:?}, primary_scale={}, hit_monitor_origin={:?}",
        cursor_pos,
        primary_scale,
        cursor_monitor.as_ref().map(|m| m.position())
    );

    let monitor = cursor_monitor
        .or_else(|| win.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());

    let monitor = match monitor {
        Some(m) => m,
        None => {
            log::warn!("[overlay] no monitor found");
            return;
        }
    };

    // Work in macOS NSScreen points (same units as monitor.position()), then
    // multiply by primary_scale at the end — Tauri's PhysicalPosition is
    // effectively `NSScreen-points × primary_scale` on macOS, so that's what
    // we feed it.
    let target_scale = monitor.scale_factor();
    let origin_x_points = monitor.position().x as f64;
    let origin_y_points = monitor.position().y as f64;
    // NSScreen width is reported_physical_size / target_scale (size field is
    // in physical pixels, but NSScreen frames live in points).
    let screen_w_points = monitor.size().width as f64 / target_scale;
    let screen_h_points = monitor.size().height as f64 / target_scale;

    let overlay_w = 320.0;
    let overlay_h = 80.0;
    let margin = 16.0;
    let top_offset = 40.0;

    let offset_x = match position {
        OverlayPosition::TopLeft => margin,
        OverlayPosition::TopRight => screen_w_points - overlay_w - margin,
        OverlayPosition::TopCenter | OverlayPosition::BottomCenter => {
            (screen_w_points - overlay_w) / 2.0
        }
    };
    let offset_y = match position {
        OverlayPosition::BottomCenter => screen_h_points - overlay_h - margin,
        _ => top_offset,
    };

    let x_points = origin_x_points + offset_x;
    let y_points = origin_y_points + offset_y;
    let x_phys = x_points * primary_scale;
    let y_phys = y_points * primary_scale;

    log::info!(
        "[overlay] target_origin_pts=({}, {}), {}x{} pts @ {}x, overlay_pts=({}, {}), phys=({}, {}), position={:?}",
        origin_x_points,
        origin_y_points,
        screen_w_points,
        screen_h_points,
        target_scale,
        x_points,
        y_points,
        x_phys,
        y_phys,
        position
    );
    let _ = win.set_position(PhysicalPosition::new(x_phys, y_phys));
}

/// On macOS, elevate the overlay window above the dock (level 20).
/// NSStatusWindowLevel (25) ensures it floats above the dock and menu bar.
#[cfg(target_os = "macos")]
fn set_overlay_above_dock(win: &tauri::WebviewWindow) {
    use objc2::msg_send;
    unsafe {
        if let Ok(ns_win) = win.ns_window() {
            let ns_win = ns_win as *mut objc2::runtime::AnyObject;
            // kCGStatusWindowLevel = 25, above kCGDockWindowLevel (20)
            let _: () = msg_send![ns_win, setLevel: 25_i64];
        }
    }
}

fn hide_overlay(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.hide();
    }
}

fn emit_transcription_error(app: &AppHandle, message: impl Into<String>) {
    let message = message.into();
    let _ = app.emit(
        "transcription-error",
        serde_json::json!({ "message": message }),
    );
}

fn transcription_inputs(
    state: &State<'_, AppState>,
) -> (String, bool, bool, Option<FocusTarget>, String, PathBuf) {
    let settings = state.settings.lock().unwrap().clone();
    let model_path = downloader::model_path(&settings.active_model);
    (
        settings.language,
        settings.auto_paste,
        settings.translate_to_english,
        state.target_focus.lock().unwrap().clone(),
        settings.active_model,
        model_path,
    )
}

fn spawn_transcription(
    app: AppHandle,
    samples_16k: Vec<f32>,
    language: String,
    auto_paste: bool,
    translate_to_english: bool,
    target_focus: Option<FocusTarget>,
    active_model: String,
    model_path: PathBuf,
    hide_overlay_on_finish: bool,
) {
    log::info!(
        "[transcribe] starting: model='{}', language='{}', translate={}, samples={}, auto_paste={}, target={:?}",
        active_model, language, translate_to_english, samples_16k.len(), auto_paste, target_focus
    );

    tokio::task::spawn_blocking(move || {
        let state = app.state::<AppState>();

        if let Err(error) = downloader::validate_model_file(&active_model) {
            log::error!("[transcribe] model validation failed: {}", error);
            emit_transcription_error(&app, error);
            if hide_overlay_on_finish {
                hide_overlay(&app);
            }
            return;
        }

        let ctx = state.whisper_ctx.lock().unwrap().take();
        let ctx = match ctx {
            Some(context) => context,
            None => match crate::transcribe::whisper::load_model(&model_path) {
                Ok(context) => context,
                Err(error) => {
                    emit_transcription_error(&app, error);
                    if hide_overlay_on_finish {
                        hide_overlay(&app);
                    }
                    return;
                }
            },
        };

        let result = crate::transcribe::whisper::transcribe(&ctx, &samples_16k, &language, translate_to_english);
        *state.whisper_ctx.lock().unwrap() = Some(ctx);

        match result {
            Ok(ref text) => {
                log::info!("[transcribe] result ({} chars): {:?}", text.len(), &text[..text.len().min(100)]);

                // Save the user's clipboard before overwriting it
                let previous_clipboard = crate::output::clipboard::read_clipboard();

                let clipboard_ok = match crate::output::clipboard::copy_to_clipboard(text) {
                    Ok(()) => true,
                    Err(e) => {
                        log::error!("[clipboard] failed: {}", e);
                        emit_transcription_error(&app, format!("Clipboard error: {}", e));
                        false
                    }
                };

                if hide_overlay_on_finish {
                    hide_overlay(&app);
                }

                let _ = app.emit(
                    "transcription-complete",
                    serde_json::json!({ "text": text }),
                );

                if clipboard_ok && auto_paste {
                    if let Some(target) = target_focus {
                        match crate::output::paste::paste_into_target(target, text) {
                            Ok(()) => {
                                // Paste succeeded — restore the user's original clipboard
                                if let Some(prev) = previous_clipboard {
                                    std::thread::sleep(std::time::Duration::from_millis(200));
                                    let _ = crate::output::clipboard::copy_to_clipboard(&prev);
                                }
                            }
                            Err(error) => {
                                // Paste failed — keep transcription on clipboard so user can Cmd+V manually
                                log::error!("[paste] failed: {}", error);
                            }
                        }
                    } else {
                        log::warn!("[paste] no target window captured — skipping paste");
                    }
                }
            }
            Err(ref error) => {
                log::error!("[transcribe] failed: {}", error);
                emit_transcription_error(&app, error.clone());
                if hide_overlay_on_finish {
                    hide_overlay(&app);
                }
            }
        }
    });
}

#[tauri::command]
pub async fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let settings = state.settings.lock().unwrap().clone();

    if settings.lower_volume_while_recording {
        match crate::audio::volume::get_system_volume() {
            Ok(vol) => {
                *state.original_volume.lock().unwrap() = Some(vol);
                if let Err(e) = crate::audio::volume::set_system_volume(0.10) {
                    log::warn!("[volume] failed to lower: {}", e);
                }
            }
            Err(e) => log::warn!("[volume] failed to read: {}", e),
        }
    }

    let handle = crate::audio::capture::start_capture(settings.max_recording_seconds)?;
    let current_level = handle.current_level.clone();
    *state.recording.lock().unwrap() = Some(handle);

    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.show();
        let win_clone = win.clone();
        let app_clone = app.clone();
        let overlay_pos = settings.overlay_position.clone();
        let _ = app.run_on_main_thread(move || {
            position_overlay(&app_clone, &win_clone, &overlay_pos);
            #[cfg(target_os = "macos")]
            set_overlay_above_dock(&win_clone);
        });
    }

    // Spawn a task that emits audio level events at ~20fps for waveform visualization
    let level_active = Arc::new(AtomicBool::new(true));
    *state.level_emitter_active.lock().unwrap() = Some(level_active.clone());
    let app_for_level = app.clone();
    tokio::spawn(async move {
        while level_active.load(Ordering::Relaxed) {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let bits = current_level.load(Ordering::Relaxed);
            let rms = f32::from_bits(bits);
            // Normalize: typical speech RMS is 0.01–0.15 for float samples
            let normalized = (rms * 8.0).min(1.0);
            let _ = app_for_level.emit("audio-level", serde_json::json!({ "level": normalized }));
        }
    });

    app.emit("recording-started", ())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    // Stop the audio level emitter
    if let Some(active) = state.level_emitter_active.lock().unwrap().take() {
        active.store(false, Ordering::Relaxed);
    }

    let handle = state
        .recording
        .lock()
        .unwrap()
        .take()
        .ok_or("Not recording")?;

    let (raw_samples, sample_rate, channels) = crate::audio::capture::stop_capture(handle);

    if let Some(vol) = state.original_volume.lock().unwrap().take() {
        if let Err(e) = crate::audio::volume::set_system_volume(vol) {
            log::warn!("[volume] failed to restore: {}", e);
        }
    }

    app.emit("recording-stopped", ())
        .map_err(|e| e.to_string())?;

    let samples_16k =
        crate::audio::resample::resample_to_16k(raw_samples, sample_rate, channels as usize)?;
    let (language, auto_paste, translate_to_english, target_focus, active_model, model_path) =
        transcription_inputs(&state);

    spawn_transcription(
        app,
        samples_16k,
        language,
        auto_paste,
        translate_to_english,
        target_focus,
        active_model,
        model_path,
        true,
    );

    Ok(())
}

#[tauri::command]
pub async fn transcribe_audio_file(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(format!("Audio file not found: {}", path.display()));
    }
    if !path.is_file() {
        return Err(format!("Selected path is not a file: {}", path.display()));
    }

    let (samples, sample_rate, channels) = crate::audio::decode::decode_audio_file(&path)?;
    let samples_16k =
        crate::audio::resample::resample_to_16k(samples, sample_rate, channels as usize)?;
    let (language, _auto_paste, translate_to_english, _target_focus, active_model, model_path) =
        transcription_inputs(&state);

    spawn_transcription(
        app,
        samples_16k,
        language,
        false,
        translate_to_english,
        None,
        active_model,
        model_path,
        false,
    );

    Ok(())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().unwrap().clone())
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    settings: Settings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let old_hotkey = state.settings.lock().unwrap().hotkey.clone();
    let new_hotkey = settings.hotkey.clone();

    settings.save()?;
    *state.settings.lock().unwrap() = settings;

    if old_hotkey != new_hotkey {
        crate::hotkey::manager::re_register_hotkey(&app, &old_hotkey, &new_hotkey)?;
    }

    Ok(())
}

#[tauri::command]
pub async fn list_models() -> Result<Vec<ModelInfo>, String> {
    Ok(downloader::list_models())
}

const VALID_MODELS: &[&str] = &["tiny", "base", "small", "medium", "large-v3"];

pub(crate) fn validate_model_name(model: &str) -> Result<(), String> {
    if VALID_MODELS.contains(&model) {
        Ok(())
    } else {
        Err(format!(
            "Unknown model '{}'. Valid models: {}",
            model,
            VALID_MODELS.join(", ")
        ))
    }
}

#[tauri::command]
pub async fn download_model(app: AppHandle, model: String) -> Result<(), String> {
    validate_model_name(&model)?;
    downloader::download_model(app, model).await
}

#[tauri::command]
pub async fn delete_model(model: String) -> Result<(), String> {
    validate_model_name(&model)?;
    downloader::delete_model(&model)
}

#[tauri::command]
pub async fn set_active_model(model: String, state: State<'_, AppState>) -> Result<(), String> {
    validate_model_name(&model)?;
    let model_path = downloader::model_path(&model);
    if !model_path.exists() {
        return Err(format!("Model '{}' is not downloaded", model));
    }

    *state.whisper_ctx.lock().unwrap() = None;

    {
        let mut settings = state.settings.lock().unwrap();
        settings.active_model = model;
        settings.save()?;
    }

    Ok(())
}

#[tauri::command]
pub async fn check_accessibility() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn AXIsProcessTrusted() -> u8;
        }

        Ok(unsafe { AXIsProcessTrusted() != 0 })
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}

#[tauri::command]
pub async fn request_accessibility() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use std::os::raw::c_void;

        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
        }

        #[link(name = "CoreFoundation", kind = "framework")]
        extern "C" {
            fn CFDictionaryCreate(
                allocator: *const c_void,
                keys: *const *const c_void,
                values: *const *const c_void,
                num_values: isize,
                key_callbacks: *const c_void,
                value_callbacks: *const c_void,
            ) -> *const c_void;
            fn CFRelease(cf: *mut c_void);
            static kCFBooleanTrue: *const c_void;
            static kCFTypeDictionaryKeyCallBacks: c_void;
            static kCFTypeDictionaryValueCallBacks: c_void;
        }

        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            static kAXTrustedCheckOptionPrompt: *const c_void;
        }

        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let options = CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks as *const _ as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const _ as *const c_void,
            );
            let trusted = AXIsProcessTrustedWithOptions(options);
            CFRelease(options as *mut c_void);
            Ok(trusted != 0)
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}

#[tauri::command]
pub async fn check_microphone() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let status = crate::check_microphone_permission();
        let label = match status {
            0 => "not_determined",
            1 => "denied",
            2 => "restricted",
            3 => "authorized",
            _ => "unknown",
        };
        Ok(label.to_string())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok("authorized".to_string())
    }
}

#[tauri::command]
pub async fn request_microphone() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        crate::request_microphone_permission();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let status = crate::check_microphone_permission();
        let label = match status {
            0 => "not_determined",
            1 => "denied",
            2 => "restricted",
            3 => "authorized",
            _ => "unknown",
        };
        Ok(label.to_string())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok("authorized".to_string())
    }
}

#[tauri::command]
pub async fn get_launch_at_login(app: AppHandle) -> Result<bool, String> {
    let manager = app.autolaunch();
    manager.is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_launch_at_login(
    app: AppHandle,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())?;
    } else {
        manager.disable().map_err(|e| e.to_string())?;
    }

    let mut settings = state.settings.lock().unwrap();
    settings.launch_at_login = enabled;
    settings.save()?;

    Ok(())
}

#[tauri::command]
pub async fn get_recent_logs() -> Result<String, String> {
    let path = crate::log_path();
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(100);
    Ok(lines[start..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_model_all_valid() {
        for name in &["tiny", "base", "small", "medium", "large-v3"] {
            assert!(
                validate_model_name(name).is_ok(),
                "{} should be valid",
                name
            );
        }
    }

    #[test]
    fn test_validate_model_path_traversal() {
        assert!(validate_model_name("../../etc/passwd").is_err());
    }

    #[test]
    fn test_validate_model_injection() {
        assert!(validate_model_name("tiny evil").is_err());
        assert!(validate_model_name("tiny/evil").is_err());
        assert!(validate_model_name("tiny\0evil").is_err());
        assert!(validate_model_name("").is_err());
    }
}
