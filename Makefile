# Hermes Agent Ultra — common dev & ops targets
#
# Usage:
#   make              # show help
#   make build        # debug build (workspace)
#   make release      # build release binary (native)
#   make release-arm  # build release for ARM64 Linux
#   make test         # run tests
#   make check        # cargo check (fast)
#   make clippy       # clippy (all crates)
#
# Voice dialog (hermes talk — requires --features talk on hermes-cli):
#   make build-talk           # debug build with talk feature
#   make release-talk         # release build with talk feature
#   make release-talk-rockchip # native aarch64 + Rockchip local ASR/TTS
#   make talk-init            # init $HERMES_HOME/hermes-talk
#   make talk-run             # start voice dialog loop

CARGO       ?= cargo
CROSS       := cross
BIN         := hermes-agent-ultra
BIN_CRATE   := hermes-cli
TARGET      ?= target
RELEASE_BIN := $(TARGET)/release/$(BIN)
DEBUG_BIN   := $(TARGET)/debug/$(BIN)

# hermes talk (optional voice dialog)
TALK_FEATURES     := talk
TALK_FEATURES_RK  := talk-rockchip
TALK_PKG          := -p $(BIN_CRATE) --features $(TALK_FEATURES) --bin $(BIN)
TALK_PKG_RK       := -p $(BIN_CRATE) --features $(TALK_FEATURES_RK) --bin $(BIN)
TALK_CRATE        := crates/hermes-talk
TALK_RKAUDIO      := $(TALK_CRATE)/rkaudio
TALK_RUN          = $(CARGO) run $(TALK_PKG) -- talk
TALK_RUN_RK       = $(CARGO) run $(TALK_PKG_RK) -- talk

# Cross-compilation targets
ARM64_TARGET        := aarch64-unknown-linux-gnu
ARM64_RELEASE       := $(TARGET)/$(ARM64_TARGET)/release/$(BIN)
ARM64_MUSL_TARGET   := aarch64-unknown-linux-musl
ARM64_MUSL_RELEASE  := $(TARGET)/$(ARM64_MUSL_TARGET)/release/$(BIN)

# Cross toolchain cache (workspace root)
ROOT        := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
CROSS_CACHE := $(ROOT)/.cross-cache
DIST        := $(ROOT)/target/dist
GCC_AARCH64 := $(CROSS_CACHE)/gcc-aarch64/bin/aarch64-none-linux-gnu-gcc
CXX_AARCH64 := $(CROSS_CACHE)/gcc-aarch64/bin/aarch64-none-linux-gnu-g++

# Rockchip SDK paths (override as needed)
RK_TTS_SDK_DIR ?= /home/leeyang/Rockchip_RKTTS_SDK_Release
RK_TTS_LIB     := $(RK_TTS_SDK_DIR)/lib/Linux/aarch64
RK_ASR_SDK_DIR ?= /home/leeyang/ASR_SDK/ROCKASR2_RK3588/rockasr2_android_linux_rk3588_20260312

# Packaging / cross prefetch (scripts/talk; vendored, respect HERMES_ULTRA_ROOT)
TALK_SCRIPTS        := $(ROOT)/scripts/talk
TALK_VENDOR_SCRIPTS ?= $(abspath $(ROOT)/../tts-stream/scripts)
TTS_STREAM_CACHE    := $(abspath $(ROOT)/../tts-stream/.cross-cache)
MODELS_ROOT         ?= $(ROOT)/.models

CROSS_AARCH64_ENV := \
	SHERPA_ONNX_ARCHIVE_DIR=$(CROSS_CACHE)/sherpa-onnx \
	HERMES_BUNDLED_RG_ARCHIVE_DIR=$(CROSS_CACHE)/ripgrep \
	PKG_CONFIG_ALLOW_CROSS=1 \
	RK_TTS_SDK_DIR=$(RK_TTS_SDK_DIR) \
	RK_ASR_SDK_DIR=$(RK_ASR_SDK_DIR) \
	LIBCLANG_PATH=$(CROSS_CACHE)/llvm-14/lib \
	CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=$(GCC_AARCH64) \
	CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS=-Clink-arg=-Wl,--allow-multiple-definition \
	BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_gnu=--target=aarch64-unknown-linux-gnu \
	BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_musl=--target=aarch64-unknown-linux-musl

RK_LINK_FLAGS := -C link-arg=-Wl,--allow-multiple-definition

# Override: make start CONFIG=path/to/config.yaml
CONFIG      ?=
CONFIG_FLAG := $(if $(CONFIG),--config $(CONFIG),)

