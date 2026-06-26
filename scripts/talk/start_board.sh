#!/bin/sh
# Rockchip board launcher: kokoro-server (TTS) + hermes-talk voice dialog.
set -eu

DIR="$(cd "$(dirname "$0")" && pwd)"
USER_HOME="${HOME:-$(cd ~ 2>/dev/null && pwd || echo "/root")}"
export HERMES_HOME="${HERMES_HOME:-${USER_HOME}/.hermes-agent-ultra}"
export HERMES_TALK_BUNDLE_DIR="${DIR}"
TALK_HOME="${HERMES_HOME}/hermes-talk"
CONFIG_EXAMPLE="${DIR}/config.example.toml"
HERMES_CONFIG_EXAMPLE="${DIR}/config.example.yaml"
HERMES_CONFIG="${HERMES_HOME}/config.yaml"
KOKORO_MODELS="${DIR}/models/kokoro"

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
    if grep -qE '11888|/home/key\.lic|/root/rktts/|"license_path": "key\.lic"' "${TALK_HOME}/config.toml" 2>/dev/null; then
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

start_kokoro_server() {
    KOKORO_BIN="${DIR}/bin/kokoro-server"
    if [ ! -x "${KOKORO_BIN}" ]; then
        echo "error: missing ${KOKORO_BIN}" >&2
        exit 1
    fi
    if [ ! -f "${KOKORO_MODELS}/kokoro_encoder.onnx" ] \
        || [ ! -f "${KOKORO_MODELS}/kokoro_decoder.rknn" ]; then
        echo "error: missing Kokoro models under ${KOKORO_MODELS}" >&2
        exit 1
    fi

    ESPEAK_DATA="${DIR}/espeak-ng-data"
    LEXICON="${DIR}/misaki-data"
    VOICES="${KOKORO_MODELS}/voices_npy"
    VOCAB="${KOKORO_MODELS}/config.json"

    KOKORO_ARGS="
      --encoder ${KOKORO_MODELS}/kokoro_encoder.onnx
      --har-gen ${KOKORO_MODELS}/har_generator.onnx
      --decoder ${KOKORO_MODELS}/kokoro_decoder.rknn
      --vocab ${VOCAB}
      --voices-dir ${VOICES}
      --ip 127.0.0.1
      --port 8848
      --disable-web-ui
    "
    if [ -d "${ESPEAK_DATA}" ]; then
        KOKORO_ARGS="${KOKORO_ARGS} --espeak-data ${ESPEAK_DATA}"
    fi
    if [ -d "${LEXICON}" ]; then
        KOKORO_ARGS="${KOKORO_ARGS} --lexicon-dir ${LEXICON}"
    fi

    export LD_LIBRARY_PATH="${DIR}/lib:${LD_LIBRARY_PATH:-}"
    # shellcheck disable=SC2086
    "${DIR}/lib/ld-linux-aarch64.so.1" \
        --library-path "${DIR}/lib" \
        "${KOKORO_BIN}" ${KOKORO_ARGS} &
    KOKORO_PID=$!
    trap 'kill "${KOKORO_PID}" 2>/dev/null || true' EXIT INT TERM

    # Wait for HTTP port
    i=0
    while [ "${i}" -lt 50 ]; do
        if wget -q -O /dev/null "http://127.0.0.1:8848/api/v1/voices" 2>/dev/null; then
            echo "kokoro-server ready on :8848 (pid ${KOKORO_PID})"
            return 0
        fi
        i=$((i + 1))
        sleep 0.2
    done
    echo "warn: kokoro-server may not be ready yet" >&2
}

preflight() {
    missing=0
    for d in "${DIR}/models/sensevoice-rk3588" "${DIR}/models/kws-zh-en" "${KOKORO_MODELS}"; do
        if [ ! -d "${d}" ]; then
            echo "warn: missing ${d}" >&2
            missing=1
        fi
    done
    if [ "${missing}" -eq 1 ]; then
        echo "warn: bundle incomplete; check make package-talk-rockchip" >&2
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
start_kokoro_server

export RUST_LOG="${RUST_LOG:-info,rustls=warn,hyper=warn,h2=warn}"

exec "${DIR}/lib/ld-linux-aarch64.so.1" \
    --library-path "${DIR}/lib" \
    "${DIR}/bin/hermes-agent-ultra" talk "$@"
