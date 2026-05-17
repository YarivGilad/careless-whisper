#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

exec poc/vendor/whisper.cpp/build/bin/whisper-stream \
  -m poc/models/ggml-base.bin \
  -l en \
  -t 6 \
  --step 500 \
  --length 5000 \
  --keep 500 \
  "$@"
