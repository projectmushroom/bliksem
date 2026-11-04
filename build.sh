#!/bin/bash
# Builds BLIKSEM.app into .build/ and ad-hoc signs it. Run ./build.sh then open .build/BLIKSEM.app
set -eo pipefail
cd "$(dirname "$0")"
if [ ! -f Bliksem.icns ]; then
  T=$(mktemp -d)/Bliksem.iconset; mkdir -p "$T"
  swift icon.swift "$T/icon_512x512@2x.png"
  for s in 16 32 128 256 512; do
    sips -z $s $s "$T/icon_512x512@2x.png" --out "$T/icon_${s}x${s}.png" >/dev/null
    [ $s -lt 512 ] && sips -z $((s*2)) $((s*2)) "$T/icon_512x512@2x.png" --out "$T/icon_${s}x${s}@2x.png" >/dev/null
  done
  iconutil -c icns "$T" -o Bliksem.icns
fi
swift build -c release 2>&1 | grep -E "error|Build complete" | tail -5
# SwiftPM does not compile Metal, so the CRT shader becomes default.metallib by hand.
xcrun -sdk macosx metal -c Shaders/crt.metal -o .build/crt.air
xcrun -sdk macosx metallib .build/crt.air -o .build/default.metallib
APP=.build/BLIKSEM.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp .build/release/Bliksem "$APP/Contents/MacOS/"
rm -f .build/release/Bliksem_Bliksem.bundle/crt.metal
cp .build/default.metallib .build/release/Bliksem_Bliksem.bundle/
cp -R .build/release/Bliksem_Bliksem.bundle "$APP/Contents/Resources/"
cp Info.plist "$APP/Contents/"
cp Bliksem.icns "$APP/Contents/Resources/"
codesign -s - -f "$APP" >/dev/null 2>&1
echo "built $PWD/$APP"
