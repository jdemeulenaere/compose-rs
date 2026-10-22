#!/bin/sh
# Builds a compose-rs library for Android (arm64) and copies it where Gradle packages it:
#   build-rust.sh <cargo package> <library name> <jniLibs directory>
# The NDK: $ANDROID_NDK_HOME if set; else $ANDROID_NDK_VERSION (the app's `ndkVersion`, which
# Gradle passes) under $ANDROID_HOME/ndk if installed; else the newest there, with a warning.
set -e
if [ $# -ne 3 ] || [ -z "$3" ]; then
    echo "usage: build-rust.sh <cargo package> <library name> <jniLibs directory>" >&2
    exit 2
fi
cd "$(dirname "$0")/.."
SDK=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}
if [ -n "$ANDROID_NDK_HOME" ]; then
    NDK=$ANDROID_NDK_HOME
elif [ -n "$ANDROID_NDK_VERSION" ] && [ -d "$SDK/ndk/$ANDROID_NDK_VERSION" ]; then
    NDK=$SDK/ndk/$ANDROID_NDK_VERSION
else
    NDK=$(ls -d "$SDK"/ndk/* 2>/dev/null | sort -V | tail -1)
    if [ -n "$ANDROID_NDK_VERSION" ] && [ -d "$NDK" ]; then
        echo "build-rust.sh: NDK $ANDROID_NDK_VERSION (the app's ndkVersion) isn't installed; linking with $(basename "$NDK"), the newest installed." >&2
        echo "build-rust.sh: Gradle strips the library with NDK $ANDROID_NDK_VERSION: install it with sdkmanager --install \"ndk;$ANDROID_NDK_VERSION\"" >&2
    fi
fi
if [ -n "$ANDROID_NDK_HOME" ] && [ ! -d "$NDK" ]; then
    echo "build-rust.sh: ANDROID_NDK_HOME ($ANDROID_NDK_HOME) isn't a directory" >&2
    exit 1
elif [ ! -d "$NDK" ]; then
    echo "build-rust.sh: no Android NDK (install ${ANDROID_NDK_VERSION:-one} with sdkmanager, or set ANDROID_NDK_HOME)" >&2
    exit 1
fi
HOST=$(ls "$NDK/toolchains/llvm/prebuilt" | head -1)
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$NDK/toolchains/llvm/prebuilt/$HOST/bin/aarch64-linux-android29-clang"
# Segments aligned to 16 KB pages, as Android 15+ devices with 16 KB pages require.
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
CARGO=$(command -v cargo || echo "$HOME/.cargo/bin/cargo")
"$CARGO" build --release --target aarch64-linux-android -p "$1"
# Only this build's library in the directory Gradle packages.
rm -rf "$3/arm64-v8a"
mkdir -p "$3/arm64-v8a"
cp "target/aarch64-linux-android/release/lib$2.so" "$3/arm64-v8a/"
