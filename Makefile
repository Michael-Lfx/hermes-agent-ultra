# Hermes Agent Ultra — common build / cross-compile shortcuts
#
# aarch64 cross needs host-side prefetch (GCC 13 + pdfium + sherpa); see Cross.toml.

.DEFAULT_GOAL := help

ROOT        := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
SCRIPTS     := $(ROOT)/scripts
CROSS_CACHE := $(ROOT)/.cross-cache
DIST        := $(ROOT)/target/dist

PKG         ?= hermes-cli
BIN         ?= hermes
# Board / release image usually runs this binary (not the hermes wrapper).
AARCH64_BIN ?= hermes-agent-ultra
CROSS       ?= cross
CARGO       ?= cargo

TARGET_LINUX_X64    := x86_64-unknown-linux-gnu
TARGET_LINUX_AARCH64 := aarch64-unknown-linux-gnu
TARGET_LINUX_MUSL   := x86_64-unknown-linux-musl
TARGET_WINDOWS_GNU  := x86_64-pc-windows-gnu

PDFIUM_TAG          := chromium_7847
PDFIUM_X64          := pdfium-linux-x64
PDFIUM_AARCH64      := pdfium-linux-arm64
SHERPA_PLATFORM_X64 := linux-x64
SHERPA_PLATFORM_AARCH64 := linux-aarch64

GCC_AARCH64_GCC := $(CROSS_CACHE)/gcc-aarch64/bin/aarch64-none-linux-gnu-gcc

# Paths inside the cross container (/project = repo root)
CROSS_PROJECT          := /project
CROSS_PDFIUM_AARCH64   := $(CROSS_PROJECT)/.cross-cache/pdfium-rs/$(PDFIUM_TAG)/$(PDFIUM_AARCH64)
CROSS_GCC_AARCH64_GCC  := $(CROSS_PROJECT)/.cross-cache/gcc-aarch64/bin/aarch64-none-linux-gnu-gcc
CROSS_SHERPA_ARCHIVE   := $(CROSS_PROJECT)/.cross-cache/sherpa-onnx

# TMPDIR/XDG_CACHE_HOME for the container are set in Cross.toml (not on the host).
CROSS_COMMON_ENV := \
	AWS_LC_SYS_CMAKE_BUILDER=1

CROSS_AARCH64_ENV := \
	$(CROSS_COMMON_ENV) \
	CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=$(CROSS_GCC_AARCH64_GCC) \
	PKG_CONFIG_ALLOW_CROSS=1 \
	PDFIUM_LIB_PATH=$(CROSS_PDFIUM_AARCH64)/lib \
	PDFIUM_INCLUDE_PATH=$(CROSS_PDFIUM_AARCH64)/include \
	SHERPA_ONNX_ARCHIVE_DIR=$(CROSS_SHERPA_ARCHIVE)

.PHONY: help build test parity clippy-cli \
	prefetch-aarch64 prefetch-x86_64 prefetch-all \
	cross-rmi cross-aarch64 cross-x86_64 cross-musl cross-windows \
	package-aarch64 package-aarch64-board package-x86_64 package-musl

help:
	@echo "Hermes build targets:"
	@echo ""
	@echo "  Native:"
	@echo "    make build              cargo build -p $(PKG) --bin $(BIN)"
	@echo "    make test               cargo test -p $(PKG)"
	@echo "    make parity             cargo test -p hermes-parity-tests"
	@echo "    make clippy-cli         clippy on hermes-cli"
	@echo ""
	@echo "  Cross (Linux release binary '$(BIN)'):"
	@echo "    make prefetch-aarch64   GCC13 + pdfium + sherpa (host, before aarch64)"
	@echo "    make prefetch-x86_64    pdfium + sherpa for x86_64 gnu"
	@echo "    make cross-aarch64      cross build $(TARGET_LINUX_AARCH64) ($(AARCH64_BIN))"
	@echo "    make package-aarch64-board  tarball + GCC13 libs for RK3588 (use run-hermes-agent-ultra.sh)"
	@echo "    make cross-x86_64       cross build $(TARGET_LINUX_X64)"
	@echo "    make cross-musl         cross build $(TARGET_LINUX_MUSL)"
	@echo "    make cross-windows      cross build $(TARGET_WINDOWS_GNU)"
	@echo "    make cross-rmi          remove stuck cross custom Docker image (aarch64)"
	@echo ""
	@echo "  Packaging:"
	@echo "    make package-aarch64    tar.gz -> target/dist/hermes-linux-aarch64.tar.gz"
	@echo "    make package-x86_64     tar.gz -> target/dist/hermes-linux-x86_64.tar.gz"
	@echo ""
	@echo "  Variables: PKG=$(PKG) BIN=$(BIN) CROSS=$(CROSS)"
	@echo "  Custom GCC URL: make prefetch-aarch64 CROSS_GCC_URL=<tar.xz URL>"

