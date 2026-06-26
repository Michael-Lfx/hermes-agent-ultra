#!/usr/bin/env bash
# Download sherpa-onnx pretrained models for hermes-talk desktop (ASR/TTS/KWS/VAD/denoise/speaker).
#
# URLs follow https://k2-fsa.github.io/sherpa/onnx/index.html
#
# Installs into ${MODELS_ROOT}/models/ (default: repo-root .models/models/):
#   sensevoice/  — SenseVoice int8 ASR
#   kokoro/      — Kokoro multi-lang TTS v1.1
#   zipvoice/    — ZipVoice zero-shot voice cloning (optional TTS)
#   kws-zh-en/   — Zipformer zh+en KWS (canonical encoder/decoder/joiner.onnx names)
#   vad/         — silero_vad.onnx
#   denoise/     — dpdfnet_baseline.onnx
#   speaker/     — 3dspeaker.onnx (zh+en campplus)
#
# Usage:
#   bash scripts/talk/download_models.sh
#   MODELS_ROOT=/path/to/.models bash scripts/talk/download_models.sh
#   (optional) export HTTPS_PROXY in the shell before running if downloads need a proxy
set -euo pipefail

ROOT="${HERMES_ULTRA_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
MODELS_ROOT="${MODELS_ROOT:-${ROOT}/.models}"
DEST="${MODELS_ROOT}/models"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

SHERPA_BASE="https://github.com/k2-fsa/sherpa-onnx/releases/download"
HF_BASE="${HF_ENDPOINT:-https://hf-mirror.com}"
HF_BASE="${HF_BASE%/}"
DOWNLOAD_PROXY="${HTTPS_PROXY:-${https_proxy:-${HTTP_PROXY:-${http_proxy:-}}}}"

# harvestsu HF encoder lacks sherpa custom_string metadata; k2-fsa model.rknn is ~459MB.
SENSEVOICE_RK3588_MODEL_MIN_BYTES=400000000
SENSEVOICE_RK3588_TARBALL="sherpa-onnx-rk3588-10-seconds-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2"

file_size_bytes() {
  wc -c <"$1" | tr -d ' '
}

verify_min_bytes() {
  local path="$1"
  local min="$2"
  [[ -f "${path}" ]] || return 1
  local size
  size="$(file_size_bytes "${path}")"
  [[ "${size}" -ge "${min}" ]]
}

fetch() {
  local url="$1"
  local out="$2"
  if [[ -f "${out}" ]]; then
    echo "  skip (cached): $(basename "${out}")"
    return 0
  fi
  echo "  GET ${url}"
  if [[ -n "${DOWNLOAD_PROXY}" ]]; then
    curl -fsSL --retry 3 --retry-delay 2 --proxy "${DOWNLOAD_PROXY}" -o "${out}" "${url}"
  else
    curl -fsSL --retry 3 --retry-delay 2 -o "${out}" "${url}"
  fi
}

fetch_min_bytes() {
  local url="$1"
  local out="$2"
  local min="$3"
  if verify_min_bytes "${out}" "${min}"; then
    echo "  skip (cached, $(file_size_bytes "${out}") bytes): $(basename "${out}")"
    return 0
  fi
  if [[ -f "${out}" ]]; then
    local size
    size="$(file_size_bytes "${out}")"
    echo "  warn: ${out} is truncated (${size} bytes, need >= ${min}); re-downloading" >&2
    rm -f "${out}"
  fi
  fetch "${url}" "${out}"
  if ! verify_min_bytes "${out}" "${min}"; then
    local size
    size="$(file_size_bytes "${out}" 2>/dev/null || echo 0)"
    echo "error: ${out} download incomplete (${size} bytes, need >= ${min})" >&2
    echo "hint: retry with HTTPS_PROXY set, or: HF_ENDPOINT=https://huggingface.co bash scripts/talk/download_models.sh" >&2
    exit 1
  fi
}

fetch_hf() {
  local repo="$1"
  local file="$2"
  local out="$3"
  fetch "${HF_BASE}/${repo}/resolve/main/${file}" "${out}"
}

