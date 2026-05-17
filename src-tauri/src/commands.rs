use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::config::settings::{OverlayPosition, Settings};
use crate::models::downloader::{self, ModelInfo};
use crate::output::paste::FocusTarget;
use crate::AppState;

const OVERLAY_WIDTH: f64 = 360.0;
const OVERLAY_BASE_HEIGHT: f64 = 120.0;

fn preview_text(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let mut preview = String::new();

    for _ in 0..max_chars {
        match chars.next() {
            Some(ch) => preview.push(ch),
            None => return preview,
        }
    }

    if chars.next().is_some() {
        preview.push_str("...");
    }

    preview
}

fn position_overlay(app: &AppHandle, win: &tauri::WebviewWindow, position: &OverlayPosition) {
    use tauri::PhysicalPosition;

    // Find the monitor the user is actually working on (cursor's monitor).
    // Tauri/macOS mixed-DPI coordinates are easiest to keep stable if we use
    // the monitor-reported origin/size directly and avoid multiplying external
    // monitor origins by the primary display scale.
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

    // X-only hit test with a nearest-monitor fallback. Cursor coords can drift
    // a few dozen pixels outside the reported monitor bounds (bezel, rounding,
    // coordinate-system mismatches), so pick the monitor whose X range is
    // closest to the cursor if no monitor contains it exactly.
    let cursor_monitor = cursor_pos.as_ref().and_then(|pos| {
        let x_distance = |m: &tauri::Monitor| -> f64 {
            let left = m.position().x as f64;
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
        "[overlay] cursor={:?}, hit_monitor_origin={:?}",
        cursor_pos,
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

    let target_scale = monitor.scale_factor();
    let origin_x = monitor.position().x as f64;
    let origin_y = monitor.position().y as f64;
    let screen_w = monitor.size().width as f64;
    let screen_h = monitor.size().height as f64;

    let overlay_w = OVERLAY_WIDTH * target_scale;
    let overlay_h = OVERLAY_BASE_HEIGHT * target_scale;
    let margin = 16.0 * target_scale;
    let top_offset = 40.0 * target_scale;

    let offset_x = match position {
        OverlayPosition::TopLeft => margin,
        OverlayPosition::TopRight => screen_w - overlay_w - margin,
        OverlayPosition::TopCenter | OverlayPosition::BottomCenter => (screen_w - overlay_w) / 2.0,
    };
    let offset_y = match position {
        OverlayPosition::BottomCenter => screen_h - overlay_h - margin,
        _ => top_offset,
    };

    let x_phys = origin_x + offset_x;
    let y_phys = origin_y + offset_y;

    log::info!(
        "[overlay] target_origin=({}, {}), {}x{} px @ {}x, overlay_px=({}, {}), position={:?}",
        origin_x,
        origin_y,
        screen_w,
        screen_h,
        target_scale,
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
            // kCGStatusWindowLevel = 25, above kCGDockWindowLevel (20).
            let _: () = msg_send![ns_win, setLevel: 25_i64];
            // Join all Spaces and appear as a fullscreen auxiliary window so
            // the overlay can stay visible when the target app is fullscreen.
            let collection_behavior: u64 = (1 << 0) | (1 << 4) | (1 << 8);
            let _: () = msg_send![ns_win, setCollectionBehavior: collection_behavior];
            let _: () = msg_send![ns_win, setCanHide: false];
            let _: () = msg_send![ns_win, orderFrontRegardless];
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

fn emit_backend_error(app: &AppHandle, message: impl Into<String>) {
    let message = message.into();
    let _ = app.emit("backend-error", serde_json::json!({ "message": message }));
}

fn emit_realtime_transcription(app: &AppHandle, text: &str, full_text: &str) {
    let _ = app.emit(
        "realtime-transcription",
        serde_json::json!({ "text": text, "full_text": full_text }),
    );
}

fn emit_realtime_mode_updated(app: &AppHandle, armed: bool, active: bool) {
    let _ = app.emit(
        "realtime-mode-updated",
        serde_json::json!({ "armed": armed, "active": active }),
    );
}

fn sample_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|sample| sample * sample).sum();
    (sum / samples.len() as f32).sqrt()
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

        let _transcription_guard = state.transcription_lock.lock().unwrap();
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

        let result = crate::transcribe::whisper::transcribe(
            &ctx,
            &samples_16k,
            &language,
            translate_to_english,
        );
        *state.whisper_ctx.lock().unwrap() = Some(ctx);

        match result {
            Ok(ref text) => {
                log::info!(
                    "[transcribe] result ({} chars): {:?}",
                    text.chars().count(),
                    preview_text(text, 100)
                );

                if hide_overlay_on_finish {
                    hide_overlay(&app);
                }

                let _ = app.emit(
                    "transcription-complete",
                    serde_json::json!({ "text": text }),
                );

                if auto_paste {
                    match target_focus {
                        Some(target) => {
                            if let Err(error) =
                                crate::output::paste::type_text_into_target(target, text)
                            {
                                log::error!("[type] failed: {}", error);
                                if let Err(clipboard_error) =
                                    crate::output::clipboard::copy_to_clipboard(text)
                                {
                                    log::error!("[clipboard] failed: {}", clipboard_error);
                                    emit_transcription_error(
                                        &app,
                                        format!("Clipboard error: {}", clipboard_error),
                                    );
                                }
                            }
                        }
                        None => {
                            log::warn!("[type] no target window captured — copying transcript");
                            if let Err(error) = crate::output::clipboard::copy_to_clipboard(text) {
                                log::error!("[clipboard] failed: {}", error);
                                emit_transcription_error(
                                    &app,
                                    format!("Clipboard error: {}", error),
                                );
                            }
                        }
                    }
                } else if let Err(error) = crate::output::clipboard::copy_to_clipboard(text) {
                    log::error!("[clipboard] failed: {}", error);
                    emit_transcription_error(&app, format!("Clipboard error: {}", error));
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

fn transcribe_realtime_chunk(
    app: &AppHandle,
    samples_16k: &[f32],
    language: &str,
    translate_to_english: bool,
    active_model: &str,
    model_path: &PathBuf,
) -> Result<String, String> {
    let state = app.state::<AppState>();
    let _transcription_guard = state.transcription_lock.lock().unwrap();

    downloader::validate_model_file(active_model)?;

    let ctx = state.whisper_ctx.lock().unwrap().take();
    let ctx = match ctx {
        Some(context) => context,
        None => crate::transcribe::whisper::load_model(model_path)?,
    };

    let result =
        crate::transcribe::whisper::transcribe(&ctx, samples_16k, language, translate_to_english);
    *state.whisper_ctx.lock().unwrap() = Some(ctx);
    result
}

fn spawn_realtime_transcription_worker(
    app: AppHandle,
    active: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
    settings: Settings,
    target_focus: Option<FocusTarget>,
    start_sample_index: usize,
    output_seen: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        const CHUNK_SECONDS: f32 = 4.0;
        const POLL_MS: u64 = 500;
        const MIN_RMS: f32 = 0.003;

        let channels_usize = channels as usize;
        let raw_chunk_len = (sample_rate as f32 * channels as f32 * CHUNK_SECONDS).round() as usize;
        let model_path = downloader::model_path(&settings.active_model);
        let mut next_sample_index = start_sample_index;
        let mut full_text = String::new();

        log::info!(
            "[realtime] worker started: chunk={:.1}s, model='{}', language='{}', rate={}, channels={}, auto_paste={}, target={:?}",
            CHUNK_SECONDS,
            settings.active_model,
            settings.language,
            sample_rate,
            channels,
            settings.auto_paste,
            target_focus
        );

        while active.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(POLL_MS));

            let raw_samples = {
                let buf = samples.lock().unwrap();
                let available = buf.len().saturating_sub(next_sample_index);
                if available < raw_chunk_len {
                    continue;
                }
                let end = buf.len();
                let chunk = buf[next_sample_index..end].to_vec();
                next_sample_index = end;
                chunk
            };

            if sample_rms(&raw_samples) < MIN_RMS {
                log::debug!("[realtime] skipping quiet chunk");
                continue;
            }

            let samples_16k = match crate::audio::resample::resample_to_16k(
                raw_samples,
                sample_rate,
                channels_usize,
            ) {
                Ok(samples) => samples,
                Err(error) => {
                    log::warn!("[realtime] resample failed: {}", error);
                    emit_backend_error(&app, format!("Realtime resample error: {}", error));
                    continue;
                }
            };

            if !active.load(Ordering::Relaxed) {
                break;
            }

            let duration_secs = samples_16k.len() as f32 / 16_000.0;
            log::info!("[realtime] transcribing {:.1}s chunk", duration_secs);

            match transcribe_realtime_chunk(
                &app,
                &samples_16k,
                &settings.language,
                settings.translate_to_english,
                &settings.active_model,
                &model_path,
            ) {
                Ok(text) if !text.trim().is_empty() => {
                    if !active.load(Ordering::Relaxed) {
                        break;
                    }
                    let text = text.trim().to_string();
                    if !full_text.is_empty() {
                        full_text.push(' ');
                    }
                    full_text.push_str(&text);
                    output_seen.store(true, Ordering::Relaxed);
                    log::info!("[realtime] partial: {:?}", text);
                    emit_realtime_transcription(&app, &text, &full_text);

                    if settings.auto_paste {
                        if let Some(target) = target_focus.clone() {
                            let committed_text = format!("{} ", text);
                            log::info!(
                                "[realtime] typing partial into target {:?}: {:?}",
                                target,
                                committed_text
                            );
                            if let Err(error) =
                                crate::output::paste::type_text_into_target(target, &committed_text)
                            {
                                log::warn!("[realtime] live typing failed: {}", error);
                                emit_backend_error(
                                    &app,
                                    format!("Realtime live typing error: {}", error),
                                );
                            }
                        } else {
                            log::warn!(
                                "[realtime] auto_paste enabled but no target captured; start with the global hotkey from a focused text field"
                            );
                        }
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    if !active.load(Ordering::Relaxed) {
                        break;
                    }
                    log::warn!("[realtime] transcription failed: {}", error);
                    emit_backend_error(&app, format!("Realtime transcription error: {}", error));
                }
            }
        }

        log::info!("[realtime] worker stopped");
    });
}

fn stop_realtime_worker(state: &AppState) {
    if let Some(active) = state.realtime_worker_active.lock().unwrap().take() {
        active.store(false, Ordering::Relaxed);
    }
}

fn start_realtime_worker_if_recording(
    app: &AppHandle,
    state: &AppState,
    settings: Settings,
    start_at_current_audio: bool,
) -> bool {
    let recording_context = {
        let recording = state.recording.lock().unwrap();
        recording.as_ref().map(|handle| {
            let start_sample_index = if start_at_current_audio {
                handle.samples.lock().unwrap().len()
            } else {
                0
            };
            (
                handle.samples.clone(),
                handle.sample_rate,
                handle.channels,
                start_sample_index,
            )
        })
    };

    let Some((samples, sample_rate, channels, start_sample_index)) = recording_context else {
        stop_realtime_worker(state);
        return false;
    };

    {
        let active_slot = state.realtime_worker_active.lock().unwrap();
        if active_slot
            .as_ref()
            .is_some_and(|active| active.load(Ordering::Relaxed))
        {
            *state.realtime_used_in_recording.lock().unwrap() = true;
            return true;
        }
    }

    let realtime_active = Arc::new(AtomicBool::new(true));
    {
        let mut active_slot = state.realtime_worker_active.lock().unwrap();
        if let Some(previous) = active_slot.take() {
            previous.store(false, Ordering::Relaxed);
        }
        *active_slot = Some(realtime_active.clone());
    }
    *state.realtime_used_in_recording.lock().unwrap() = true;

    let target_focus = state.target_focus.lock().unwrap().clone();
    spawn_realtime_transcription_worker(
        app.clone(),
        realtime_active,
        samples,
        sample_rate,
        channels,
        settings,
        target_focus,
        start_sample_index,
        state.realtime_output_seen.clone(),
    );
    true
}

fn sync_realtime_worker(
    app: &AppHandle,
    state: &AppState,
    enabled: bool,
    settings: Settings,
    start_at_current_audio: bool,
) -> bool {
    if enabled {
        start_realtime_worker_if_recording(app, state, settings, start_at_current_audio)
    } else {
        stop_realtime_worker(state);
        false
    }
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
    let realtime_samples = handle.samples.clone();
    let realtime_sample_rate = handle.sample_rate;
    let realtime_channels = handle.channels;
    let realtime_target_focus = state.target_focus.lock().unwrap().clone();
    let target_captured = realtime_target_focus.is_some();
    *state.recording.lock().unwrap() = Some(handle);
    *state.realtime_used_in_recording.lock().unwrap() = settings.realtime_transcription;
    state.realtime_output_seen.store(false, Ordering::Relaxed);

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

    if settings.realtime_transcription {
        if let Some(active) = state.realtime_worker_active.lock().unwrap().take() {
            active.store(false, Ordering::Relaxed);
        }
        let realtime_active = Arc::new(AtomicBool::new(true));
        *state.realtime_worker_active.lock().unwrap() = Some(realtime_active.clone());
        spawn_realtime_transcription_worker(
            app.clone(),
            realtime_active,
            realtime_samples,
            realtime_sample_rate,
            realtime_channels,
            settings.clone(),
            realtime_target_focus,
            0,
            state.realtime_output_seen.clone(),
        );
    }

    app.emit(
        "recording-started",
        serde_json::json!({
            "realtime": settings.realtime_transcription,
            "auto_paste": settings.auto_paste,
            "target_captured": target_captured,
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn start_recording_from_settings(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    *state.target_focus.lock().unwrap() = None;
    start_recording(app, state).await
}

#[tauri::command]
pub async fn start_recording_from_settings_with_target_delay(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.hide();
    }

    tokio::time::sleep(Duration::from_millis(1600)).await;
    let target = crate::output::paste::get_frontmost_target();
    #[cfg(target_os = "macos")]
    let target = target.filter(|pid| *pid != std::process::id() as i32);
    log::info!(
        "[settings] delayed start captured target_focus = {:?}",
        target
    );
    *state.target_focus.lock().unwrap() = target;

    start_recording(app, state).await
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    // Stop the audio level emitter
    if let Some(active) = state.level_emitter_active.lock().unwrap().take() {
        active.store(false, Ordering::Relaxed);
    }
    if let Some(active) = state.realtime_worker_active.lock().unwrap().take() {
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

    let used_realtime = {
        let mut realtime_used = state.realtime_used_in_recording.lock().unwrap();
        let used_realtime = *realtime_used;
        *realtime_used = false;
        used_realtime
    };
    let realtime_output_seen = state.realtime_output_seen.load(Ordering::Relaxed);
    let skip_final_transcription = used_realtime && realtime_output_seen;
    app.emit(
        "recording-stopped",
        serde_json::json!({
            "finalizing": !skip_final_transcription,
            "realtime": used_realtime,
        }),
    )
    .map_err(|e| e.to_string())?;
    emit_realtime_mode_updated(
        &app,
        state.settings.lock().unwrap().realtime_transcription,
        false,
    );

    if skip_final_transcription {
        log::info!("[realtime] skipping final batch transcription after realtime recording");
        hide_overlay(&app);
        let _ = app.emit(
            "transcription-complete",
            serde_json::json!({ "text": "", "realtime": true }),
        );
        return Ok(());
    }

    if used_realtime {
        log::info!(
            "[realtime] no realtime output was seen before stop; running final batch fallback"
        );
    }

    let samples_16k =
        crate::audio::resample::resample_to_16k(raw_samples, sample_rate, channels as usize)?;
    let (language, auto_paste, translate_to_english, target_focus, active_model, model_path) =
        transcription_inputs(&state);
    let final_auto_paste = auto_paste && !state.settings.lock().unwrap().realtime_transcription;

    spawn_transcription(
        app,
        samples_16k,
        language,
        final_auto_paste,
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
    let old_settings = state.settings.lock().unwrap().clone();
    let old_hotkey = old_settings.hotkey.clone();
    let new_hotkey = settings.hotkey.clone();

    let hotkey_changed = old_hotkey != new_hotkey;
    if hotkey_changed {
        crate::hotkey::manager::re_register_hotkey(&app, &old_hotkey, &new_hotkey)?;
    }

    if let Err(error) = settings.save() {
        if hotkey_changed {
            let _ = crate::hotkey::manager::re_register_hotkey(&app, &new_hotkey, &old_hotkey);
        }
        return Err(error);
    }
    *state.settings.lock().unwrap() = settings.clone();

    let mut realtime_active = state
        .realtime_worker_active
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|active| active.load(Ordering::Relaxed));

    if old_settings.realtime_transcription != settings.realtime_transcription {
        realtime_active = sync_realtime_worker(
            &app,
            &state,
            settings.realtime_transcription,
            settings.clone(),
            true,
        );
        emit_realtime_mode_updated(&app, settings.realtime_transcription, realtime_active);
    }

    let _ = app.emit(
        "settings-updated",
        serde_json::json!({ "settings": settings, "realtime_active": realtime_active }),
    );

    Ok(())
}

#[tauri::command]
pub async fn set_realtime_transcription(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap().clone();
    if settings.realtime_transcription == enabled {
        let active = state
            .realtime_worker_active
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|worker| worker.load(Ordering::Relaxed));
        emit_realtime_mode_updated(&app, enabled, active);
        return Ok(());
    }

    settings.realtime_transcription = enabled;
    settings.save()?;
    *state.settings.lock().unwrap() = settings.clone();

    let realtime_active = sync_realtime_worker(&app, &state, enabled, settings.clone(), true);
    emit_realtime_mode_updated(&app, enabled, realtime_active);
    let _ = app.emit(
        "settings-updated",
        serde_json::json!({ "settings": settings, "realtime_active": realtime_active }),
    );

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

    #[test]
    fn preview_text_keeps_unicode_boundaries() {
        assert_eq!(preview_text("שלום עולם", 5), "שלום ...");
        assert_eq!(preview_text("hello", 100), "hello");
    }
}