build:
	$(CARGO) build -p $(PKG) --bin $(BIN)

test:
	$(CARGO) test -p $(PKG)

parity:
	$(CARGO) test -p hermes-parity-tests

clippy-cli:
	$(CARGO) clippy -p $(PKG) -- -D warnings

prefetch-aarch64:
	$(SCRIPTS)/cross_install_gcc_aarch64.sh
	$(SCRIPTS)/cross_prefetch_pdfium.sh $(PDFIUM_AARCH64)
	$(SCRIPTS)/cross_prefetch_sherpa_onnx.sh $(SHERPA_PLATFORM_AARCH64)
	@test -x "$(GCC_AARCH64_GCC)" || (echo "missing $(GCC_AARCH64_GCC)" >&2; exit 1)

prefetch-x86_64:
	$(SCRIPTS)/cross_prefetch_pdfium.sh $(PDFIUM_X64)
	$(SCRIPTS)/cross_prefetch_sherpa_onnx.sh $(SHERPA_PLATFORM_X64)

prefetch-all: prefetch-aarch64 prefetch-x86_64

cross-rmi:
	-docker rmi cross-custom-hermes-agent-ultra:$(TARGET_LINUX_AARCH64)-6e8c7-pre-build 2>/dev/null

cross-aarch64: prefetch-aarch64
	$(CROSS_AARCH64_ENV) $(CROSS) build --release --target $(TARGET_LINUX_AARCH64) -p $(PKG) --bin $(AARCH64_BIN)

cross-x86_64: prefetch-x86_64
	$(CROSS_COMMON_ENV) SHERPA_ONNX_ARCHIVE_DIR=$(CROSS_PROJECT)/.cross-cache/sherpa-onnx \
		PDFIUM_LIB_PATH=$(CROSS_PROJECT)/.cross-cache/pdfium-rs/$(PDFIUM_TAG)/$(PDFIUM_X64)/lib \
		PDFIUM_INCLUDE_PATH=$(CROSS_PROJECT)/.cross-cache/pdfium-rs/$(PDFIUM_TAG)/$(PDFIUM_X64)/include \
		$(CROSS) build --release --target $(TARGET_LINUX_X64) -p $(PKG) --bin $(BIN)

cross-musl:
	$(CROSS_COMMON_ENV) $(CROSS) build --release --target $(TARGET_LINUX_MUSL) -p $(PKG) --bin $(BIN)

cross-windows:
	$(CROSS_COMMON_ENV) $(CROSS) build --release --target $(TARGET_WINDOWS_GNU) -p $(PKG) --bin $(BIN)

package-aarch64: cross-aarch64
	@mkdir -p $(DIST)
	cp $(ROOT)/target/$(TARGET_LINUX_AARCH64)/release/$(AARCH64_BIN) $(DIST)/$(AARCH64_BIN)
	tar -C $(DIST) -czf $(DIST)/hermes-linux-aarch64.tar.gz $(AARCH64_BIN)
	@echo "$(DIST)/hermes-linux-aarch64.tar.gz"

package-aarch64-board: cross-aarch64
	chmod +x $(SCRIPTS)/package_aarch64_board.sh
	DIST_DIR=$(DIST) $(SCRIPTS)/package_aarch64_board.sh

package-x86_64: cross-x86_64
	@mkdir -p $(DIST)
	cp $(ROOT)/target/$(TARGET_LINUX_X64)/release/$(BIN) $(DIST)/
	tar -C $(DIST) -czf $(DIST)/hermes-linux-x86_64.tar.gz $(BIN)
	@echo "$(DIST)/hermes-linux-x86_64.tar.gz"

package-musl: cross-musl
	@mkdir -p $(DIST)
	cp $(ROOT)/target/$(TARGET_LINUX_MUSL)/release/$(BIN) $(DIST)/
	tar -C $(DIST) -czf $(DIST)/hermes-linux-x86_64-musl.tar.gz $(BIN)
	@echo "$(DIST)/hermes-linux-x86_64-musl.tar.gz"
