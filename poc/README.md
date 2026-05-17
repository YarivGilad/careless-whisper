# Realtime Whisper POC

This folder is a standalone proof of concept for chunked local dictation before
integrating realtime transcription into the Tauri app.

## What It Does

- Records short microphone chunks with `ffmpeg`.
- Transcribes each chunk with a local `whisper.cpp` build.
- Prints recognized text to the terminal as each chunk finishes.

This is intentionally not polished streaming dictation yet. It answers the
first practical question: whether local chunked Whisper has acceptable latency
and quality on this machine.

## Local Setup

The POC uses:

- `vendor/whisper.cpp/` cloned from `ggerganov/whisper.cpp`.
- `models/ggml-base.bin`, symlinked to the model already downloaded by the
  installed Careless Whisper app.
- `realtime_mic_poc.py`, a stdlib-only Python runner.

## File Map

Intentional POC files:

- `README.md` - this overview and runbook.
- `realtime_mic_poc.py` - chunked ffmpeg capture plus `whisper-cli`
  transcription.
- `run_whisper_stream.sh` - wrapper for the lower-latency `whisper-stream`
  experiment.
- `test_samples.md` - fixed English and Hebrew read-aloud paragraphs for
  repeatable comparisons.
- `.gitignore` - keeps generated audio, model binaries, vendored code, and
  Python cache files out of git.

Generated or external POC paths:

- `audio/` - kept WAV/AIFF recordings from test runs; safe to delete when not
  needed for comparison.
- `models/ggml-base.bin` - symlink to the installed app's downloaded base
  model.
- `vendor/whisper.cpp/` - local clone/build of `ggerganov/whisper.cpp`.
- `__pycache__/` - Python bytecode cache.

Related tracking docs:

- `../docs/realtime_whisper/plan.md`
- `../docs/realtime_whisper/implementation_tracker.md`
- `../docs/realtime_whisper/current_issues.md`
- `../docs/realtime_whisper_discussion.md`

## Run

List macOS microphone devices:

```sh
python3 poc/realtime_mic_poc.py --list-devices
```

Start continuous capture using the MacBook Pro Microphone seen on this machine:

```sh
python3 poc/realtime_mic_poc.py
```

This is the preferred runner now. It keeps ffmpeg recording while Whisper
transcribes previous chunks, avoiding the word-loss gap from the original
sequential POC.

For Hebrew comparison:

```sh
python3 poc/realtime_mic_poc.py --language he
```

Press `Ctrl+C` to stop.

For consistent comparisons, use the fixed read-aloud paragraphs in
`poc/test_samples.md`.

## Lower-Latency Stream Test

`whisper-stream` is also built locally. Unlike the Python chunk runner, it keeps
the model loaded and captures continuously through SDL2:

```sh
poc/run_whisper_stream.sh
```

For Hebrew comparison:

```sh
poc/run_whisper_stream.sh -l he
```

SDL2 capture IDs do not necessarily match ffmpeg/AVFoundation IDs. If the
default device is wrong, inspect the capture devices printed during startup and
then pin one:

```sh
poc/run_whisper_stream.sh --capture N
```

Stop it with `Ctrl+C`.

By default the POC runs `whisper.cpp` on CPU because that is the most reliable
first smoke test from a terminal. Add `--gpu` to try the Metal path:

```sh
python3 poc/realtime_mic_poc.py --gpu
```

To compare against the older sequential mode:

```sh
python3 poc/realtime_mic_poc.py --capture-mode chunked --gpu
```

Each chunk prints basic audio levels:

```text
[0001] 4.8s | record=3.0s whisper=1.8s | audio=3.0s rms=-28.4dBFS peak=-7.2dBFS nonzero=48000
TEXT [0001]: Hello, this is a short test for real time dictation.
```

The POC prints text in the terminal only. It does not paste into another app or
show the app overlay yet. At the end of the run it prints a combined
`FULL TRANSCRIPT` block.

If RMS is around `-50dBFS` or lower while you are speaking, the selected device
is probably wrong, muted, or too quiet. The runner skips very quiet chunks by
default to avoid Whisper hallucinating on silence. To force transcription of
every chunk:

```sh
python3 poc/realtime_mic_poc.py --language he --gpu --no-silence-skip
```

If the output says `all-zero audio` or `nonzero=0`, ffmpeg recorded digital
silence. Check:

- System Settings -> Privacy & Security -> Microphone has Terminal/iTerm enabled.
- `python3 poc/realtime_mic_poc.py --list-devices` shows the device you expect.
- The `--device ':N'` audio index matches the real microphone.

On this Mac, the relevant devices seen so far were:

- `:0` - Virtual Desktop Speakers, not a microphone for this POC.
- `:1` - MacBook Pro Microphone.
- `:2` - Virtual Desktop Mic.

The runner defaults to `:1` for this machine.

## Notes

- macOS may prompt for microphone access for the terminal app.
- The Codex sandbox may not expose microphone devices; run the mic command from
  your normal Terminal/iTerm session if `--list-devices` is empty here.
- The default chunk size is 3 seconds, so text appears after each chunk and its
  transcription complete.
- If the wrong mic is used, run `--list-devices` and pass the audio index as
  `--device ':N'`.
