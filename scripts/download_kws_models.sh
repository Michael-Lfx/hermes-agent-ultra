#!/usr/bin/env bash
# Download sherpa-onnx zh-en KWS models next to half_duplex.toml (default: ~/.hermes-agent-ultra).
set -euo pipefail

HERMES_HOME="${HERMES_HOME:-$HOME/.hermes-agent-ultra}"
MODEL_DIR="${1:-$HERMES_HOME/models/kws-zh-en-3M-2025-12-20}"
TAG="kws-models"
ARCHIVE="kws-zh-en-3M-2025-12-20.tar.bz2"
BASE="https://github.com/k2-fsa/sherpa-onnx/releases/download/${TAG}"

mkdir -p "$(dirname "$MODEL_DIR")"
if [[ -f "${MODEL_DIR}/encoder.onnx" ]]; then
  echo "KWS models already present: ${MODEL_DIR}"
  exit 0
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

echo "Downloading ${ARCHIVE} ..."
curl -fsSL -o "${tmpdir}/${ARCHIVE}" "${BASE}/${ARCHIVE}"
tar -xjf "${tmpdir}/${ARCHIVE}" -C "${tmpdir}"
src="${tmpdir}/kws-zh-en-3M-2025-12-20"
if [[ ! -d "$src" ]]; then
  echo "unexpected archive layout under ${tmpdir}" >&2
  exit 1
fi
rm -rf "$MODEL_DIR"
mv "$src" "$MODEL_DIR"
echo "Installed KWS models to ${MODEL_DIR}"
echo "Set in ${HERMES_HOME}/half_duplex.toml:"
echo "  [wake] enabled = true"
echo "  model_dir = \"models/kws-zh-en-3M-2025-12-20\""
