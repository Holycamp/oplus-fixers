#!/bin/bash
# Cross-compile touch-hal for aarch64 Android (API 30). Used by the GitHub workflow.
set -euo pipefail
cd "$(dirname "$0")"
NDK="${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME}"
TC="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
export CC_aarch64_linux_android="$TC/aarch64-linux-android30-clang"
export AR_aarch64_linux_android="$TC/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TC/aarch64-linux-android30-clang"
cargo build --release --target aarch64-linux-android
mkdir -p out
cp target/aarch64-linux-android/release/touch-hal-service out/vendor.oplus.hardware.touch-service
file out/vendor.oplus.hardware.touch-service
