#!/usr/bin/env bash
# Bundle hermes-agent-ultra + GCC 13 runtime for boards with older glibc (RK3588).
# Must launch via bundled ld-linux — LD_LIBRARY_PATH alone causes immediate segfaults.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="${DIST_DIR:-${ROOT}/target/dist}"
BIN="${ROOT}/target/aarch64-unknown-linux-gnu/release/hermes-agent-ultra"
GCC_LIB="${ROOT}/.cross-cache/gcc-aarch64/aarch64-none-linux-gnu"
OUT="${DIST}/hermes-agent-ultra-aarch64"

if [[ ! -f "${BIN}" ]]; then
  echo "missing ${BIN}; run: make cross-aarch64" >&2
  exit 1
fi

mkdir -p "${OUT}/bin" "${OUT}/lib"
cp -f "${BIN}" "${OUT}/bin/hermes-agent-ultra"
chmod +x "${OUT}/bin/hermes-agent-ultra"

copy_lib() {
  local name="$1"
  local dir="$2"
  local src
  src="$(find "${dir}" -maxdepth 1 -name "${name}" 2>/dev/null | head -1)"
  if [[ -z "${src}" ]]; then
    echo "warning: ${name} not found under ${dir}" >&2
    return 0
  fi
  cp -aL "${src}" "${OUT}/lib/" 2>/dev/null || cp -a "${src}" "${OUT}/lib/"
}

# Dynamic linker (must match bundled libc)
copy_lib ld-linux-aarch64.so.1 "${GCC_LIB}/libc/lib"
copy_lib ld-linux-aarch64.so.1 "${GCC_LIB}/libc/lib64"

for lib in libc.so.6 libm.so.6 libpthread.so.0 libdl.so.2 librt.so.1 libutil.so.1 \
  libresolv.so.2 libnss_files.so.2 libnss_dns.so.2 libgcc_s.so.1 libstdc++.so.6; do
  copy_lib "${lib}" "${GCC_LIB}/libc/lib64"
done
copy_lib libstdc++.so.6 "${GCC_LIB}/lib64"
copy_lib libgcc_s.so.1 "${GCC_LIB}/lib64"

if [[ ! -x "${OUT}/lib/ld-linux-aarch64.so.1" ]]; then
  echo "missing ld-linux-aarch64.so.1 in ${OUT}/lib; check GCC toolchain under ${GCC_LIB}" >&2
  exit 1
fi

cat > "${OUT}/run-hermes-agent-ultra.sh" <<'EOF'
#!/bin/sh
DIR="$(cd "$(dirname "$0")" && pwd)"
# Do not use LD_LIBRARY_PATH alone: system ld-linux + bundled libc => segfault at startup.
exec "${DIR}/lib/ld-linux-aarch64.so.1" \
  --library-path "${DIR}/lib" \
  "${DIR}/bin/hermes-agent-ultra" "$@"
EOF
chmod +x "${OUT}/run-hermes-agent-ultra.sh"

mkdir -p "${DIST}"
TARBALL="${DIST}/hermes-agent-ultra-aarch64.tar.gz"
tar -C "${DIST}" -czf "${TARBALL}" "$(basename "${OUT}")"
echo "${TARBALL}"
echo "On board: tar xzf $(basename "${TARBALL}") && ./hermes-agent-ultra-aarch64/run-hermes-agent-ultra.sh"
