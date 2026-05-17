# Realtime Whisper Current Issues

This file tracks open questions and known problems for the realtime dictation
effort. Keep it short and current.

## Open Issues

### App Prototype Needs Live Validation

The Rust/Tauri realtime prototype is wired in, but it still needs more real app
testing with microphone permission, the overlay window, captured target focus,
live chunk paste, and stop behavior together.

Next action: run the app, enable **Realtime transcription** and **Auto-paste
after transcription**, start from a focused text field with the global hotkey,
record the English control paragraph from `poc/test_samples.md`, and compare
overlay partials, live pasted chunks, and whether stop exits cleanly without a
second batch transcription pass after at least one live chunk appears.

### Continuous Runner Latency Is Still Noticeable

With the correct microphone device (`:1`), continuous capture is good enough to
prove audio/model quality but still feels slow for realtime dictation. The first
continuous English control result was broadly correct, but text still appears
after chunk boundaries rather than truly live.

Mitigation already added: `realtime_mic_poc.py` now defaults to continuous
ffmpeg capture, so recording continues while Whisper transcribes previous
chunks. This reduces dropped words from capture gaps, but it still launches
`whisper-cli` and loads the model for every chunk.

Next action: test the English sample with continuous capture:

```sh
python3 poc/realtime_mic_poc.py --gpu --chunk-seconds 5 --max-chunks 8 --keep-audio
```

Then compare a persistent-model path:

```sh
poc/run_whisper_stream.sh
```

If the default SDL capture is wrong, inspect the capture device list printed at
startup and only then try `poc/run_whisper_stream.sh --capture N`.

### Stream Mode Quality Is Poor So Far

The first stream test with `--capture 1` hallucinated text, which suggests SDL
capture ID `1` is not the same as ffmpeg's AVFoundation `:1`. Running without
`--capture` produced Hebrew output, but it was garbled and repeated phrases like
`תודה רבה`. Timings ended around 57 seconds with many fallbacks, so this is not
yet a usable realtime path.

Next action: use the English sample in `poc/test_samples.md` first as the
control case, then compare Hebrew once the pipeline behavior is understood.

### English Quality Has Minor Chunk Artifacts

The first continuous English result was readable and mostly accurate, but still
included minor artifacts: duplicated "I'm", "testing" misheard as "just in",
and small missing/substituted words. These look like chunk-boundary/context
issues rather than broken capture.

Next action: after latency, test chunk duration and overlap/deduplication.

### Hebrew Quality Is Mixed With Base Model

The first valid mic run produced one good Hebrew chunk:
`אפשר להמשיך לעבוד בצורה כזאתי`, but earlier chunks were garbled. This may be
from short chunks, lack of context/VAD, the base model, or normal Whisper
instability on small audio windows.

Next action: defer Hebrew tuning until English chunked and stream baselines are
understood.

## Resolved / Explained

### Mic Capture Was All-Zero Audio

The second live command:

```sh
python3 poc/realtime_mic_poc.py --device ':0' --language he --gpu --max-chunks 3 --keep-audio
```

produced chunks with `rms=-inf` and `peak=-inf`. Inspecting the saved WAV files
confirmed every sample was zero. This is digital silence, not low-quality
Hebrew transcription.

Confirmed cause:

- `--device ':0'` mapped to `Virtual Desktop Speakers`, not the MacBook mic.
- `--device ':1'` maps to `MacBook Pro Microphone`.

Resolution: use `--device ':1'`, now also the POC default on this machine.

### First Hebrew GPU Run Hallucinated Repeated Text

The first live command:

```sh
python3 poc/realtime_mic_poc.py --device ':0' --language he --gpu
```

returned repeated `הוא עוד קצת את זה` phrases for multiple chunks. That does
not look like usable Hebrew dictation. Later device listing showed `:0` was
`Virtual Desktop Speakers`, so this was Whisper hallucinating on all-zero audio.

Resolution: re-run with `--device ':1'`; the hallucinated phrase was from
recording all-zero audio on `:0`.

### Mic Access In Codex Sandbox

`ffmpeg` did not list AVFoundation audio devices from the Codex sandbox. This
does not mean the POC is broken; it means live mic validation needs to run from
normal Terminal/iTerm with macOS microphone permission.

Next action: run `python3 poc/realtime_mic_poc.py --list-devices` outside Codex.

### Metal/GPU Path Needs A Real Terminal Test

The `whisper.cpp` build detected Metal support, but the first reliable smoke
test used CPU with `-ng`. GPU behavior should be tested from a normal terminal
before assuming realtime latency numbers.

Next action: compare `--gpu` vs default CPU on the same spoken phrase.

### Sequential Chunked Capture Was Creating Gaps

The first POC recorded one complete chunk, transcribed it, printed text, then
recorded the next chunk. That proved local chunked transcription, but it left
gaps while Whisper was running.

Resolution: `realtime_mic_poc.py` now defaults to continuous ffmpeg capture, and
the Rust prototype reads from the active app capture buffer while recording. The
remaining issue is latency, not dropped audio during transcription.

### Tauri Dev Linker Failure With Optimized Incremental Builds

`pnpm tauri dev` was failing at link time on macOS arm64 with undefined
`_anon...llvm...` serde symbols, while normal `cargo check` still passed. The
failing command was reproduced as:

```sh
cargo build --manifest-path src-tauri/Cargo.toml --no-default-features --features metal
```

Resolution: the same command linked cleanly with `CARGO_INCREMENTAL=0`, so
`src-tauri/Cargo.toml` now sets `incremental = false` for the optimized dev
profile.

### Duplicate Text Handling Is Unimplemented

Overlapping windows are not active yet, so there is no duplicate-suffix or
stable-prefix logic. This will matter as soon as we add overlap.

Next action: add overlap only after baseline chunk latency is measured.

### Realtime Commit Semantics Are Still Naive

The app now pastes realtime chunks into the captured hotkey target when
**Auto-paste after transcription** is enabled. On macOS this now uses Unicode
keyboard events instead of clipboard-plus-Cmd+V when a target was captured. This
validates the product path, but it commits whole chunk output directly. There is no overlap,
duplicate-suffix handling, stable-prefix logic, or edit/replace strategy yet.

Next action: test naive chunk commits first. If words repeat or chunk boundaries
feel rough, add overlap plus deduplication before trying any arbitrary-app
replace behavior.

### Settings Start Has No External Text Target

The Settings **Start Recording** button used to clear `target_focus`, so it
could not type into the app where the user previously had a cursor. Clicking
Settings changes the focused app to Settings itself, so immediate target capture
from that button is not useful.

Resolution: the button now hides Settings, waits briefly, captures the newly
focused frontmost app, and then starts recording. The hotkey remains the most
reliable start path.

### VAD Is Deferred

No voice activity detection is included in the POC. Without VAD, Whisper may
process silence and produce unstable or empty chunks.

Next action: evaluate whether chunked output is usable enough before adding VAD.

### App Integration Must Preserve Batch Mode

The current batch path is simple and reliable. Realtime work landed as an
opt-in settings flag and overlay-only partial text, not as a replacement for the
existing stop-then-transcribe behavior.

Resolution: batch mode remains the default fallback. In realtime mode,
auto-paste inserts live chunks during recording. Stop skips the final batch
transcription pass after realtime has produced user-visible output; very short
recordings fall back to the final pass if no live chunk returned.
