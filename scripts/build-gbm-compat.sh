#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
sdk=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}
ndk=${ANDROID_NDK_HOME:-$(find "$sdk/ndk" -mindepth 1 -maxdepth 1 -type d -print | sort -V | tail -1)}
compiler=$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin
[ -x "$compiler/i686-linux-android26-clang" ] || { echo 'An Android NDK is required for the camera buffer compatibility library' >&2; exit 1; }
mkdir -p "$root/target/gbm-compat/lib" "$root/target/gbm-compat/lib64"
for abi in lib lib64; do
    case "$abi" in lib) target=i686-linux-android26 ;; lib64) target=x86_64-linux-android26 ;; esac
    output=$root/target/gbm-compat/$abi/libdroidpc_gbm_wrapper.so
    "$compiler/$target-clang" -std=c11 -O2 -Wall -Wextra -Werror -fPIC -fvisibility=hidden -shared \
        -Wl,-soname,libdroidpc_gbm_wrapper.so -Wl,-z,defs \
        "$root/scripts/native/gbm_compat.c" -ldl -o "$output.tmp"
    mv "$output.tmp" "$output"
done
