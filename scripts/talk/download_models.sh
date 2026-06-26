#!/usr/bin/env bash
# Download sherpa-onnx pretrained models for hermes-talk desktop (ASR/TTS/KWS/VAD/denoise/speaker).
#
# URLs follow https://k2-fsa.github.io/sherpa/onnx/index.html
#
# Installs into ${MODELS_ROOT}/models/ (default: repo-root .models/models/):
#   sensevoice/  — SenseVoice int8 ASR
#   kokoro/      — Kokoro multi-lang TTS v1.0
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
  tar xf "${archive}" -C "${TMP}"
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
  local encoder="encoder.rk3588.fp16-scaled.rknn"
  if [[ -f "${dest}/${encoder}" && -f "${dest}/tokens.txt" ]]; then
    echo "=== ${name}: already present ==="
    return 0
  fi
  echo "=== ${name} (SenseVoice RKNN for RK3588, via ${HF_BASE}) ==="
  mkdir -p "${dest}"
  local repo="harvestsu/sensevoice-rknn"
  fetch_hf "${repo}" "sense-voice-encoder.rk3588.fp16-scaled.rknn" "${dest}/${encoder}"
  fetch_hf "${repo}" "am.mvn" "${dest}/am.mvn"
  fetch_hf "${repo}" "embedding.npy" "${dest}/embedding.npy"
  fetch_hf "${repo}" "chn_jpn_yue_eng_ko_spectok.bpe.model" "${dest}/chn_jpn_yue_eng_ko_spectok.bpe.model"
  local tokens_archive="sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2"
  if [[ ! -f "${dest}/tokens.txt" ]]; then
    fetch_hf "${repo}" "${tokens_archive}" "${TMP}/${tokens_archive}"
    local extract="${TMP}/sensevoice-rk3588-tokens"
    rm -rf "${extract}"
    mkdir -p "${extract}"
    tar xf "${TMP}/${tokens_archive}" -C "${extract}"
    local inner
    inner="$(find "${extract}" -name tokens.txt | head -1)"
    if [[ -n "${inner}" ]]; then
      cp -f "${inner}" "${dest}/tokens.txt"
    else
      echo "warn: tokens.txt not found in ${tokens_archive}; trying k2-fsa fallback" >&2
      fetch "${SHERPA_BASE}/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2" \
        "${TMP}/sherpa-sensevoice-int8.tar.bz2"
      rm -rf "${extract}"
      mkdir -p "${extract}"
      tar xf "${TMP}/sherpa-sensevoice-int8.tar.bz2" -C "${extract}"
      inner="$(find "${extract}" -name tokens.txt | head -1)"
      [[ -n "${inner}" ]] && cp -f "${inner}" "${dest}/tokens.txt"
    fi
  fi
  if [[ ! -f "${dest}/tokens.txt" ]]; then
    echo "error: failed to obtain tokens.txt for ${name}" >&2
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
  if [[ -f "${dest}/model.onnx" && -f "${dest}/voices.bin" && -f "${dest}/tokens.txt" ]]; then
    echo "=== ${name}: already present ==="
    return 0
  fi
  echo "=== ${name} (Kokoro multi-lang v1.0, zh+en) ==="
  echo "    doc: https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/kokoro.html"
  local archive="kokoro-multi-lang-v1_0.tar.bz2"
  fetch "${SHERPA_BASE}/tts-models/${archive}" "${TMP}/${archive}"
  rm -rf "${dest}"
  extract_tarball "${TMP}/${archive}" "${dest}"
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
  for sub in sensevoice sensevoice-rk3588 kokoro zipvoice kws-zh-en vad denoise speaker; do
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

install_sensevoice
install_sensevoice_rk3588
install_kokoro
install_zipvoice
install_kws
install_vad
install_denoise
install_speaker

install_to_talk_home

echo
echo "=== Done ==="
echo "Models installed under ${DEST}"
echo "Packaging: make package-talk-* (MODELS_ROOT=${MODELS_ROOT})"
echo "Runtime:   copy/symlink into \$HERMES_HOME/hermes-talk/models/ or use bundled start script"
