#!/usr/bin/env bash
# Package hermes-agent-ultra (talk-rockchip) for aarch64 Rockchip boards.
#
# Bundled stack:
#   ASR: SenseVoice RKNN (NPU)
#   TTS: Kokoro RKNN in-process (optional) + sherpa kokoro-multi-lang-v1_1 CPU fallback
#
# Prepare under repo-root `.models/`:
#   models/sensevoice-rk3588/
#   models/kokoro/  — sherpa v1_1 (fallback) + optional RKNN split models
#   models/kws-zh-en/ vad/ denoise/ speaker/
#
# Optional RKNN TTS from KOKORO_SERVER_DIR or .models/models/kokoro/:
#   kokoro_encoder.onnx, har_generator.onnx, kokoro_decoder.rknn,
#   config.json, voices_npy/, misaki-data/, espeak-ng-data/
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIST="${DIST_DIR:-${ROOT}/target/dist}"
BIN="${BIN_PATH:-${ROOT}/target/aarch64-unknown-linux-gnu/release/hermes-agent-ultra}"
GCC="${ROOT}/.cross-cache/gcc-aarch64/aarch64-none-linux-gnu"
OUT="${DIST}/${OUT_NAME:-hermes-talk-rk3588}"
MODELS_ROOT="${MODELS_ROOT:-${ROOT}/.models}"

KOKORO_SERVER_DIR="${KOKORO_SERVER_DIR:-/home/leeyang/kokoro-server}"
KOKORO_CACHE="${KOKORO_CACHE:-${ROOT}/.cross-cache/kokoro-server}"
RK_TTS_SDK_DIR="${RK_TTS_SDK_DIR:-/home/leeyang/Rockchip_RKTTS_SDK_Release}"
RK_NPU_LIB_DIR="${RK_NPU_LIB_DIR:-${RK_TTS_SDK_DIR}/lib/Linux/aarch64}"

fail() {
  echo "error: $*" >&2
  exit 1
}

if [[ ! -f "${BIN}" ]]; then
  fail "missing ${BIN}; run: make release-talk-rockchip-arm64 (or package-talk-rockchip-dev)"
fi

rm -rf "${OUT}"
mkdir -p "${OUT}/bin" "${OUT}/lib" "${OUT}/models"

cp -f "${BIN}" "${OUT}/bin/hermes-agent-ultra"
chmod +x "${OUT}/bin/hermes-agent-ultra"

cp -a "${GCC}/libc/lib/ld-linux-aarch64.so.1" "${OUT}/lib/"

for lib in libc.so.6 libm.so.6 libpthread.so.0 libdl.so.2 librt.so.1 \
           libutil.so.1 libresolv.so.2 libnss_files.so.2 libnss_dns.so.2; do
  cp -a "${GCC}/libc/lib64/${lib}" "${OUT}/lib/"
done

cp -a "${GCC}/lib64/libstdc++.so.6.0.32" "${OUT}/lib/"
ln -sf libstdc++.so.6.0.32 "${OUT}/lib/libstdc++.so.6"
cp -a "${GCC}/lib64/libgcc_s.so.1" "${OUT}/lib/"

if [[ -f "${RK_NPU_LIB_DIR}/librknnrt.so" ]]; then
  cp "${RK_NPU_LIB_DIR}/librknnrt.so" "${OUT}/lib/"
else
  echo "warn: missing ${RK_NPU_LIB_DIR}/librknnrt.so (ASR/TTS NPU runtime)" >&2
fi

# Sherpa kokoro-multi-lang-v1_1 CPU fallback (required)
[[ -d "${MODELS_ROOT}/models/kokoro" ]] \
  || fail "missing ${MODELS_ROOT}/models/kokoro; run: make ensure-talk-models-rockchip"
for f in model.onnx voices.bin tokens.txt lexicon-us-en.txt lexicon-zh.txt; do
  [[ -f "${MODELS_ROOT}/models/kokoro/${f}" ]] \
    || fail "missing ${MODELS_ROOT}/models/kokoro/${f}; run: make ensure-talk-models-rockchip"
done
[[ -d "${MODELS_ROOT}/models/kokoro/espeak-ng-data" ]] \
  || fail "missing ${MODELS_ROOT}/models/kokoro/espeak-ng-data; run: make ensure-talk-models-rockchip"
mkdir -p "${OUT}/models/kokoro"
cp -a "${MODELS_ROOT}/models/kokoro/." "${OUT}/models/kokoro/"

