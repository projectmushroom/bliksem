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
# Universal binary so the DMG runs natively on Intel and Apple Silicon.
swift build -c release --arch arm64 --arch x86_64 2>&1 | grep -E "error|Build complete" | tail -5
OUT=.build/apple/Products/Release
# SwiftPM does not compile Metal, so the CRT shader becomes default.metallib by hand.
xcrun -sdk macosx metal -c Shaders/crt.metal -o .build/crt.air
xcrun -sdk macosx metallib .build/crt.air -o .build/default.metallib
APP=.build/BLIKSEM.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$OUT/Bliksem" "$APP/Contents/MacOS/"
R="$OUT/Bliksem_Bliksem.bundle"; [ -d "$R/Contents/Resources" ] && R="$R/Contents/Resources"
rm -f "$R/crt.metal" "$OUT/Bliksem_Bliksem.bundle/default.metallib"
cp .build/default.metallib "$R/"
cp -R "$OUT/Bliksem_Bliksem.bundle" "$APP/Contents/Resources/"
cp Info.plist "$APP/Contents/"
[ -z "$VERSION" ] || plutil -replace CFBundleShortVersionString -string "$VERSION" "$APP/Contents/Info.plist"
cp Bliksem.icns "$APP/Contents/Resources/"
codesign -s - -f "$APP" >/dev/null 2>&1
echo "built $PWD/$APP"
