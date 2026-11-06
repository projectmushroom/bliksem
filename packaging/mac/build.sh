#!/bin/bash
# Builds target/BLIKSEM.app, universal for Intel and Apple Silicon, ad-hoc signed. VERSION=x.y.z sets its version.
set -euo pipefail
cd "$(dirname "$0")/../.."
command -v cargo >/dev/null || export PATH="$HOME/.cargo/bin:$PATH"
rustup target add x86_64-apple-darwin aarch64-apple-darwin >/dev/null 2>&1
if [ ! -f Bliksem.icns ]; then
  T=$(mktemp -d)/Bliksem.iconset; mkdir -p "$T"
  swift packaging/mac/icon.swift "$T/icon_512x512@2x.png"
  for s in 16 32 128 256 512; do
    sips -z $s $s "$T/icon_512x512@2x.png" --out "$T/icon_${s}x${s}.png" >/dev/null
    [ $s -lt 512 ] && sips -z $((s*2)) $((s*2)) "$T/icon_512x512@2x.png" --out "$T/icon_${s}x${s}@2x.png" >/dev/null
  done
  iconutil -c icns "$T" -o Bliksem.icns
fi
cargo build --release --target x86_64-apple-darwin --target aarch64-apple-darwin
APP=target/BLIKSEM.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create target/x86_64-apple-darwin/release/bliksem target/aarch64-apple-darwin/release/bliksem -output "$APP/Contents/MacOS/Bliksem"
cp packaging/mac/Info.plist "$APP/Contents/"
[ -z "${VERSION:-}" ] || plutil -replace CFBundleShortVersionString -string "$VERSION" "$APP/Contents/Info.plist"
cp Bliksem.icns "$APP/Contents/Resources/"
codesign -s - -f "$APP" >/dev/null 2>&1
echo "built $PWD/$APP"
