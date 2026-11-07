#!/bin/bash
# Usage: packaging/mac/release.sh 1.0.0 builds target/BLIKSEM-1.0.0.dmg, tags v1.0.0 and publishes a GitHub release.
set -eo pipefail
cd "$(dirname "$0")/../.."
V=${1:?version, e.g. 1.0.0}
grep -q "^version = \"$V\"" Cargo.toml || { echo "Cargo.toml doesn't say version $V"; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "commit first, working tree is dirty"; exit 1; }
VERSION="$V" packaging/mac/build.sh
STAGE=$(mktemp -d)
cp -R target/BLIKSEM.app "$STAGE/"
ln -s /Applications "$STAGE/Applications"
DMG="target/BLIKSEM-$V.dmg"
rm -f "$DMG"
hdiutil create -quiet -volname "BLIKSEM $V" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
git tag -a "v$V" -m "BLIKSEM $V"
git push -q origin "v$V"
gh release create "v$V" "$DMG" --title "BLIKSEM $V" \
  --notes "Mac: drag BLIKSEM.app to Applications. It is ad-hoc signed, not notarized, so after downloading run: xattr -d com.apple.quarantine /Applications/BLIKSEM.app. Omarchy: clone the repo and run packaging/omarchy/install.sh."
echo "released v$V: $DMG"
