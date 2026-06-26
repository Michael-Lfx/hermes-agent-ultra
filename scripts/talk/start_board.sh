#!/bin/sh
# Rockchip board launcher: SenseVoice RKNN ASR + Kokoro hybrid-v1 RKNN TTS (sherpa CPU fallback).
set -eu

DIR="$(cd "$(dirname "$0")" && pwd)"
USER_HOME="${HOME:-$(cd ~ 2>/dev/null && pwd || echo "/root")}"
export HERMES_HOME="${HERMES_HOME:-${USER_HOME}/.hermes-agent-ultra}"
export HERMES_TALK_BUNDLE_DIR="${DIR}"
TALK_HOME="${HERMES_HOME}/hermes-talk"
CONFIG_EXAMPLE="${DIR}/config.example.toml"
HERMES_CONFIG_EXAMPLE="${DIR}/config.example.yaml"
HERMES_CONFIG="${HERMES_HOME}/config.yaml"

init_hermes_home() {
    mkdir -p \
        "${HERMES_HOME}" \
        "${HERMES_HOME}/profiles" \
        "${HERMES_HOME}/sessions" \
        "${HERMES_HOME}/logs" \
        "${HERMES_HOME}/skills" \
        "${HERMES_HOME}/cron" \
        "${HERMES_HOME}/cache" \
        "${TALK_HOME}"
}

init_talk_assets() {
    for item in models misaki-data espeak-ng-data; do
        if [ ! -e "${DIR}/${item}" ]; then
            continue
        fi
        dst="${TALK_HOME}/${item}"
        if [ -d "${dst}" ] && [ ! -L "${dst}" ]; then
            rm -rf "${dst}"
        fi
        ln -sfn "${DIR}/${item}" "${dst}"
    done
}

needs_default_config() {
    if [ ! -f "${TALK_HOME}/config.toml" ]; then
        return 0
    fi
    if grep -qE '11888|/home/key\.lic|/root/rktts/|"license_path": "key\.lic"|kokoro_server|127\.0\.0\.1:8848' "${TALK_HOME}/config.toml" 2>/dev/null; then
        return 0
    fi
    return 1
}

needs_hermes_config() {
    if [ ! -f "${HERMES_CONFIG}" ]; then
        return 0
    fi
    if grep -q '11888' "${HERMES_CONFIG}" 2>/dev/null; then
        return 0
    fi
    return 1
}

write_hermes_config() {
    if [ ! -f "${HERMES_CONFIG_EXAMPLE}" ]; then
        echo "error: missing ${HERMES_CONFIG_EXAMPLE}" >&2
        exit 1
    fi
    cp -f "${HERMES_CONFIG_EXAMPLE}" "${HERMES_CONFIG}"
    echo "Initialized ${HERMES_CONFIG} from config.example.yaml"
}

write_talk_config() {
    if [ ! -f "${CONFIG_EXAMPLE}" ]; then
        echo "error: missing ${CONFIG_EXAMPLE}" >&2
        exit 1
    fi
    cp -f "${CONFIG_EXAMPLE}" "${TALK_HOME}/config.toml"
    echo "Initialized ${TALK_HOME}/config.toml from config.example.toml"
}

preflight() {
    missing=0
    encoder="${DIR}/models/sensevoice-rk3588/encoder.rk3588.fp16-scaled.rknn"
    if [ -f "${encoder}" ]; then
        size="$(wc -c <"${encoder}")"
        if [ "${size}" -lt 400000000 ]; then
            echo "error: ${encoder} is truncated (${size} bytes, need ~490MB)" >&2
            echo "hint: on dev machine run: rm ${encoder} && make ensure-talk-models-rockchip && make package-talk-rockchip-dev" >&2
            missing=1
        fi
    fi
    for f in \
        "${encoder}" \
        "${DIR}/models/kokoro/model.onnx" \
        "${DIR}/models/kokoro/voices.bin" \
        "${DIR}/models/kokoro/tokens.txt" \
        "${DIR}/models/kokoro/lexicon-us-en.txt" \
        "${DIR}/models/kokoro/lexicon-zh.txt"
    do
        if [ ! -f "${f}" ]; then
            echo "warn: missing ${f}" >&2
            missing=1
        fi
    done
    if [ ! -d "${DIR}/models/kokoro/espeak-ng-data" ]; then
        echo "warn: missing ${DIR}/models/kokoro/espeak-ng-data" >&2
        missing=1
    fi
    if [ ! -f "${DIR}/models/kokoro-hybrid-v1/rk3588/kokoro-decoder-front.int8.rknn" ]; then
        echo "warn: missing kokoro-hybrid-v1 RKNN models; will use sherpa CPU kokoro fallback" >&2
        echo "warn: run make ensure-kokoro-rockchip on dev machine before packaging" >&2
    fi
    if [ "${missing}" -eq 1 ]; then
        echo "warn: bundle incomplete; run make ensure-talk-models-rockchip" >&2
    fi
}

echo "HERMES_HOME=${HERMES_HOME}"
init_hermes_home
echo "Initialized Hermes home (${HERMES_HOME})"
init_talk_assets
if needs_hermes_config; then
    write_hermes_config
fi
if needs_default_config; then
    write_talk_config
fi
preflight

export RUST_LOG="${RUST_LOG:-info,rustls=warn,hyper=warn,h2=warn}"
export LD_LIBRARY_PATH="${DIR}/lib:${LD_LIBRARY_PATH:-}"

exec "${DIR}/lib/ld-linux-aarch64.so.1" \
    --library-path "${DIR}/lib" \
    "${DIR}/bin/hermes-agent-ultra" talk "$@"
