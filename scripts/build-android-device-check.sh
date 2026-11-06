#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
sdk=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}
android=$(find "$sdk/platforms" -name android.jar -print | sort -V | tail -1)
tools=$(find "$sdk/build-tools" -maxdepth 2 -name aapt2 -print | sort -V | tail -1)
tools=${tools%/*}
[ -n "$android" ] && [ -x "$tools/d8" ] || { echo 'Android SDK platform and build-tools are required' >&2; exit 1; }
mkdir -p "$root/target"
build=$(mktemp -d "$root/target/device-check.XXXXXX")
trap 'rm -rf -- "$build"' EXIT
javac --release 8 -Xlint:-options -cp "$android" -d "$build" "$root/scripts/android/devicecheck/DeviceCheck.java"
"$tools/d8" --min-api 26 --lib "$android" --output "$build" "$build"/com/droidpc/devicecheck/*.class
"$tools/aapt2" link -I "$android" --manifest "$root/scripts/android/devicecheck/AndroidManifest.xml" --min-sdk-version 26 --target-sdk-version 32 -o "$build/check.apk"
(cd "$build" && zip -q check.apk classes.dex)
"$tools/zipalign" -f 4 "$build/check.apk" "$build/aligned.apk"
if [ ! -f "$root/target/device-check.keystore" ]; then
    keytool -genkeypair -keystore "$root/target/device-check.keystore" -storepass android -keypass android -alias droidpc-check -dname CN=Droidpc -keyalg RSA -validity 3650 -noprompt
fi
"$tools/apksigner" sign --ks "$root/target/device-check.keystore" --ks-pass pass:android --out "$build/signed.apk" "$build/aligned.apk"
"$tools/apksigner" verify "$build/signed.apk"
mv "$build/signed.apk" "$root/target/droidpc-device-check.apk"
