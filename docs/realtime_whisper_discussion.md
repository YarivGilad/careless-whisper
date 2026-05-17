# Realtime Whisper Dictation Discussion

## Current Transcription Flow

Careless Whisper currently uses a batch transcription model:

1. The user presses the global hotkey.
2. The app captures microphone audio into memory.
3. The user presses the hotkey again, or releases it in push-to-talk mode.
4. Capture stops.
5. The full audio buffer is resampled to mono 16 kHz.
6. Whisper transcribes the complete buffer once.
7. The final text is copied to the clipboard and optionally pasted into the app that was focused before recording started.

The live microphone path is hotkey-driven, not a visible React record button.

Relevant code:

- `src-tauri/src/hotkey/manager.rs`: registers the global hotkey and emits `hotkey-start` / `hotkey-stop`.
- `src/App.tsx`: the overlay window listens for those events and invokes `start_recording` / `stop_recording`.
- `src-tauri/src/commands.rs`: owns the command flow, recording lifecycle, transcription worker, clipboard copy, and paste.
- `src-tauri/src/audio/capture.rs`: uses `cpal` to capture audio from the default input device.
- `src-tauri/src/audio/resample.rs`: converts captured audio to mono 16 kHz.
- `src-tauri/src/transcribe/whisper.rs`: wraps `whisper-rs` and runs Whisper over a complete audio buffer.

There is also a settings UI button for transcribing existing audio files, but that is separate from live microphone capture.

## Desired Realtime Behavior

The desired behavior is closer to browser dictation:

1. The microphone buffer streams in continuously.
2. Audio chunks are sent to Whisper while recording is still active.
3. Text comes out incrementally.
4. The text is sent into the currently focused app, so the user can speak anywhere they would normally type.

This is similar in user experience to browser speech recognition, commonly exposed through the Web Speech API as `SpeechRecognition` / `webkitSpeechRecognition`, but system-wide instead of limited to a browser input or web component.

The advantage of doing this locally with Whisper is that dictation could work across the whole desktop and without cloud transcription.

## Is This Possible Here?

Yes. The repo already has several important pieces:

- Microphone capture through `cpal`.
- Local Whisper inference through `whisper-rs`.
- Settings, language selection, model management, and model reuse.
- Focus capture before recording starts.
- Clipboard and paste integration for sending text into another app.
- Overlay UI for recording/transcribing state.

The missing piece is a streaming dictation pipeline between microphone capture and text output.

This would not be a small flag on the current implementation. The current code is structured around:

```text
record full audio -> stop -> resample -> transcribe once -> paste final text
```

Realtime dictation would need something more like:

```text
capture chunks -> run continuous worker -> transcribe overlapping windows -> stabilize text -> commit text to target app
```

## Main Architecture Change

The microphone capture layer currently appends samples to a `Vec<f32>` and returns that complete buffer when recording stops.

Realtime dictation would likely replace or extend this with a channel or ring buffer:

1. `cpal` callback receives audio frames.
2. Frames are pushed into a thread-safe stream buffer.
3. A transcription worker consumes chunks while recording continues.
4. Chunks are converted to mono 16 kHz.
5. Whisper runs on rolling windows of recent audio.
6. New stable text is emitted to the output path.

The batch mode can still remain as the default or fallback path.

## Difficulties

### Whisper Is Not Naturally Browser-Style Realtime

The current Whisper call uses `state.full(params, samples)`, which expects an audio window and returns segments for that window. It is not the same kind of streaming API as browser speech recognition.

Whisper can be used for near-realtime transcription by repeatedly running it on short audio windows, but it may revise earlier words when it receives more context.

### Latency vs Accuracy

Small chunks produce faster feedback but worse stability.

Larger chunks improve accuracy but feel less realtime.

A practical first target would probably use 1-3 second chunks with overlap, not character-by-character transcription.

### Duplicate Text Handling

Sliding windows will repeat content. For example, a 3-second window with 1 second of overlap may transcribe words that were already emitted.

The app would need logic to compare the new transcript against previously committed text and emit only the stable new suffix.

### Partial vs Committed Text

In a browser text area, an app can show partial text and then replace it when recognition revises the phrase.