extract_tarball() {
  local archive="$1"
  local dest="$2"
  mkdir -p "${dest}"
  tar xf "${archive}" -C "${TMP}" --no-same-owner
  local inner
  inner="$(find "${TMP}" -mindepth 1 -maxdepth 1 -type d | head -1)"
  if [[ -z "${inner}" ]]; then
    echo "extract failed: no top-level dir in ${archive}" >&2
    exit 1
  fi
  cp -a "${inner}/." "${dest}/"
}

install_sensevoice_rk3588() {
  local name="sensevoice-rk3588"
  local dest="${DEST}/${name}"
  local model="model.rknn"
  if verify_min_bytes "${dest}/${model}" "${SENSEVOICE_RK3588_MODEL_MIN_BYTES}" \
    && [[ -f "${dest}/tokens.txt" ]]; then
    echo "=== ${name}: already present ==="
    return 0
  fi
  echo "=== ${name} (SenseVoice RKNN for RK3588, k2-fsa ${SENSEVOICE_RK3588_TARBALL}) ==="
  mkdir -p "${dest}"
  local archive="${TMP}/${SENSEVOICE_RK3588_TARBALL}"
  fetch "${SHERPA_BASE}/asr-models/${SENSEVOICE_RK3588_TARBALL}" "${archive}"
  local extract="${TMP}/sensevoice-rk3588-extract"
  rm -rf "${extract}"
  mkdir -p "${extract}"
  tar xjf "${archive}" -C "${extract}" --no-same-owner
  local inner
  inner="$(find "${extract}" -name model.rknn | head -1)"
  if [[ -z "${inner}" || ! -f "${inner%/*}/tokens.txt" ]]; then
    echo "error: ${SENSEVOICE_RK3588_TARBALL} missing model.rknn or tokens.txt" >&2
    exit 1
  fi
  cp -f "${inner}" "${dest}/${model}"
  cp -f "${inner%/*}/tokens.txt" "${dest}/tokens.txt"
  if ! verify_min_bytes "${dest}/${model}" "${SENSEVOICE_RK3588_MODEL_MIN_BYTES}"; then
    local size
    size="$(file_size_bytes "${dest}/${model}" 2>/dev/null || echo 0)"
    echo "error: ${dest}/${model} looks truncated (${size} bytes)" >&2
    exit 1
  fi
  echo "  -> ${dest}"
}

install_sensevoice() {
  local name="sensevoice"
  local dest="${DEST}/${name}"
  if [[ -f "${dest}/model.int8.onnx" && -f "${dest}/tokens.txt" ]]; then
    echo "=== ${name}: already present ==="
    return 0
  fi
  echo "=== ${name} (SenseVoice int8, zh/en/ja/ko/yue) ==="
  echo "    doc: https://k2-fsa.github.io/sherpa/onnx/sense-voice/pretrained.html"
  local archive="sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2"
  fetch "${SHERPA_BASE}/asr-models/${archive}" "${TMP}/${archive}"
  rm -rf "${dest}"
  extract_tarball "${TMP}/${archive}" "${dest}"
  echo "  -> ${dest}"
}

install_kokoro() {
  local name="kokoro"
  local dest="${DEST}/${name}"
  local archive="kokoro-multi-lang-v1_1.tar.bz2"
  if [[ -f "${dest}/model.onnx" && -f "${dest}/voices.bin" && -f "${dest}/tokens.txt" ]]; then
    echo "=== ${name}: already present ==="
    return 0
  fi
  echo "=== ${name} (Kokoro multi-lang v1_1, zh+en, 103 speakers) ==="
  echo "    doc: https://k2-fsa.github.io/sherpa/onnx/tts/all/Chinese-English/kokoro-multi-lang-v1_1.html"
  local archive_path=""
  if [[ -f "${dest}/${archive}" ]]; then
    archive_path="${dest}/${archive}"
    echo "  use local: ${archive}"
  else
    archive_path="${TMP}/${archive}"
    fetch "${SHERPA_BASE}/tts-models/${archive}" "${archive_path}"
  fi
  rm -rf "${dest}"
  extract_tarball "${archive_path}" "${dest}"
  echo "  -> ${dest}"
}