# Release binary if built; otherwise `cargo run --`.
HERMES      = $(if $(wildcard $(RELEASE_BIN)),$(RELEASE_BIN),$(CARGO) run --bin $(BIN) --)

.PHONY: help build release release-arm release-arm64 release-arm64-musl \
        test check clippy clean \
        build-talk release-talk release-talk-rockchip release-talk-rockchip-arm64 \
        package-talk-rockchip prefetch-talk-aarch64 \
        test-talk check-talk clippy-talk \
        talk-init talk-run talk-enroll talk-list-devices \
        talk-probe-capture talk-probe-playback

help:
	@echo "Core:"
	@echo "  build              Debug build (workspace)"
	@echo "  release            Release build (native)"
	@echo "  release-arm        Alias for release-arm64-musl (most portable)"
	@echo "  release-arm64      Build release for ARM64 Linux (glibc)"
	@echo "  release-arm64-musl Build release for ARM64 Linux (musl, fully static)"
	@echo "  test               Run workspace tests"
	@echo "  check              cargo check (fast, workspace)"
	@echo "  clippy             cargo clippy (all crates, -D warnings)"
	@echo ""
	@echo "  start              Run hermes (release if built, else debug)"
	@echo ""
	@echo "hermes talk (voice dialog, --features talk):"
	@echo "  build-talk                 Debug build hermes-cli + hermes-talk"
	@echo "  release-talk               Release build with talk feature"
	@echo "  release-talk-rockchip      Native aarch64 + Rockchip local ASR/TTS"
	@echo "  release-talk-rockchip-arm64 Cross-compile aarch64 + Rockchip"
	@echo "  package-talk-rockchip      Bundle binary + SDK libs + models from .models/"
	@echo "  prefetch-talk-aarch64      Download LLVM/sherpa-onnx for aarch64 cross"
	@echo "  test-talk / check-talk / clippy-talk"
	@echo "  talk-init                  Init \$$HERMES_HOME/hermes-talk"
	@echo "  talk-run                   Start voice dialog loop"
	@echo "  talk-enroll                Enroll voiceprint (SECONDS=5)"
	@echo "  talk-list-devices          List audio devices"
	@echo "  talk-probe-capture         Mic diagnostic (SECONDS=5)"
	@echo "  talk-probe-playback        Speaker test tone"
	@echo ""
	@echo "Options:"
	@echo "  CONFIG=path        Pass --config to hermes (e.g. CONFIG=config.yaml)"
	@echo "  SECONDS=N          Duration for talk-enroll / talk-probe-capture"
	@echo "  RK_TTS_SDK_DIR=    Rockchip TTS SDK root (fallback if not in .models/)"
	@echo "  RK_ASR_SDK_DIR=    Rockchip ASR SDK root (fallback if not in .models/)"
	@echo "  MODELS_ROOT=       Packaging model tree (default: ./.models, incl. auth/)"
	@echo "  TALK_VENDOR_SCRIPTS= Path to aarch64 prefetch scripts (default: ../tts-stream/scripts)"

build:
	$(CARGO) build

release:
	$(CARGO) build --release --bin $(BIN)

release-arm: release-arm64-musl

release-arm64:
	$(CROSS) build --release --target $(ARM64_TARGET) --bin $(BIN)
	@echo "Built $(ARM64_RELEASE)"

release-arm64-musl:
	$(CROSS) build --release --target $(ARM64_MUSL_TARGET) --bin $(BIN)
	@echo "Built $(ARM64_MUSL_RELEASE)"

test:
	$(CARGO) test

check:
	$(CARGO) check

clippy:
	$(CARGO) clippy -- -D warnings

start:
	$(HERMES) $(CONFIG_FLAG)

clean:
	$(CARGO) clean
	rm -rf $(TARGET)/$(ARM64_TARGET) $(TARGET)/$(ARM64_MUSL_TARGET)

# ---------------------------------------------------------------------------
# hermes talk
# ---------------------------------------------------------------------------

build-talk:
	$(CARGO) build $(TALK_PKG)

release-talk:
	$(CARGO) build --release $(TALK_PKG)
	@echo "Built $(RELEASE_BIN) (features: $(TALK_FEATURES))"

release-talk-rockchip:
	RUSTFLAGS="$(RK_LINK_FLAGS)" \
	RK_TTS_SDK_DIR=$(RK_TTS_SDK_DIR) \
	RK_ASR_SDK_DIR=$(RK_ASR_SDK_DIR) \
	$(CARGO) build --release $(TALK_PKG_RK)
	@echo "Built $(RELEASE_BIN) (features: $(TALK_FEATURES_RK))"

