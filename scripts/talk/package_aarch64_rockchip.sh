#!/usr/bin/env bash
# Package hermes-agent-ultra (talk-rockchip) for aarch64 Rockchip boards.
#
# Bundled stack:
#   ASR: SenseVoice RKNN (NPU)
#   TTS: Kokoro hybrid-v1 RKNN (NPU) + sherpa kokoro-multi-lang-v1_1 CPU fallback
#
# Prepare under repo-root `.models/`:
#   models/sensevoice-rk3588/
#   models/kokoro/           — sherpa v1_1 CPU fallback
#   models/kokoro-hybrid-v1/ — HF hybrid v1 NPU TTS
#   models/kws-zh-en/ vad/ denoise/ speaker/
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIST="${DIST_DIR:-${ROOT}/target/dist}"
BIN="${BIN_PATH:-${ROOT}/target/aarch64-unknown-linux-gnu/release/hermes-agent-ultra}"
GCC="${ROOT}/.cross-cache/gcc-aarch64/aarch64-none-linux-gnu"
OUT="${DIST}/${OUT_NAME:-hermes-talk-rk3588}"
MODELS_ROOT="${MODELS_ROOT:-${ROOT}/.models}"

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

# sherpa-onnx RKNN shared runtime (linked when SHERPA_ONNX_PACK=rknn)
BIN_DIR="$(dirname "${BIN}")"
shopt -s nullglob
for so in "${BIN_DIR}"/*.so; do
  cp -f "${so}" "${OUT}/lib/"
done
shopt -u nullglob
SHERPA_RKNN_SHARED="${ROOT}/.cross-cache/sherpa-onnx/sherpa-onnx-v1.13.3-rknn-linux-aarch64-shared.tar.bz2"
if [[ ! -f "${OUT}/lib/libsherpa-onnx-c-api.so" && -f "${SHERPA_RKNN_SHARED}" ]]; then
  tar xjf "${SHERPA_RKNN_SHARED}" -C "${OUT}/lib" --strip-components=2 \
    sherpa-onnx-v1.13.3-rknn-linux-aarch64-shared/lib/libonnxruntime.so \
    sherpa-onnx-v1.13.3-rknn-linux-aarch64-shared/lib/libsherpa-onnx-c-api.so \
    sherpa-onnx-v1.13.3-rknn-linux-aarch64-shared/lib/libsherpa-onnx-cxx-api.so 2>/dev/null || true
fi
if [[ ! -f "${OUT}/lib/libsherpa-onnx-c-api.so" ]]; then
  echo "warn: missing sherpa RKNN shared libs in ${OUT}/lib; rebuild with SHERPA_ONNX_PACK=rknn" >&2
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

# Kokoro hybrid-v1 NPU TTS (optional; falls back to sherpa CPU when libkokoro unavailable)
if [[ -d "${MODELS_ROOT}/models/kokoro-hybrid-v1" ]]; then
  mkdir -p "${OUT}/models/kokoro-hybrid-v1"
  cp -a "${MODELS_ROOT}/models/kokoro-hybrid-v1/." "${OUT}/models/kokoro-hybrid-v1/"
  echo "Bundled Kokoro hybrid-v1 from ${MODELS_ROOT}/models/kokoro-hybrid-v1"
else
  echo "warn: missing ${MODELS_ROOT}/models/kokoro-hybrid-v1; run: make ensure-kokoro-rockchip" >&2
  echo "warn: board will use sherpa CPU kokoro fallback for TTS" >&2
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
