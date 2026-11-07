#!/bin/bash
# Fills a clone of the public omarchy-bliksem repo with the plugin and one player per architecture, each built natively.
# Usage: dev/publish-plugin.sh VERSION CLONE_DIR [BUILD_HOST...]. Commit and push the clone yourself after reading the diff.
# The Apple Silicon machine uses 16K memory pages, so build there rather than cross-compiling from x86.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd); V=${1:?version}; OUT=${2:?clone of omarchy-bliksem}; shift 2
grep -q "^version = \"$V\"" "$ROOT/Cargo.toml" || { echo "Cargo.toml doesn't say version $V: the app compares itself against the plugin's version"; exit 1; }
HOSTS=("$@"); [ ${#HOSTS[@]} -gt 0 ] || HOSTS=(five6one@omarchy-mbp-15 projectmushroom@projectmushroom)
SSH=(ssh -i "$HOME/.ssh/bliksem_build" -o IdentitiesOnly=yes -o BatchMode=yes)
SRC=bliksem-publish # relative to the home folder on the build box
rm -rf "$OUT/bin"; mkdir -p "$OUT/bin"
for HOST in "${HOSTS[@]}"; do
  ARCH=$("${SSH[@]}" "$HOST" uname -m)
  tar --no-xattrs --exclude=target --exclude=.git --exclude=.build --exclude='*.icns' -czf - -C "$ROOT" . | "${SSH[@]}" "$HOST" "mkdir -p $SRC && tar xzf - -C $SRC"
  # Build paths would otherwise put the build machine's home folder in a public binary.
  "${SSH[@]}" "$HOST" "cd $SRC && export PATH=\$HOME/.cargo/bin:\$PATH && RUSTFLAGS=\"--remap-path-prefix=\$HOME=/build\" cargo build --release 2>&1 | tail -1"
  "${SSH[@]}" "$HOST" "cd $SRC && export PATH=\$HOME/.cargo/bin:\$PATH && cargo metadata --format-version 1 --filter-platform $ARCH-unknown-linux-gnu" > "/tmp/bliksem-metadata.json"
  mkdir -p "$OUT/bin/$ARCH"
  "${SSH[@]}" "$HOST" "cat $SRC/target/release/bliksem" > "$OUT/bin/$ARCH/bliksem"
  chmod 755 "$OUT/bin/$ARCH/bliksem"
  "${SSH[@]}" "$HOST" "rm -rf $SRC"
done
install -m 755 "$ROOT/packaging/omarchy/launcher" "$OUT/bin/bliksem"
cp "$ROOT"/packaging/omarchy/projectmushroom.bliksem/* "$OUT"/
python3 - "$OUT/manifest.json" "$V" <<'P'
import json, sys
m = json.load(open(sys.argv[1])); m["version"] = sys.argv[2]
json.dump(m, open(sys.argv[1], "w"), indent=2); open(sys.argv[1], "a").write("\n")
P
cp "$ROOT/assets/FONT-LICENSE.txt" "$OUT/FONT-LICENSE.txt"
python3 - /tmp/bliksem-metadata.json > "$OUT/THIRD-PARTY.txt" <<'P'
import json, sys
print("Open source packages compiled into bin/bliksem. Their source is on crates.io.\n")
for p in sorted(json.load(open(sys.argv[1]))["packages"], key=lambda p: p["name"]):
    if p["name"] != "bliksem":
        print(f'{p["name"]} {p["version"]}: {p["license"] or "see the package"}')
P
rm -f /tmp/bliksem-metadata.json
echo "filled $OUT at version $V"
