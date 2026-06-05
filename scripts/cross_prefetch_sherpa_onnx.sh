#!/usr/bin/env bash
# Prefetch sherpa-onnx static libs for sherpa-onnx-sys (see SHERPA_ONNX_ARCHIVE_DIR).
#
#   ./scripts/cross_prefetch_sherpa_onnx.sh linux-aarch64
#   export SHERPA_ONNX_ARCHIVE_DIR="/project/.cross-cache/sherpa-onnx"
#   cross build --target aarch64-unknown-linux-gnu --release -p hermes-cli
#
# Platforms: linux-x64 | linux-aarch64 | osx-x64 | osx-arm64 | win-x64

set -euo pipefail

PLATFORM="${1:?usage: $0 <linux-aarch64|linux-x64|...>}"
VERSION="${SHERPA_ONNX_VERSION:-1.13.2}"
ARCHIVE="sherpa-onnx-v${VERSION}-${PLATFORM}-static-lib.tar.bz2"
ROOT="${XDG_CACHE_HOME:-$(cd "$(dirname "$0")/.." && pwd)/.cross-cache}"
DEST="${ROOT}/sherpa-onnx"
URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/v${VERSION}/${ARCHIVE}"

mkdir -p "${DEST}"
if [[ -f "${DEST}/${ARCHIVE}" ]]; then
  echo "sherpa-onnx archive already at ${DEST}/${ARCHIVE}"
  exit 0
fi

command -v curl >/dev/null || { echo "curl required" >&2; exit 1; }

echo "sherpa-onnx: GET ${URL}"
curl -fsSL -o "${DEST}/${ARCHIVE}" "${URL}"
echo "sherpa-onnx cached at ${DEST}/${ARCHIVE}"