release-talk-rockchip-arm64: $(GCC_AARCH64) $(TALK_RKAUDIO)/librktts_c_api.a $(TALK_RKAUDIO)/lib
	$(CROSS_AARCH64_ENV) \
	RUSTFLAGS="$(RK_LINK_FLAGS) -C link-arg=-static-libstdc++ -C link-arg=-static-libgcc" \
	$(CROSS) build --release --target $(ARM64_TARGET) $(TALK_PKG_RK)
	patchelf --set-rpath '$$ORIGIN/lib' $(ARM64_RELEASE)
	@echo "Built $(ARM64_RELEASE) (features: $(TALK_FEATURES_RK))"

$(TALK_RKAUDIO)/librktts_c_api.a: $(TALK_RKAUDIO)/rk_tts_c_api.cpp $(TALK_RKAUDIO)/rk_tts_c_api.h
	mkdir -p $(TALK_RKAUDIO)
	$(CXX_AARCH64) -std=c++11 \
		-I$(RK_TTS_SDK_DIR)/include \
		-c $(TALK_RKAUDIO)/rk_tts_c_api.cpp \
		-o $(TALK_RKAUDIO)/rk_tts_c_api.o
	ar rcs $@ $(TALK_RKAUDIO)/rk_tts_c_api.o
	rm -f $(TALK_RKAUDIO)/rk_tts_c_api.o

$(TALK_RKAUDIO)/lib: $(RK_TTS_LIB)/librktts.so $(RK_TTS_LIB)/librknnrt.so
	mkdir -p $(TALK_RKAUDIO)/lib
	cp $(RK_TTS_LIB)/librktts.so $(TALK_RKAUDIO)/lib/
	cp $(RK_TTS_LIB)/librknnrt.so $(TALK_RKAUDIO)/lib/

$(GCC_AARCH64):
	@mkdir -p $(CROSS_CACHE)
	@if [ -x '$(GCC_AARCH64)' ] && [ ! -L '$(CROSS_CACHE)/gcc-aarch64' ]; then \
		: ; \
	elif [ -x '$(TTS_STREAM_CACHE)/gcc-aarch64/bin/aarch64-none-linux-gnu-gcc' ]; then \
		echo "Copying cross toolchain from $(TTS_STREAM_CACHE) -> $(CROSS_CACHE) (required for cross Docker)"; \
		rm -rf '$(CROSS_CACHE)/gcc-aarch64' '$(CROSS_CACHE)/llvm-14' '$(CROSS_CACHE)/sherpa-onnx'; \
		cp -a '$(TTS_STREAM_CACHE)/gcc-aarch64' '$(CROSS_CACHE)/'; \
		cp -a '$(TTS_STREAM_CACHE)/llvm-14' '$(CROSS_CACHE)/'; \
		cp -a '$(TTS_STREAM_CACHE)/sherpa-onnx' '$(CROSS_CACHE)/'; \
	fi
	@if [ ! -x '$(GCC_AARCH64)' ]; then \
		$(MAKE) prefetch-talk-aarch64; \
	fi
	@test -x '$(GCC_AARCH64)' || (echo "missing $(GCC_AARCH64); run: make prefetch-talk-aarch64" >&2; exit 1)

prefetch-talk-aarch64:
	@mkdir -p $(CROSS_CACHE)
	HERMES_ULTRA_ROOT=$(ROOT) CROSS_CACHE=$(CROSS_CACHE) $(TALK_SCRIPTS)/prefetch_aarch64.sh
	HERMES_ULTRA_ROOT=$(ROOT) CROSS_GCC_PREFIX=$(CROSS_CACHE)/gcc-aarch64 $(TALK_SCRIPTS)/install_gcc_aarch64.sh

package-talk-rockchip: release-talk-rockchip-arm64
	ROOT=$(ROOT) DIST_DIR=$(DIST) MODELS_ROOT=$(MODELS_ROOT) $(TALK_SCRIPTS)/package_aarch64_rockchip.sh

test-talk:
	$(CARGO) test -p hermes-talk

check-talk:
	$(CARGO) check $(TALK_PKG)

clippy-talk:
	$(CARGO) clippy -p hermes-talk -p hermes-config -- -D warnings

talk-init:
	$(TALK_RUN) init

talk-run:
	$(TALK_RUN) run

talk-enroll:
	$(TALK_RUN) enroll $(if $(SECONDS),--seconds $(SECONDS),)

talk-list-devices:
	$(TALK_RUN) list-devices

talk-probe-capture:
	$(TALK_RUN) probe-capture $(if $(SECONDS),--seconds $(SECONDS),)

talk-probe-playback:
	$(TALK_RUN) probe-playback
