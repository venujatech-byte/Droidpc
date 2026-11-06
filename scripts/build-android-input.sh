#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
sdk=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}
android=$(find "$sdk/platforms" -name android.jar -print | sort -V | tail -1)
d8=$(find "$sdk/build-tools" -maxdepth 2 -name d8 -print | sort -V | tail -1)
[ -n "$android" ] && [ -n "$d8" ] || { echo 'Android SDK platform and build-tools are required' >&2; exit 1; }
mkdir -p "$root/target"
build=$(mktemp -d "$root/target/android-input.XXXXXX")
trap 'rm -rf -- "$build"' EXIT
javac --release 8 -Xlint:-options -cp "$android" -d "$build" "$root/scripts/android/DroidpcInput.java"
"$d8" --min-api 26 --lib "$android" --output "$build" "$build"/*.class
jar cf "$build/input.jar" -C "$build" classes.dex
mv "$build/input.jar" "$root/target/droidpc-input.jar"
