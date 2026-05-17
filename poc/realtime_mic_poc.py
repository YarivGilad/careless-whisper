#!/usr/bin/env python3
"""Chunked microphone-to-Whisper proof of concept.

This intentionally keeps the POC outside the Tauri app. It records short audio
windows with ffmpeg, runs whisper.cpp on each WAV file, and prints transcripts.
"""

from __future__ import annotations

import argparse
import math
import os
import queue
import shutil
import subprocess
import sys
import threading
import time
import wave
from array import array
from pathlib import Path


ROOT = Path(__file__).resolve().parent
DEFAULT_WHISPER = ROOT / "vendor" / "whisper.cpp" / "build" / "bin" / "whisper-cli"
DEFAULT_MODEL = ROOT / "models" / "ggml-base.bin"
DEFAULT_AUDIO_DIR = ROOT / "audio"
SAMPLE_RATE = 16000
CHANNELS = 1
BYTES_PER_SAMPLE = 2


def require_file(path: Path, label: str) -> None:
    if not path.exists():
        raise SystemExit(f"{label} not found: {path}")


def require_exe(name: str) -> str:
    path = shutil.which(name)
    if not path:
        raise SystemExit(f"Missing required executable on PATH: {name}")
    return path


def list_devices(ffmpeg: str) -> int:
    cmd = [
        ffmpeg,
        "-hide_banner",
        "-f",
        "avfoundation",
        "-list_devices",
        "true",
        "-i",
        "",
    ]
    subprocess.run(cmd, check=False)
    return 0


def record_chunk(
    ffmpeg: str,
    device: str,
    seconds: float,
    output_path: Path,
) -> None:
    cmd = [
        ffmpeg,
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-f",
        "avfoundation",
        "-i",
        device,
        "-t",
        f"{seconds:.3f}",
        "-ac",
        "1",
        "-ar",
        "16000",
        "-c:a",
        "pcm_s16le",
        "-y",
        str(output_path),
    ]
    result = subprocess.run(cmd, check=False)
    if result.returncode != 0:
        raise RuntimeError(
            "ffmpeg could not record from the microphone. "
            "Run with --list-devices, check --device, and confirm macOS "
            "microphone permission for your terminal."
        )


def start_continuous_capture(ffmpeg: str, device: str) -> subprocess.Popen[bytes]:
    cmd = [
        ffmpeg,
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-f",
        "avfoundation",
        "-i",
        device,
        "-ac",
        str(CHANNELS),
        "-ar",
        str(SAMPLE_RATE),
        "-f",
        "s16le",
        "-c:a",
        "pcm_s16le",
        "-",
    ]
    return subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )


def read_exact(stream, size: int) -> bytes:
    parts: list[bytes] = []
    remaining = size
    while remaining > 0:
        chunk = stream.read(remaining)
        if not chunk:
            break
        parts.append(chunk)
        remaining -= len(chunk)
    return b"".join(parts)


def write_wav(audio_path: Path, pcm_bytes: bytes) -> None:
    with wave.open(str(audio_path), "wb") as wav:
        wav.setnchannels(CHANNELS)
        wav.setsampwidth(BYTES_PER_SAMPLE)
        wav.setframerate(SAMPLE_RATE)
        wav.writeframes(pcm_bytes)


def continuous_capture_reader(
    process: subprocess.Popen[bytes],
    chunk_bytes: int,
    chunks,
    stop_event: threading.Event,
) -> None:
    assert process.stdout is not None
    chunk_index = 0
    while not stop_event.is_set():
        pcm_bytes = read_exact(process.stdout, chunk_bytes)
        if len(pcm_bytes) < chunk_bytes:
            break
        chunk_index += 1
        chunks.put((chunk_index, pcm_bytes, time.time()))


def dbfs(value: float) -> float:
    if value <= 0:
        return float("-inf")
    return 20.0 * math.log10(value / 32768.0)


def format_db(value: float) -> str:
    if math.isinf(value):
        return "-inf"
    return f"{value:.1f}"


def audio_stats(audio_path: Path) -> tuple[float, float, float, int]:
    with wave.open(str(audio_path), "rb") as wav:
        frame_count = wav.getnframes()
        sample_rate = wav.getframerate()
        sample_width = wav.getsampwidth()
        raw = wav.readframes(frame_count)

    if sample_width != 2:
        raise RuntimeError(f"Expected 16-bit PCM WAV, got sample width {sample_width}")

    samples = array("h")
    samples.frombytes(raw)
    if sys.byteorder != "little":
        samples.byteswap()

    if not samples:
        return 0.0, float("-inf"), float("-inf"), 0

    nonzero = sum(1 for sample in samples if sample)
    peak = max(abs(sample) for sample in samples)
    rms = math.sqrt(sum(sample * sample for sample in samples) / len(samples))
    duration = frame_count / sample_rate if sample_rate else 0.0
    return duration, dbfs(rms), dbfs(float(peak)), nonzero


