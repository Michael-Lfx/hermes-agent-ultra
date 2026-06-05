#!/usr/bin/env bash
# Prefetch pdfium binaries for liteparse-pdfium-sys (same layout as its build.rs cache).
#
# Use when `cross build` cannot reach GitHub from inside Docker (e.g. proxy on 127.0.0.1).
#
#   ./scripts/cross_prefetch_pdfium.sh pdfium-linux-arm64
#   export PDFIUM_LIB_PATH="/project/.cross-cache/pdfium-rs/chromium_7847/pdfium-linux-arm64/lib"
#   export PDFIUM_INCLUDE_PATH="/project/.cross-cache/pdfium-rs/chromium_7847/pdfium-linux-arm64/include"
#   cross build --target aarch64-unknown-linux-gnu --release -p hermes-cli
#
# Assets: pdfium-linux-x64 | pdfium-linux-arm64 | pdfium-mac-arm64 | ...

set -euo pipefail

ASSET="${1:?usage: $0 <pdfium-linux-arm64|pdfium-linux-x64|...>}"
TAG_SAFE="chromium_7847"
TAG_ENCODED="chromium%2F7847"
ROOT="${XDG_CACHE_HOME:-$(cd "$(dirname "$0")/.." && pwd)/.cross-cache}"
CACHE="${ROOT}/pdfium-rs/${TAG_SAFE}/${ASSET}"
URL="https://github.com/run-llama/pdfium-binaries/releases/download/${TAG_ENCODED}/${ASSET}.tgz"

if [[ -d "${CACHE}/lib" && -d "${CACHE}/include" ]]; then
  echo "pdfium already cached at ${CACHE}"
  exit 0
fi

command -v curl >/dev/null || { echo "curl required" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

echo "pdfium: GET ${URL}"
curl -fsSL -o "${TMP}/pdfium.tgz" "${URL}"
mkdir -p "${TMP}/extract" "$(dirname "${CACHE}")"
tar --no-same-owner -xzf "${TMP}/pdfium.tgz" -C "${TMP}/extract"
rm -rf "${CACHE}"
mv "${TMP}/extract" "${CACHE}"
echo "pdfium cached at ${CACHE}"
