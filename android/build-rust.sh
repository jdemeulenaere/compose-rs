#!/bin/sh
# Builds a compose-rs library for Android (arm64) and copies it where Gradle packages it:
#   build-rust.sh <cargo package> <library name> <jniLibs directory>
# The NDK is found in $ANDROID_NDK_HOME, or the newest in $ANDROID_HOME/ndk.
set -e
cd "$(dirname "$0")/.."
SDK=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}
NDK=${ANDROID_NDK_HOME:-$(ls -d "$SDK"/ndk/* 2>/dev/null | sort -V | tail -1)}
if [ ! -d "$NDK" ]; then
    echo "build-rust.sh: no Android NDK (install one with sdkmanager, or set ANDROID_NDK_HOME)" >&2
    exit 1
fi
HOST=$(ls "$NDK/toolchains/llvm/prebuilt" | head -1)
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$NDK/toolchains/llvm/prebuilt/$HOST/bin/aarch64-linux-android29-clang"
CARGO=$(command -v cargo || echo "$HOME/.cargo/bin/cargo")
"$CARGO" build --release --target aarch64-linux-android -p "$1"
mkdir -p "$3/arm64-v8a"
cp "target/aarch64-linux-android/release/lib$2.so" "$3/arm64-v8a/"
