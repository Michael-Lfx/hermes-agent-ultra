#!/usr/bin/env bash
# Package hermes-agent-ultra (talk-rockchip) for aarch64 Rockchip boards.
#
# Bundled stack: SenseVoice RKNN ASR (in-process) + Kokoro TTS (kokoro-server sidecar).
#
# Prepare under repo-root `.models/` (gitignored) or source trees:
#   .models/models/sensevoice-rk3588/
#   .models/models/kws-zh-en/ vad/ denoise/ speaker/
#   KOKORO_SERVER_DIR/build/kokoro-server + onnx/ voices_npy/ Kokoro-82M/config.json
#
# Env:
#   KOKORO_SERVER_DIR  default: /home/leeyang/kokoro-server
#   RK_NPU_LIB_DIR     default: RK_TTS_SDK/lib/Linux/aarch64 (for librknnrt.so)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIST="${DIST_DIR:-${ROOT}/target/dist}"
BIN="${BIN_PATH:-${ROOT}/target/aarch64-unknown-linux-gnu/release/hermes-agent-ultra}"
GCC="${ROOT}/.cross-cache/gcc-aarch64/aarch64-none-linux-gnu"
OUT="${DIST}/${OUT_NAME:-hermes-talk-rk3588}"
MODELS_ROOT="${MODELS_ROOT:-${ROOT}/.models}"

KOKORO_SERVER_DIR="${KOKORO_SERVER_DIR:-/home/leeyang/kokoro-server}"
RK_TTS_SDK_DIR="${RK_TTS_SDK_DIR:-/home/leeyang/Rockchip_RKTTS_SDK_Release}"
RK_NPU_LIB_DIR="${RK_NPU_LIB_DIR:-${RK_TTS_SDK_DIR}/lib/Linux/aarch64}"

if [[ ! -f "${BIN}" ]]; then
  echo "missing ${BIN}; run: make release-talk-rockchip-arm64" >&2
  exit 1
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

# kokoro-server binary
KOKORO_BIN="${KOKORO_SERVER_DIR}/build/kokoro-server"
if [[ -x "${KOKORO_BIN}" ]]; then
  cp -f "${KOKORO_BIN}" "${OUT}/bin/kokoro-server"
  chmod +x "${OUT}/bin/kokoro-server"
else
  echo "warn: missing ${KOKORO_BIN}; build kokoro-server with -DUSE_RKNN=ON" >&2
fi

# Kokoro models (encoder/har ONNX + decoder RKNN + voices)
KOKORO_OUT="${OUT}/models/kokoro"
mkdir -p "${KOKORO_OUT}"
for f in kokoro_encoder.onnx har_generator.onnx kokoro_decoder.rknn; do
  if [[ -f "${KOKORO_SERVER_DIR}/onnx/${f}" ]]; then
    cp -f "${KOKORO_SERVER_DIR}/onnx/${f}" "${KOKORO_OUT}/"
  else
    echo "warn: missing ${KOKORO_SERVER_DIR}/onnx/${f}" >&2
  fi
done
if [[ -f "${KOKORO_SERVER_DIR}/Kokoro-82M/config.json" ]]; then
  cp -f "${KOKORO_SERVER_DIR}/Kokoro-82M/config.json" "${KOKORO_OUT}/config.json"
fi
if [[ -d "${KOKORO_SERVER_DIR}/voices_npy" ]]; then
  mkdir -p "${KOKORO_OUT}/voices_npy"
  cp -a "${KOKORO_SERVER_DIR}/voices_npy/." "${KOKORO_OUT}/voices_npy/"
fi
for aux in misaki-data espeak-ng-data; do
  if [[ -d "${KOKORO_SERVER_DIR}/${aux}" ]]; then
    cp -a "${KOKORO_SERVER_DIR}/${aux}" "${OUT}/${aux}"
  elif [[ -d "${KOKORO_SERVER_DIR}/build/${aux}" ]]; then
    cp -a "${KOKORO_SERVER_DIR}/build/${aux}" "${OUT}/${aux}"
  fi
done

# SenseVoice RKNN ASR
if [[ -d "${MODELS_ROOT}/models/sensevoice-rk3588" ]]; then
  mkdir -p "${OUT}/models/sensevoice-rk3588"
  cp -a "${MODELS_ROOT}/models/sensevoice-rk3588/." "${OUT}/models/sensevoice-rk3588/"
else
  echo "warn: missing ${MODELS_ROOT}/models/sensevoice-rk3588" >&2
fi

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
