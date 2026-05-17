# Realtime Whisper Plan

This plan tracks the realtime dictation effort described in
`docs/realtime_whisper_discussion.md`. The first phase is intentionally a
standalone POC before integrating anything into the Tauri app.

## Phase 1 - Standalone POC

Goal: prove that local chunked microphone transcription is practical on this
machine.

Scope:

- Keep the experiment outside the app under `poc/`.
- Use the already-downloaded `ggml-base.bin` model from Careless Whisper app
  data.
- Record short microphone chunks.
- Send each chunk to local `whisper.cpp`.
- Print recognized text to the terminal.
- Measure rough latency and transcription quality for English and Hebrew.

Done when:

- The POC can list usable microphone devices from a normal Terminal session.
- Speaking into the mic produces chunk-level text output.
- We know whether CPU is enough or Metal/GPU is required.
- We have notes on chunk duration, language quality, and observed latency.

## Phase 2 - POC Hardening

Goal: make the experiment representative enough to guide app design.

Scope:

- Try chunk sizes around 1, 2, and 3 seconds.
- Compare CPU vs Metal/GPU.
- Add optional overlap between chunks.
- Add simple duplicate-suffix filtering.
- Keep unstable text in terminal output only.
- Capture failure modes from microphone permissions and device selection.

Done when:

- We can describe the expected latency/quality tradeoff.
- We have a preferred chunk size and overlap strategy.
- We know whether basic deduplication is sufficient for a first app prototype.

## Phase 3 - App Prototype: Internal Partial Text

Goal: integrate the streaming path into Careless Whisper without typing into
other apps yet.

Scope:

- Add a streaming capture path alongside existing batch transcription.
- Send audio chunks to a background transcription worker.
- Emit partial transcript events to the overlay or logs.
- Preserve the existing batch mode as the reliable default.
- Avoid clipboard and paste changes in this phase.

Done when:

- Starting recording creates live partial transcript events.
- Stopping recording shuts down the worker cleanly.
- Batch transcription still works unchanged.

## Phase 4 - Stabilized Dictation Output

Goal: send only stable committed text to the target app.

Scope:

- Split transcript state into partial and committed text.
- Add VAD or silence-based commit boundaries.
- Deduplicate overlapping chunk text.
- Decide whether commits use clipboard paste, simulated typing, or a hybrid.
- Preserve or restore clipboard where possible.

Done when:

- Dictated text appears in the original focused app without obvious duplicates.
- Partial revisions do not corrupt text already committed to the target app.
- Focus changes and paste failures have defined behavior.

## Phase 5 - Productization

Goal: make realtime dictation a user-facing mode.

Scope:

- Add settings for realtime vs batch mode.
- Tune defaults by model size and platform.
- Add cancellation, errors, and performance reporting.
- Test on macOS first, then Windows.
- Keep batch mode available as the dependable fallback.

Done when:

- Realtime dictation is usable for normal writing.
- Performance and edge cases are documented.
- The implementation can be reviewed as a bounded app feature.
