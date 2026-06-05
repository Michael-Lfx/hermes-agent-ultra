#!/usr/bin/env bash
# Deprecated name — installs GCC 13 (gcc-aarch64), not GCC 10.
exec "$(cd "$(dirname "$0")" && pwd)/cross_install_gcc_aarch64.sh" "$@"
