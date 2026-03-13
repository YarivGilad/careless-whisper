use tauri::{AppHandle, Emitter, Manager, State};

use crate::config::settings::Settings;
use crate::models::downloader::{self, ModelInfo};
use crate::AppState;

// ── Recording ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let max_seconds = state.settings.lock().unwrap().max_recording_seconds;

    let handle = crate::audio::capture::start_capture(max_seconds)?;
    *state.recording.lock().unwrap() = Some(handle);

    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.show();
    }

    app.emit("recording-started", ()).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let handle = state
        .recording
        .lock()
        .unwrap()
        .take()
        .ok_or("Not recording")?;

    let (raw_samples, sample_rate) = crate::audio::capture::stop_capture(handle);

    app.emit("recording-stopped", ()).map_err(|e| e.to_string())?;

    let samples_16k = crate::audio::resample::resample_to_16k(raw_samples, sample_rate, 1);

    let language = state.settings.lock().unwrap().language.clone();
    let auto_paste = state.settings.lock().unwrap().auto_paste;
    let target_focus = *state.target_focus.lock().unwrap();
    let active_model = state.settings.lock().unwrap().active_model.clone();
    let model_path = downloader::model_path(&active_model);

    let app_clone = app.clone();

    tokio::task::spawn_blocking(move || {
        let state = app_clone.state::<AppState>();

        // Reuse cached model context, or load and cache it on first use.
        let ctx = state.whisper_ctx.lock().unwrap().take();
        let ctx = match ctx {
            Some(c) => c,
            None => match crate::transcribe::whisper::load_model(&model_path) {
                Ok(c) => c,
                Err(e) => {
                    let _ = app_clone.emit(
                        "transcription-error",
                        serde_json::json!({ "message": e }),
                    );
                    if let Some(win) = app_clone.get_webview_window("overlay") {
                        let _ = win.hide();
                    }
                    return;
                }
            },
        };

        let result = crate::transcribe::whisper::transcribe(&ctx, &samples_16k, &language);

        // Put the context back for next recording
        *state.whisper_ctx.lock().unwrap() = Some(ctx);

        match result {
            Ok(text) => {
                let _ = crate::output::clipboard::copy_to_clipboard(&text);

                if let Some(win) = app_clone.get_webview_window("overlay") {
                    let _ = win.hide();
                }

                let _ = app_clone.emit(
                    "transcription-complete",
                    serde_json::json!({ "text": text }),
                );

                if auto_paste {
                    if let Some(target) = target_focus {
                        if let Err(e) = crate::output::paste::paste_into_target(target) {
                            eprintln!("[paste error] {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                let _ = app_clone.emit(
                    "transcription-error",
                    serde_json::json!({ "message": e }),
                );
                if let Some(win) = app_clone.get_webview_window("overlay") {
                    let _ = win.hide();
                }
            }
        }
    });

    Ok(())
}

// ── Settings ─────────────────────────────────────────────────────────────────

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

// ── Models ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_models() -> Result<Vec<ModelInfo>, String> {
    Ok(downloader::list_models())
}

#[tauri::command]
pub async fn download_model(app: AppHandle, model: String) -> Result<(), String> {
    downloader::download_model(app, model).await
}

#[tauri::command]
pub async fn delete_model(model: String) -> Result<(), String> {
    downloader::delete_model(&model)
}

#[tauri::command]
pub async fn set_active_model(
    model: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
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
