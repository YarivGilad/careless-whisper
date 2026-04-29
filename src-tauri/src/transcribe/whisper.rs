use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub fn load_model(path: &Path) -> Result<WhisperContext, String> {
    let path_str = path.to_str().ok_or("Invalid model path")?;

    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    log::info!(
        "[whisper] loading model from {} ({:.1} MB)",
        path.display(),
        file_size as f64 / 1_048_576.0
    );

    WhisperContext::new_with_params(path_str, WhisperContextParameters::default()).map_err(|e| {
        format!(
            "Failed to load model: {:?}. Path: {}, size: {} bytes. \
             The file may be corrupted — try deleting and re-downloading it.",
            e,
            path.display(),
            file_size
        )
    })
}

pub fn transcribe(ctx: &WhisperContext, samples: &[f32], language: &str, translate: bool) -> Result<String, String> {
    let mut state = ctx.create_state().map_err(|e| format!("{:?}", e))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_token_timestamps(false);
    params.set_translate(translate);
    // Stop whisper from emitting subtitle-style annotations like "(silence)",
    // "[typing]", "(door creaking)" when the audio is short, quiet, or noisy.
    params.set_suppress_non_speech_tokens(true);
    params.set_no_speech_thold(0.5);

    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get().min(16) as i32)
        .unwrap_or(4);
    params.set_n_threads(n_threads);

    let duration_secs = samples.len() as f32 / 16000.0;
    let single_segment = duration_secs < 30.0;
    params.set_single_segment(single_segment);

    let lang_label = if language == "auto" || language.is_empty() { "auto" } else { language };
    log::info!(
        "[whisper] transcribing {:.1}s audio | threads={} | language={} | single_segment={}",
        duration_secs, n_threads, lang_label, single_segment
    );

    // "auto" → empty string triggers whisper.cpp auto-detect
    if language != "auto" && !language.is_empty() {
        params.set_language(Some(language));
    }

    state
        .full(params, samples)
        .map_err(|e| format!("Transcription failed: {:?}", e))?;

    let num_segments = state.full_n_segments().map_err(|e| format!("{:?}", e))?;

    let mut text = String::new();
    for i in 0..num_segments {
        if let Ok(segment) = state.full_get_segment_text(i) {
            text.push_str(&segment);
        }
    }

    let text = text
        .replace("[BLANK_AUDIO]", "")
        .replace("[BLANK AUDIO]", "");

    let trimmed = text.trim();

    // Catch any subtitle-style annotation that slipped past suppress_non_speech_tokens —
    // e.g. "(silence)", "[ Silence ]", "(keyboard clicking)", "(door creaking)".
    // If the entire output is one bracketed annotation, treat it as empty.
    let looks_like_annotation = {
        let inner = trimmed
            .strip_suffix('.')
            .unwrap_or(trimmed)
            .trim();
        (inner.starts_with('(') && inner.ends_with(')'))
            || (inner.starts_with('[') && inner.ends_with(']'))
    };
    if looks_like_annotation {
        return Ok(String::new());
    }

    Ok(trimmed.to_string())
}
