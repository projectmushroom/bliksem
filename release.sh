#!/bin/bash
# Usage: ./release.sh 0.1.0 → builds, makes .build/BLIKSEM-0.1.0.dmg, tags v0.1.0, publishes a GitHub release.
set -eo pipefail
cd "$(dirname "$0")"
V=${1:?version, e.g. 0.1.0}
[ -z "$(git status --porcelain)" ] || { echo "commit first, working tree is dirty"; exit 1; }
VERSION="$V" ./build.sh
STAGE=$(mktemp -d)
cp -R .build/BLIKSEM.app "$STAGE/"
ln -s /Applications "$STAGE/Applications"
DMG=".build/BLIKSEM-$V.dmg"
rm -f "$DMG"
hdiutil create -quiet -volname "BLIKSEM $V" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
git tag -a "v$V" -m "BLIKSEM $V"
git push -q origin "v$V"
gh release create "v$V" "$DMG" --title "BLIKSEM $V" \
  --notes "Drag BLIKSEM.app to Applications. It is ad-hoc signed, not notarized, so after downloading run: xattr -d com.apple.quarantine /Applications/BLIKSEM.app"
echo "released v$V: $DMG"