install_kokoro_hybrid_v1() {
  local name="kokoro-hybrid-v1"
  local dest="${DEST}/${name}"
  local repo="harvestsu/seeed-local-voice-rk-artifacts"
  local base="rk3588/kokoro-hybrid-v1"
  local marker="${dest}/rk3588/kokoro-decoder-front.int8.rknn"
  if [[ -f "${marker}" \
    && -f "${dest}/kokoro-prefix-cpu.onnx" \
    && -f "${dest}/kokoro-vocoder-tail-rest-cpu.onnx" \
    && -f "${dest}/tokens.txt" \
    && -f "${dest}/default.npy" ]]; then
    echo "=== ${name}: already present ==="
    return 0
  fi
  echo "=== ${name} (Kokoro hybrid v1 RK3588 NPU TTS, via ${HF_BASE}) ==="
  echo "    ref: https://huggingface.co/${repo}/tree/main/${base}"
  mkdir -p "${dest}/rk3588"
  local files=(
    "${base}/kokoro-prefix-cpu.onnx"
    "${base}/kokoro-generator-tail-cpu.onnx"
    "${base}/kokoro-vocoder-tail-rest-cpu.onnx"
    "${base}/tokens.txt"
    "${base}/default.npy"
    "${base}/style.npy"
    "${base}/rk3588/kokoro-decoder-front.int8.rknn"
    "${base}/rk3588/kokoro-vocoder-front-half.native.fp16.rknn"
  )
  local rel out_dir
  for rel in "${files[@]}"; do
    out_dir="${dest}/$(dirname "${rel#${base}/}")"
    mkdir -p "${out_dir}"
    fetch_hf "${repo}" "${rel}" "${dest}/${rel#${base}/}"
  done
  echo "  -> ${dest}"
}

install_zipvoice() {
  local name="zipvoice"
  local dest="${DEST}/${name}"
  if [[ -f "${dest}/encoder.int8.onnx" && -f "${dest}/decoder.int8.onnx" && -f "${dest}/vocos_24khz.onnx" ]]; then
    echo "=== ${name}: already present ==="
    return 0
  fi
  echo "=== ${name} (ZipVoice distill int8 zh+en) ==="
  echo "    doc: https://k2-fsa.github.io/sherpa/onnx/tts/zipvoice.html"
  local archive="sherpa-onnx-zipvoice-distill-int8-zh-en-emilia.tar.bz2"
  fetch "${SHERPA_BASE}/tts-models/${archive}" "${TMP}/${archive}"
  rm -rf "${dest}"
  extract_tarball "${TMP}/${archive}" "${dest}"
  fetch "${SHERPA_BASE}/vocoder-models/vocos_24khz.onnx" "${dest}/vocos_24khz.onnx"
  echo "  -> ${dest}"
}

install_kws() {
  local name="kws-zh-en"
  local dest="${DEST}/${name}"
  if [[ -f "${dest}/encoder.onnx" && -f "${dest}/decoder.onnx" && -f "${dest}/joiner.onnx" ]]; then
    echo "=== ${name}: already present ==="
    return 0
  fi
  echo "=== ${name} (Zipformer zh+en KWS) ==="
  echo "    doc: https://k2-fsa.github.io/sherpa/onnx/kws/pretrained_models/index.html"
  local archive="sherpa-onnx-kws-zipformer-zh-en-3M-2025-12-20.tar.bz2"
  fetch "${SHERPA_BASE}/kws-models/${archive}" "${TMP}/${archive}"
  local extract="${TMP}/kws-src"
  rm -rf "${extract}" "${dest}"
  mkdir -p "${extract}"
  tar xf "${TMP}/${archive}" -C "${extract}"
  local inner
  inner="$(find "${extract}" -mindepth 1 -maxdepth 1 -type d | head -1)"
  mkdir -p "${dest}"
  cp -f "${inner}/tokens.txt" "${dest}/"
  cp -f "${inner}/en.phone" "${dest}/"
  # chunk-16: 320ms latency; int8 encoder/joiner + fp32 decoder (per sherpa docs)
  cp -f "${inner}/encoder-epoch-13-avg-2-chunk-16-left-64.int8.onnx" "${dest}/encoder.onnx"
  cp -f "${inner}/decoder-epoch-13-avg-2-chunk-16-left-64.onnx" "${dest}/decoder.onnx"
  cp -f "${inner}/joiner-epoch-13-avg-2-chunk-16-left-64.int8.onnx" "${dest}/joiner.onnx"
  echo "  -> ${dest}"
}

install_vad() {
  local name="vad"
  local dest="${DEST}/${name}"
  local out="${dest}/silero_vad.onnx"
  if [[ -f "${out}" ]]; then
    echo "=== ${name}: already present ==="
    return 0
  fi
  echo "=== ${name} (Silero VAD, k2-fsa export) ==="
  echo "    doc: https://k2-fsa.github.io/sherpa/onnx/vad/silero-vad.html"
  mkdir -p "${dest}"
  fetch "${SHERPA_BASE}/asr-models/silero_vad.onnx" "${out}"
  echo "  -> ${out}"
}

install_denoise() {
  local name="denoise"
  local dest="${DEST}/${name}"
  local out="${dest}/dpdfnet_baseline.onnx"
  if [[ -f "${out}" ]]; then
    echo "=== ${name}: already present ==="
    return 0
  fi
  echo "=== ${name} (DPDFNet baseline, 16 kHz) ==="
  echo "    doc: https://k2-fsa.github.io/sherpa/onnx/speech-enhancement/dpdfnet.html"
  mkdir -p "${dest}"
  fetch "${SHERPA_BASE}/speech-enhancement-models/dpdfnet_baseline.onnx" "${out}"
  echo "  -> ${out}"
}

install_speaker() {
  local name="speaker"
  local dest="${DEST}/${name}"
  local out="${dest}/3dspeaker.onnx"
  if [[ -f "${out}" ]]; then
    echo "=== ${name}: already present ==="
    return 0
  fi
  echo "=== ${name} (3D-Speaker campplus zh+en) ==="
  echo "    doc: https://k2-fsa.github.io/sherpa/onnx/speaker-identification/index.html"
  mkdir -p "${dest}"
  local src="3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx"
  fetch "${SHERPA_BASE}/speaker-recongition-models/${src}" "${TMP}/${src}"
  cp -f "${TMP}/${src}" "${out}"
  echo "  -> ${out}"
}

install_to_talk_home() {
  if [[ "${SKIP_TALK_INSTALL:-0}" == "1" ]]; then
    return 0
  fi
  local talk_home
  if [[ -n "${HERMES_HOME:-}" ]]; then
    talk_home="${HERMES_HOME}/hermes-talk/models"
  elif [[ -n "${USERPROFILE:-}" ]]; then
    talk_home="${USERPROFILE}/.hermes-agent-ultra/hermes-talk/models"
  elif [[ -n "${HOME:-}" ]]; then
    talk_home="${HOME}/.hermes-agent-ultra/hermes-talk/models"
  else
    return 0
  fi
  echo "=== install to talk home: ${talk_home} ==="
  for sub in sensevoice sensevoice-rk3588 kokoro kokoro-hybrid-v1 zipvoice kws-zh-en vad denoise speaker; do
    if [[ -d "${DEST}/${sub}" ]]; then
      mkdir -p "${talk_home}/${sub}"
      cp -a "${DEST}/${sub}/." "${talk_home}/${sub}/"
    fi
  done
  echo "  -> ${talk_home}"
}

echo "=== hermes-talk model download ==="
echo "MODELS_ROOT=${MODELS_ROOT}"
echo "DEST=${DEST}"
if [[ -n "${DOWNLOAD_PROXY}" ]]; then
  echo "HTTPS_PROXY=${DOWNLOAD_PROXY}"
fi
echo

mkdir -p "${DEST}"

if [[ "${ROCKCHIP_ONLY:-0}" == "1" ]]; then
  install_sensevoice_rk3588
  install_kokoro
  install_kokoro_hybrid_v1
  install_kws
  install_vad
  install_denoise
  install_speaker
else
  install_sensevoice
  install_sensevoice_rk3588
  install_kokoro
  install_kokoro_hybrid_v1
  install_zipvoice
  install_kws
  install_vad
  install_denoise
  install_speaker
fi

install_to_talk_home

echo
echo "=== Done ==="
echo "Models installed under ${DEST}"
echo "Packaging: make package-talk-* (MODELS_ROOT=${MODELS_ROOT})"
echo "Runtime:   copy/symlink into \$HERMES_HOME/hermes-talk/models/ or use bundled start script"
