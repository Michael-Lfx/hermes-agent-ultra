#!/usr/bin/env bash
# Ensure Kokoro RKNN split-model artefacts (optional; sherpa CPU kokoro is the fallback).
#
# Env:
#   KOKORO_SERVER_DIR  default: /home/leeyang/kokoro-server
#   CHECK_ONLY=1       verify only
set -euo pipefail

ROOT="${HERMES_ULTRA_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
KOKORO_SERVER_DIR="${KOKORO_SERVER_DIR:-/home/leeyang/kokoro-server}"
ONNX_DIR="${KOKORO_SERVER_DIR}/onnx"

REQUIRED=(
  "kokoro_encoder.onnx"
  "har_generator.onnx"
  "kokoro_decoder.rknn"
)

missing=()
for f in "${REQUIRED[@]}"; do
  if [[ ! -f "${ONNX_DIR}/${f}" ]]; then
    missing+=("${ONNX_DIR}/${f}")
  fi
done

if [[ ${#missing[@]} -eq 0 ]] \
  && [[ -d "${KOKORO_SERVER_DIR}/voices_npy" ]] \
  && [[ -f "${KOKORO_SERVER_DIR}/Kokoro-82M/config.json" ]]; then
  echo "=== kokoro RKNN models OK (${ONNX_DIR}) ==="
  exit 0
fi

echo "=== kokoro RKNN models missing (optional; sherpa CPU fallback still works) ==="
printf '  %s\n' "${missing[@]}"
[[ -d "${KOKORO_SERVER_DIR}/voices_npy" ]] || echo "  missing ${KOKORO_SERVER_DIR}/voices_npy/"
[[ -f "${KOKORO_SERVER_DIR}/Kokoro-82M/config.json" ]] \
  || echo "  missing ${KOKORO_SERVER_DIR}/Kokoro-82M/config.json"

if [[ "${CHECK_ONLY:-0}" == "1" ]]; then
  echo "Prepare: cd ${KOKORO_SERVER_DIR} && python3 build.py" >&2
  exit 1
fi

if [[ ! -f "${KOKORO_SERVER_DIR}/build.py" ]]; then
  echo "error: ${KOKORO_SERVER_DIR}/build.py not found" >&2
  exit 1
fi

if ! python3 -c "import torch, onnx" 2>/dev/null; then
  echo "error: python3 torch+onnx required to run build.py" >&2
  exit 1
fi

echo "=== running build.py in ${KOKORO_SERVER_DIR} ==="
(cd "${KOKORO_SERVER_DIR}" && python3 build.py)
echo "=== kokoro RKNN models ready under ${ONNX_DIR} ==="
