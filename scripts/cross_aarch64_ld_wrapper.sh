#!/usr/bin/env bash
# Link hermes-agent-ultra with crt-static (glibc/libstdc++ in-binary) and dynamic libasound only.
set -euo pipefail

REAL_GCC="${REAL_AARCH64_GCC:-/project/.cross-cache/gcc-aarch64/bin/aarch64-none-linux-gnu-gcc}"
ASOUND_LIB_DIR="${ASOUND_LIB_DIR:-/usr/lib/aarch64-linux-gnu}"

args=("$@")
out=()
fixed_asound=0
i=0
while [[ $i -lt ${#args[@]} ]]; do
  a="${args[i]}"
  if [[ "$a" == "-lasound" && "$fixed_asound" -eq 0 ]]; then
    out+=(-Wl,-Bdynamic "-L${ASOUND_LIB_DIR}" -lasound -Wl,-Bstatic)
    fixed_asound=1
    i=$((i + 1))
    continue
  fi
  # rustc/g++ may append -Bdynamic before -lstdc++; keep libstdc++ static.
  if [[ "$a" == "-Wl,-Bdynamic" && $((i + 1)) -lt ${#args[@]} && "${args[i + 1]}" == "-lstdc++" ]]; then
    out+=(-Wl,-Bstatic)
    i=$((i + 1))
    continue
  fi
  out+=("$a")
  i=$((i + 1))
done

exec "${REAL_GCC}" "${out[@]}"
