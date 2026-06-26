#!/usr/bin/env bash
# Verify Kokoro hybrid-v1 RK3588 artefacts (HF prebuilt, no build.py).
#
# Source: harvestsu/seeed-local-voice-rk-artifacts rk3588/kokoro-hybrid-v1
#   https://huggingface.co/harvestsu/seeed-local-voice-rk-artifacts/tree/main/rk3588/kokoro-hybrid-v1
#
# Usage:
#   bash scripts/talk/ensure_kokoro_rockchip.sh
#   CHECK_ONLY=1 bash scripts/talk/ensure_kokoro_rockchip.sh
set -euo pipefail

ROOT="${HERMES_ULTRA_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
MODELS_ROOT="${MODELS_ROOT:-${ROOT}/.models}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DEST="${MODELS_ROOT}/models/kokoro-hybrid-v1"

REQUIRED=(
  "kokoro-prefix-cpu.onnx"
  "kokoro-generator-tail-cpu.onnx"
  "kokoro-vocoder-tail-rest-cpu.onnx"
  "tokens.txt"
  "default.npy"
  "rk3588/kokoro-decoder-front.int8.rknn"
  "rk3588/kokoro-vocoder-front-half.native.fp16.rknn"
)

missing=()
for rel in "${REQUIRED[@]}"; do
  if [[ ! -f "${DEST}/${rel}" ]]; then
    missing+=("${rel}")
  fi
done

if [[ ${#missing[@]} -eq 0 ]]; then
  echo "=== kokoro hybrid-v1 RKNN models OK (${DEST}) ==="
  exit 0
fi

echo "=== kokoro hybrid-v1 models missing under ${DEST} ==="
printf '  %s\n' "${missing[@]}"

if [[ "${CHECK_ONLY:-0}" == "1" ]]; then
  echo "Run: make ensure-kokoro-rockchip" >&2
  exit 1
fi

echo "=== downloading kokoro-hybrid-v1 from HuggingFace ==="
ROCKCHIP_ONLY=1 HERMES_ULTRA_ROOT="${ROOT}" MODELS_ROOT="${MODELS_ROOT}" \
  bash "${SCRIPT_DIR}/download_models.sh"

missing=()
for rel in "${REQUIRED[@]}"; do
  if [[ ! -f "${DEST}/${rel}" ]]; then
    missing+=("${rel}")
  fi
done
if [[ ${#missing[@]} -eq 0 ]]; then
  echo "=== kokoro hybrid-v1 RKNN models OK (${DEST}) ==="
  exit 0
fi

echo "error: kokoro hybrid-v1 still incomplete after download" >&2
printf '  %s\n' "${missing[@]}" >&2
exit 1
