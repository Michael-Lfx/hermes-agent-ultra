#!/usr/bin/env bash
# cross + crt-static makes cc-rs pass -static when compiling .c → .o, which breaks
# in the cross container. Strip -static only for compile (-c) invocations.
set -euo pipefail

REAL_GCC="${REAL_AARCH64_GCC:-/usr/bin/aarch64-linux-gnu-gcc}"
args=()
compile=0
for a in "$@"; do
  case "$a" in
    -c) compile=1 ;;
    -static) [[ "$compile" -eq 1 ]] && continue ;;
  esac
  args+=("$a")
done
exec "${REAL_GCC}" "${args[@]}"
