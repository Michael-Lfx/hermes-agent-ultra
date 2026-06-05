#!/usr/bin/env bash
# ARM GNU Toolchain 13.x for aarch64 cross-link (sherpa-onnx v1.13 static libs need libstdc++ >= 12).
#
# Host-only (not cross pre-build). Then in cross container (/project = repo root):
#   export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="/project/.cross-cache/gcc-aarch64/bin/aarch64-none-linux-gnu-gcc"
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${CROSS_GCC_VERSION:-13.2.rel1}"
FILE="arm-gnu-toolchain-${VERSION}-x86_64-aarch64-none-linux-gnu.tar.xz"
PREFIX="${CROSS_GCC_PREFIX:-${ROOT}/.cross-cache/gcc-aarch64}"
GCC="${PREFIX}/bin/aarch64-none-linux-gnu-gcc"
URL="${CROSS_GCC_URL:-https://armkeil.blob.core.windows.net/developer/Files/downloads/gnu/${VERSION}/binrel/${FILE}}"

if [[ -x "${GCC}" ]]; then
  echo "gcc-aarch64 already installed at ${PREFIX}"
  "${GCC}" --version | head -1
  exit 0
fi

command -v curl >/dev/null || { echo "curl required" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

echo "gcc-aarch64: GET ${URL}"
curl -fsSL -o "${TMP}/gcc-aarch64.tar.xz" "${URL}"
mkdir -p "${PREFIX}"
tar --no-same-owner -xJf "${TMP}/gcc-aarch64.tar.xz" -C "${PREFIX}" --strip-components=1
chmod -R a+rX "${PREFIX}"
echo "gcc-aarch64 installed: $("${GCC}" --version | head -1)"
