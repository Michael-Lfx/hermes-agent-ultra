#!/usr/bin/env bash
# Verify RK3588 talk bundle models under ${MODELS_ROOT}/models/; download if missing.
#
# TTS: sherpa-onnx kokoro-multi-lang-v1_1 (prebuilt tarball, no local build.py)
#   https://k2-fsa.github.io/sherpa/onnx/tts/all/Chinese-English/kokoro-multi-lang-v1_1.html
#
# Usage:
#   bash scripts/talk/ensure_models_rockchip.sh
#   CHECK_ONLY=1 bash scripts/talk/ensure_models_rockchip.sh
set -euo pipefail

ROOT="${HERMES_ULTRA_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
MODELS_ROOT="${MODELS_ROOT:-${ROOT}/.models}"
DEST="${MODELS_ROOT}/models"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Must match download_models.sh (truncated HF LFS downloads fail rknn_init on board).
SENSEVOICE_RK3588_ENCODER_MIN_BYTES=400000000

REQUIRED=(
  "sensevoice-rk3588/encoder.rk3588.fp16-scaled.rknn"
  "sensevoice-rk3588/tokens.txt"
  "kokoro/model.onnx"
  "kokoro/voices.bin"
  "kokoro/tokens.txt"
  "kokoro/lexicon-us-en.txt"
  "kokoro/lexicon-zh.txt"
  "kws-zh-en/encoder.onnx"
  "kws-zh-en/decoder.onnx"
  "kws-zh-en/joiner.onnx"
  "kws-zh-en/tokens.txt"
  "vad/silero_vad.onnx"
  "denoise/dpdfnet_baseline.onnx"
  "speaker/3dspeaker.onnx"
)

REQUIRED_DIRS=(
  "kokoro/espeak-ng-data"
)

missing=()
for rel in "${REQUIRED[@]}"; do
  if [[ ! -f "${DEST}/${rel}" ]]; then
    missing+=("${rel}")
  fi
done
encoder="${DEST}/sensevoice-rk3588/encoder.rk3588.fp16-scaled.rknn"
if [[ -f "${encoder}" ]]; then
  size="$(wc -c <"${encoder}" | tr -d ' ')"
  if [[ "${size}" -lt "${SENSEVOICE_RK3588_ENCODER_MIN_BYTES}" ]]; then
    echo "warn: ${encoder} looks truncated (${size} bytes, need >= ${SENSEVOICE_RK3588_ENCODER_MIN_BYTES})" >&2
    missing+=("sensevoice-rk3588/encoder.rk3588.fp16-scaled.rknn (re-download)")
    rm -f "${encoder}"
  fi
fi
for rel in "${REQUIRED_DIRS[@]}"; do
  if [[ ! -d "${DEST}/${rel}" ]]; then
    missing+=("${rel}/")
  fi
done

if [[ ${#missing[@]} -eq 0 ]]; then
  echo "=== rockchip talk models OK (${DEST}) ==="
  exit 0
fi

echo "=== rockchip talk models missing under ${DEST} ==="
printf '  %s\n' "${missing[@]}"
if [[ "${CHECK_ONLY:-0}" == "1" ]]; then
  echo "Run: make ensure-talk-models-rockchip" >&2
  exit 1
fi

echo "=== downloading kokoro-multi-lang-v1_1 + aux (HTTPS_PROXY=${HTTPS_PROXY:-${https_proxy:-${HTTP_PROXY:-${http_proxy:-unset}}}}) ==="
ROCKCHIP_ONLY=1 HERMES_ULTRA_ROOT="${ROOT}" MODELS_ROOT="${MODELS_ROOT}" \
  bash "${SCRIPT_DIR}/download_models.sh"
