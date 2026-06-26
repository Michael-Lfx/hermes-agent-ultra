#!/usr/bin/env bash
# Verify Kokoro RKNN split-model artefacts (optional; copy prebuilt, no build.py).
#
# Searches:
#   ${KOKORO_SERVER_DIR}/onnx/
#   ${KOKORO_CACHE}/onnx/
#   ${MODELS_ROOT}/models/kokoro/
#
# Usage:
#   bash scripts/talk/ensure_kokoro_rockchip.sh
#   CHECK_ONLY=1 bash scripts/talk/ensure_kokoro_rockchip.sh
set -euo pipefail

ROOT="${HERMES_ULTRA_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
MODELS_ROOT="${MODELS_ROOT:-${ROOT}/.models}"
KOKORO_SERVER_DIR="${KOKORO_SERVER_DIR:-/home/leeyang/kokoro-server}"
KOKORO_CACHE="${KOKORO_CACHE:-${ROOT}/.cross-cache/kokoro-server}"

RKNN_FILES=(kokoro_encoder.onnx har_generator.onnx kokoro_decoder.rknn)

resolve_kokoro_rknn_root() {
  local base onnx_dir
  for base in "${KOKORO_SERVER_DIR}" "${KOKORO_CACHE}" "${MODELS_ROOT}/models/kokoro"; do
    onnx_dir="${base}/onnx"
    if [[ -f "${onnx_dir}/kokoro_decoder.rknn" ]]; then
      echo "${base}"
      return 0
    fi
    if [[ -f "${base}/kokoro_decoder.rknn" ]]; then
      echo "${base}"
      return 0
    fi
  done
  return 1
}

resolve_kokoro_aux() {
  local kind="$1"
  case "${kind}" in
    voices)
      for d in \
        "${KOKORO_SERVER_DIR}/voices_npy" \
        "${KOKORO_CACHE}/voices_npy" \
        "${MODELS_ROOT}/models/kokoro/voices_npy"; do
        [[ -d "${d}" ]] && echo "${d}" && return 0
      done
      ;;
    config)
      for f in \
        "${KOKORO_SERVER_DIR}/Kokoro-82M/config.json" \
        "${KOKORO_CACHE}/Kokoro-82M/config.json" \
        "${MODELS_ROOT}/models/kokoro/config.json"; do
        [[ -f "${f}" ]] && echo "${f}" && return 0
      done
      ;;
    misaki)
      for d in \
        "${KOKORO_SERVER_DIR}/misaki-data" \
        "${KOKORO_SERVER_DIR}/build/misaki-data" \
        "${KOKORO_CACHE}/misaki-data"; do
        [[ -d "${d}" ]] && echo "${d}" && return 0
      done
      ;;
    espeak)
      for d in \
        "${KOKORO_SERVER_DIR}/espeak-ng-data" \
        "${KOKORO_SERVER_DIR}/build/espeak-ng-data" \
        "${KOKORO_CACHE}/espeak-ng-data"; do
        [[ -d "${d}" ]] && echo "${d}" && return 0
      done
      ;;
  esac
  return 1
}

if root="$(resolve_kokoro_rknn_root)"; then
  voices="$(resolve_kokoro_aux voices || true)"
  config="$(resolve_kokoro_aux config || true)"
  misaki="$(resolve_kokoro_aux misaki || true)"
  espeak="$(resolve_kokoro_aux espeak || true)"
  if [[ -n "${voices}" && -n "${config}" && -n "${misaki}" && -n "${espeak}" ]]; then
    echo "=== kokoro RKNN models OK (${root}) ==="
    exit 0
  fi
  echo "=== kokoro RKNN split models found under ${root}, but aux data incomplete ==="
  [[ -z "${voices}" ]] && echo "  missing voices_npy/"
  [[ -z "${config}" ]] && echo "  missing config.json"
  [[ -z "${misaki}" ]] && echo "  missing misaki-data/"
  [[ -z "${espeak}" ]] && echo "  missing espeak-ng-data/"
else
  echo "=== kokoro RKNN models missing (optional; sherpa CPU fallback still works) ==="
  for f in "${RKNN_FILES[@]}"; do
    echo "  missing */onnx/${f} (or models/kokoro/${f})"
  done
fi

if [[ "${CHECK_ONLY:-0}" == "1" ]]; then
  echo "Copy prebuilt RKNN artefacts into .models/models/kokoro/ or set KOKORO_SERVER_DIR" >&2
  echo "  need: kokoro_encoder.onnx, har_generator.onnx, kokoro_decoder.rknn," >&2
  echo "        config.json, voices_npy/, misaki-data/, espeak-ng-data/" >&2
  exit 1
fi

echo "warn: kokoro RKNN TTS unavailable until split models are installed; will use sherpa CPU fallback" >&2
exit 0