def transcribe_chunk(
    whisper: Path,
    model: Path,
    audio_path: Path,
    language: str,
    threads: int,
    use_gpu: bool,
    no_speech_threshold: float,
) -> str:
    cmd = [
        str(whisper),
        "-m",
        str(model),
        "-f",
        str(audio_path),
        "-l",
        language,
        "-t",
        str(threads),
        "-nt",
        "-np",
        "-sns",
        "-nth",
        str(no_speech_threshold),
    ]
    if not use_gpu:
        cmd.append("-ng")
    result = subprocess.run(cmd, check=False, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "whisper-cli failed")
    return " ".join(result.stdout.split())


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Record microphone chunks and transcribe them with local whisper.cpp."
    )
    parser.add_argument(
        "--list-devices",
        action="store_true",
        help="List macOS AVFoundation capture devices and exit.",
    )
    parser.add_argument(
        "--device",
        default=":1",
        help="AVFoundation input selector. On this Mac, ':1' is the MacBook Pro Microphone.",
    )
    parser.add_argument(
        "--chunk-seconds",
        type=float,
        default=3.0,
        help="Seconds of audio per transcription chunk.",
    )
    parser.add_argument(
        "--capture-mode",
        choices=("continuous", "chunked"),
        default="continuous",
        help="continuous keeps recording while Whisper transcribes; chunked is the older sequential ffmpeg mode.",
    )
    parser.add_argument(
        "--language",
        default="en",
        help="Whisper language code, for example 'en', 'he', or 'auto'.",
    )
    parser.add_argument(
        "--threads",
        type=int,
        default=max(2, min(8, (os.cpu_count() or 4) // 2)),
        help="Whisper compute threads.",
    )
    parser.add_argument(
        "--gpu",
        action="store_true",
        help="Allow whisper.cpp to use the GPU/Metal backend. CPU is the default for POC stability.",
    )
    parser.add_argument(
        "--min-rms-dbfs",
        type=float,
        default=-50.0,
        help="Skip Whisper for chunks quieter than this RMS dBFS level.",
    )
    parser.add_argument(
        "--no-silence-skip",
        action="store_true",
        help="Always send chunks to Whisper, even if recorded audio looks silent.",
    )
    parser.add_argument(
        "--no-speech-threshold",
        type=float,
        default=0.30,
        help="Whisper no-speech threshold. Lower is more aggressive about silence.",
    )
    parser.add_argument(
        "--max-chunks",
        type=int,
        default=0,
        help="Stop after this many chunks. 0 means run until Ctrl+C.",
    )
    parser.add_argument(
        "--keep-audio",
        action="store_true",
        help="Keep recorded WAV chunks under poc/audio for inspection.",
    )
    parser.add_argument(
        "--queue-size",
        type=int,
        default=16,
        help="Maximum captured chunks to queue in continuous mode before applying backpressure.",
    )
    parser.add_argument(
        "--whisper",
        type=Path,
        default=DEFAULT_WHISPER,
        help="Path to whisper-cli.",
    )
    parser.add_argument(
        "--model",
        type=Path,
        default=DEFAULT_MODEL,
        help="Path to a ggml Whisper model.",
    )
    parser.add_argument(
        "--audio-dir",
        type=Path,
        default=DEFAULT_AUDIO_DIR,
        help="Directory for temporary WAV chunks.",
    )
    return parser.parse_args()


def process_recorded_chunk(
    args: argparse.Namespace,
    audio_path: Path,
    chunk_index: int,
    started: float,
    recording_elapsed: float,
    queue_age: float | None = None,
) -> str | None:
    duration, rms, peak, nonzero = audio_stats(audio_path)
    audio_summary = (
        f"audio={duration:.1f}s rms={format_db(rms)}dBFS "
        f"peak={format_db(peak)}dBFS nonzero={nonzero}"
    )
    queue_summary = f" queue={queue_age:.1f}s" if queue_age is not None else ""
    if nonzero == 0:
        elapsed = time.time() - started
        print(
            f"[{chunk_index:04d}] {elapsed:.1f}s | "
            f"record={recording_elapsed:.1f}s{queue_summary} | "
            f"<all-zero audio: {audio_summary}. "
            "Check mic permission or --device index.>",
            flush=True,
        )
        return None
    if not args.no_silence_skip and rms < args.min_rms_dbfs:
        elapsed = time.time() - started
        print(
            f"[{chunk_index:04d}] {elapsed:.1f}s | "
            f"record={recording_elapsed:.1f}s{queue_summary} | "
            f"<skipped quiet chunk: {audio_summary}>",
            flush=True,
        )
        return None

    transcribe_started = time.time()
    text = transcribe_chunk(
        args.whisper,
        args.model,
        audio_path,
        args.language,
        args.threads,
        args.gpu,
        args.no_speech_threshold,
    )
    transcribe_elapsed = time.time() - transcribe_started
    elapsed = time.time() - started
    if text:
        print(
            f"[{chunk_index:04d}] {elapsed:.1f}s | "
            f"record={recording_elapsed:.1f}s "
            f"whisper={transcribe_elapsed:.1f}s{queue_summary} | "
            f"{audio_summary}",
            flush=True,
        )
        print(
            f"TEXT [{chunk_index:04d}]: {text}",
            flush=True,
        )
        return text

    print(
        f"[{chunk_index:04d}] {elapsed:.1f}s | "
        f"record={recording_elapsed:.1f}s "
        f"whisper={transcribe_elapsed:.1f}s{queue_summary} | "
        f"{audio_summary} | <no speech>",
        flush=True,
    )
    return None


def main() -> int:
    args = parse_args()
    ffmpeg = require_exe("ffmpeg")

    if args.list_devices:
        return list_devices(ffmpeg)

    require_file(args.whisper, "whisper-cli")
    require_file(args.model, "Whisper model")

    args.audio_dir.mkdir(parents=True, exist_ok=True)
    print(
        "Recording chunks. Speak into the selected mic; press Ctrl+C to stop.",
        flush=True,
    )
    print(
        f"device={args.device} chunk={args.chunk_seconds:.1f}s "
        f"mode={args.capture_mode} language={args.language} gpu={args.gpu} "
        f"min_rms={format_db(args.min_rms_dbfs)}dBFS model={args.model}",
        flush=True,
    )

    chunk_index = 0
    transcript_parts: list[str] = []
    stop_event = threading.Event()
    capture_process: subprocess.Popen[bytes] | None = None
    reader_thread: threading.Thread | None = None
    try:
        if args.capture_mode == "continuous":
            chunk_bytes = int(args.chunk_seconds * SAMPLE_RATE * CHANNELS * BYTES_PER_SAMPLE)
            chunks = queue.Queue(maxsize=max(1, args.queue_size))
            capture_process = start_continuous_capture(ffmpeg, args.device)
            reader_thread = threading.Thread(
                target=continuous_capture_reader,
                args=(capture_process, chunk_bytes, chunks, stop_event),
                daemon=True,
            )
            reader_thread.start()
            print("continuous capture active; speak normally.", flush=True)

            while args.max_chunks <= 0 or chunk_index < args.max_chunks:
                try:
                    queued_index, pcm_bytes, captured_at = chunks.get(timeout=0.5)
                except queue.Empty:
                    if capture_process.poll() is not None:
                        raise RuntimeError(
                            "ffmpeg continuous capture exited before producing audio. "
                            "Check microphone permission and --device."
                        )
                    continue
                chunk_index = queued_index
                started = time.time()
                audio_path = args.audio_dir / f"chunk-{chunk_index:04d}.wav"
                write_wav(audio_path, pcm_bytes)
                text = process_recorded_chunk(
                    args,
                    audio_path,
                    chunk_index,
                    started,
                    args.chunk_seconds,
                    queue_age=started - captured_at,
                )
                if text:
                    transcript_parts.append(text)
                if not args.keep_audio:
                    audio_path.unlink(missing_ok=True)
        else:
            while args.max_chunks <= 0 or chunk_index < args.max_chunks:
                chunk_index += 1
                audio_path = args.audio_dir / f"chunk-{chunk_index:04d}.wav"
                started = time.time()
                print(f"[{chunk_index:04d}] recording...", flush=True)
                recording_started = time.time()
                record_chunk(ffmpeg, args.device, args.chunk_seconds, audio_path)
                recording_elapsed = time.time() - recording_started
                text = process_recorded_chunk(
                    args,
                    audio_path,
                    chunk_index,
                    started,
                    recording_elapsed,
                )
                if text:
                    transcript_parts.append(text)
                if not args.keep_audio:
                    audio_path.unlink(missing_ok=True)
    except KeyboardInterrupt:
        print("\nStopped.", flush=True)
    except RuntimeError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    finally:
        stop_event.set()
        if capture_process is not None and capture_process.poll() is None:
            capture_process.terminate()
            try:
                capture_process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                capture_process.kill()
        if reader_thread is not None:
            reader_thread.join(timeout=1)

    if transcript_parts:
        print("\nFULL TRANSCRIPT:", flush=True)
        print(" ".join(transcript_parts), flush=True)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
