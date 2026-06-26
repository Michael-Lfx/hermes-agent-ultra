#!/usr/bin/env bash
# Cross-compile Kokoro hybrid-v1 RKNN FFI static lib for aarch64 Rockchip boards.
#
# Output: ${CROSS_CACHE}/kokoro-hybrid/libkokoro_ffi.a
# Consumed by crates/hermes-talk/build.rs (auto-detected prebuilt path).
set -euo pipefail

ROOT="${HERMES_ULTRA_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
CACHE="${CROSS_CACHE:-${ROOT}/.cross-cache}"
OUT_DIR="${CACHE}/kokoro-hybrid"
INCLUDE="${OUT_DIR}/include"
LIB="${OUT_DIR}/libkokoro_ffi.a"
SRC="${ROOT}/crates/hermes-talk/kokoro/kokoro_ffi.cpp"
KOKORO_INC="${ROOT}/crates/hermes-talk/kokoro"

GCC_DIR="${CACHE}/gcc-aarch64/bin"
CXX="${CXX_AARCH64:-${GCC_DIR}/aarch64-none-linux-gnu-g++}"

ORT_VER=1.24.4
ORT_TAG="v${ORT_VER}"
RKNN_API_URL="https://github.com/airockchip/rknn-toolkit2/raw/v2.2.0/rknpu2/runtime/Linux/librknn_api/include/rknn_api.h"

mkdir -p "${INCLUDE}/rknn" "${OUT_DIR}"

if [[ ! -x "${CXX}" ]]; then
  echo "error: missing cross g++ at ${CXX}; run: make prefetch-talk-aarch64" >&2
  exit 1
fi

if [[ ! -f "${INCLUDE}/rknn/rknn_api.h" ]]; then
  echo "  GET ${RKNN_API_URL}"
  curl -fsSL --retry 3 --retry-delay 2 -o "${INCLUDE}/rknn/rknn_api.h" "${RKNN_API_URL}"
fi

ORT_INC="${INCLUDE}/onnxruntime"
if [[ ! -f "${ORT_INC}/onnxruntime_cxx_api.h" ]]; then
  ORT_ARCHIVE="${OUT_DIR}/onnxruntime-${ORT_TAG}.tar.gz"
  ORT_EXTRACT="${OUT_DIR}/.ort-extract"
  ORT_URL="https://github.com/microsoft/onnxruntime/archive/refs/tags/${ORT_TAG}.tar.gz"
  if [[ ! -f "${ORT_ARCHIVE}" ]]; then
    echo "  GET ${ORT_URL}"
    curl -fsSL --retry 3 --retry-delay 2 -o "${ORT_ARCHIVE}" "${ORT_URL}"
  fi
  rm -rf "${ORT_EXTRACT}"
  mkdir -p "${ORT_EXTRACT}"
  tar xzf "${ORT_ARCHIVE}" -C "${ORT_EXTRACT}" --no-same-owner
  ORT_SRC="$(find "${ORT_EXTRACT}" -path "*/include/onnxruntime/core/session/onnxruntime_cxx_api.h" | head -1)"
  if [[ -z "${ORT_SRC}" ]]; then
    echo "error: onnxruntime ${ORT_TAG} headers not found in archive" >&2
    exit 1
  fi
  rm -rf "${ORT_INC}"
  mkdir -p "${ORT_INC}"
  cp -a "$(dirname "${ORT_SRC}")/"* "${ORT_INC}/"
  rm -rf "${ORT_EXTRACT}"
  echo "  onnxruntime ${ORT_VER} headers -> ${ORT_INC}"
fi

OBJ="${OUT_DIR}/kokoro_ffi.o"
echo "=== building ${LIB} (aarch64) ==="
"${CXX}" -std=c++20 -O2 -fPIC \
  -I"${KOKORO_INC}" \
  -I"${ORT_INC}" \
  -I"${INCLUDE}/rknn" \
  -c "${SRC}" -o "${OBJ}"
ar rcs "${LIB}" "${OBJ}"
rm -f "${OBJ}"
echo "  -> ${LIB}"