# Optional Kokoro RKNN split models + G2P aux
resolve_kokoro_onnx_dir() {
  if [[ -d "${KOKORO_SERVER_DIR}/onnx" ]] \
    && [[ -f "${KOKORO_SERVER_DIR}/onnx/kokoro_decoder.rknn" ]]; then
    echo "${KOKORO_SERVER_DIR}/onnx"
  elif [[ -d "${KOKORO_CACHE}/onnx" ]] \
    && [[ -f "${KOKORO_CACHE}/onnx/kokoro_decoder.rknn" ]]; then
    echo "${KOKORO_CACHE}/onnx"
  elif [[ -f "${OUT}/models/kokoro/kokoro_decoder.rknn" ]]; then
    echo "${OUT}/models/kokoro"
  else
    return 1
  fi
}

if KOKORO_ONNX="$(resolve_kokoro_onnx_dir)"; then
  for f in kokoro_encoder.onnx har_generator.onnx kokoro_decoder.rknn; do
    if [[ -f "${KOKORO_ONNX}/${f}" ]]; then
      cp -f "${KOKORO_ONNX}/${f}" "${OUT}/models/kokoro/"
    fi
  done
  KOKORO_CONFIG="${KOKORO_SERVER_DIR}/Kokoro-82M/config.json"
  if [[ ! -f "${KOKORO_CONFIG}" && -f "${KOKORO_CACHE}/Kokoro-82M/config.json" ]]; then
    KOKORO_CONFIG="${KOKORO_CACHE}/Kokoro-82M/config.json"
  elif [[ ! -f "${KOKORO_CONFIG}" && -f "${OUT}/models/kokoro/config.json" ]]; then
    KOKORO_CONFIG="${OUT}/models/kokoro/config.json"
  fi
  if [[ -f "${KOKORO_CONFIG}" ]]; then
    cp -f "${KOKORO_CONFIG}" "${OUT}/models/kokoro/config.json"
  fi
  VOICES_SRC="${KOKORO_SERVER_DIR}/voices_npy"
  if [[ ! -d "${VOICES_SRC}" && -d "${KOKORO_CACHE}/voices_npy" ]]; then
    VOICES_SRC="${KOKORO_CACHE}/voices_npy"
  elif [[ ! -d "${VOICES_SRC}" && -d "${OUT}/models/kokoro/voices_npy" ]]; then
    VOICES_SRC="${OUT}/models/kokoro/voices_npy"
  fi
  if [[ -d "${VOICES_SRC}" ]]; then
    mkdir -p "${OUT}/models/kokoro/voices_npy"
    cp -a "${VOICES_SRC}/." "${OUT}/models/kokoro/voices_npy/"
  fi
  for aux in misaki-data espeak-ng-data; do
    if [[ -d "${KOKORO_SERVER_DIR}/${aux}" ]]; then
      cp -a "${KOKORO_SERVER_DIR}/${aux}" "${OUT}/${aux}"
    elif [[ -d "${KOKORO_SERVER_DIR}/build/${aux}" ]]; then
      cp -a "${KOKORO_SERVER_DIR}/build/${aux}" "${OUT}/${aux}"
    elif [[ -d "${KOKORO_CACHE}/${aux}" ]]; then
      cp -a "${KOKORO_CACHE}/${aux}" "${OUT}/${aux}"
    fi
  done
  echo "Bundled Kokoro RKNN split models from ${KOKORO_ONNX}"
else
  echo "warn: missing Kokoro RKNN split models; board will use sherpa CPU kokoro fallback" >&2
fi

# SenseVoice RKNN ASR (required)
[[ -d "${MODELS_ROOT}/models/sensevoice-rk3588" ]] \
  || fail "missing ${MODELS_ROOT}/models/sensevoice-rk3588; run: make ensure-talk-models-rockchip"
mkdir -p "${OUT}/models/sensevoice-rk3588"
cp -a "${MODELS_ROOT}/models/sensevoice-rk3588/." "${OUT}/models/sensevoice-rk3588/"

# Sherpa ONNX aux (wake / vad / denoise / speaker)
if [[ -d "${MODELS_ROOT}/models/kws-zh-en" ]]; then
  mkdir -p "${OUT}/models/kws-zh-en"
  cp -a "${MODELS_ROOT}/models/kws-zh-en/." "${OUT}/models/kws-zh-en/"
fi
for sub in vad denoise speaker; do
  if [[ -d "${MODELS_ROOT}/models/${sub}" ]]; then
    mkdir -p "${OUT}/models/${sub}"
    cp -a "${MODELS_ROOT}/models/${sub}/." "${OUT}/models/${sub}/"
  fi
done

cp "${ROOT}/scripts/talk/config.example.rockchip.toml" "${OUT}/config.example.toml"
cp "${ROOT}/scripts/talk/config.example.rockchip.yaml" "${OUT}/config.example.yaml"
cp "${ROOT}/scripts/talk/start_board.sh" "${OUT}/start.sh"
chmod +x "${OUT}/start.sh"

echo "Bundled: ${OUT}"
echo "On board: cd ${OUT} && ./start.sh"
