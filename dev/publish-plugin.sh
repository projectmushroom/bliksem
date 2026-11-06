#!/bin/bash
# Fills a clone of the public omarchy-bliksem repo with the plugin and a Linux binary built on an Omarchy box.
# Usage: dev/publish-plugin.sh VERSION CLONE_DIR [BUILD_HOST]. Commit and push the clone yourself after reading the diff.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd); V=${1:?version}; OUT=${2:?clone of omarchy-bliksem}; HOST=${3:-five6one@omarchy-mbp-15}
SRC=bliksem-publish # relative to the home folder on the build box
tar --no-xattrs --exclude=target --exclude=.git --exclude=.build --exclude='*.icns' -czf - -C "$ROOT" . | ssh -o BatchMode=yes "$HOST" "mkdir -p $SRC && tar xzf - -C $SRC"
# Build paths would otherwise put the build machine's home folder in a public binary.
ssh -o BatchMode=yes "$HOST" "cd $SRC && export PATH=\$HOME/.cargo/bin:\$PATH && RUSTFLAGS=\"--remap-path-prefix=\$HOME=/build\" cargo build --release 2>&1 | tail -1 && cargo metadata --format-version 1 --filter-platform x86_64-unknown-linux-gnu" | sed -n '/^{/,$p' > /tmp/bliksem-metadata.json
mkdir -p "$OUT/bin"
scp -q -o BatchMode=yes "$HOST:$SRC/target/release/bliksem" "$OUT/bin/bliksem"
chmod 755 "$OUT/bin/bliksem"
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
ssh -o BatchMode=yes "$HOST" "rm -rf $SRC"
echo "filled $OUT at version $V"