System-wide dictation is harder. Once text is pasted or typed into another app, changing it requires backspaces, selections, or replacement commands. That gets brittle across arbitrary desktop apps.

A safer approach is:

- Keep unstable partial text internal or in the overlay.
- Only send text to the target app once it is stable enough.
- Optionally commit on silence boundaries detected by VAD.

### Voice Activity Detection

Realtime dictation benefits from VAD or silence detection. Without it, Whisper may run too often, waste compute, and emit unstable text.

VAD could help decide:

- When speech starts.
- When a phrase is complete.
- When to commit text to the focused app.
- When to ignore silence.

### CPU/GPU Load

The current app runs Whisper once after capture stops. Realtime mode would run Whisper repeatedly while recording.

This is much heavier. Tiny/base models may be practical. Larger models may introduce too much latency or CPU/GPU usage.

### Model Context and Threading

The current app stores a reusable `whisper_ctx` behind a mutex. Realtime dictation would likely need a long-running transcription worker that owns or carefully coordinates access to the Whisper context.

The design should avoid blocking:

- Hotkey handling.
- UI event handling.
- Audio capture callbacks.
- Clipboard/paste output.

### Output Into Arbitrary Apps

The repo already has clipboard and paste support, but realtime typing raises new edge cases:

- What if focus changes while dictating?
- Should text continue going to the originally focused app or the current app?
- What if paste fails midway?
- Should output use clipboard paste, simulated key events, or both?
- How should the app avoid destroying the user's clipboard for incremental output?

The current final-paste path is simpler because it only writes once.

## Likely Implementation Plan

### Prototype

Build a rough streaming mode while keeping the existing batch mode intact:

1. Add a streaming capture path that sends audio chunks over a channel.
2. Add a transcription worker started by `start_recording`.
3. Accumulate 2-3 seconds of audio, resample, and transcribe.
4. Use overlapping windows.
5. Log or emit partial transcripts to the overlay first.
6. Add basic deduplication against prior text.
7. Commit stable text to clipboard/paste only after short silence or chunk boundaries.

This would prove feasibility without committing to a perfect UX immediately.

### Usable Dictation Mode

After the prototype works:

1. Add VAD or silence detection.
2. Split text into partial and committed states.
3. Tune chunk size and overlap by model size.
4. Add settings for realtime mode vs current batch mode.
5. Improve output behavior so text appears naturally in the target app.
6. Handle cancellation, errors, and focus changes.

### Polished System-Wide Dictation

For a polished version:

1. Add robust incremental transcript stabilization.
2. Consider true keyboard event output for small text increments.
3. Preserve and restore clipboard more carefully.
4. Add platform-specific testing on macOS, Windows, and Linux.
5. Add performance tuning by model.
6. Add overlay UI for partial text and current dictation state.

## Effort Estimate

Prototype: 1-2 days.

This would demonstrate chunked capture, repeated Whisper calls, and basic incremental output. It would likely have duplicate text and stability issues.

Usable dictation mode: 4-7 days.

This would include VAD, overlap handling, deduplication, committed-vs-partial text, cancellation, and a reasonable output path.

Polished system-wide realtime dictation: 2-4 weeks.

This would require careful UX, performance tuning, platform-specific behavior, focus edge cases, robust paste/typing behavior, and tests.

## Initial Recommendation

Treat this as a new realtime dictation mode, not as a small modification to the existing transcription command.

The current batch mode should stay because it is simpler, accurate, and reliable. Realtime mode can be added alongside it with a separate capture/transcription worker pipeline.

The first milestone should not try to type into arbitrary apps immediately. It should first stream partial text to the overlay or logs. Once chunking, latency, and deduplication are understood, then connect stable committed text to the existing clipboard/paste path.

## Secondary Research Question

The secondary question is whether an existing local Whisper project already provides system-wide realtime dictation.

That should be researched separately, with attention to:

- Whether it uses Whisper locally or a cloud API.
- Whether it supports system-wide dictation into arbitrary apps.
- Whether it solves incremental deduplication and partial commits.
- Whether it supports macOS, Windows, and Linux.
- Whether its architecture can be reused or studied for this app.
