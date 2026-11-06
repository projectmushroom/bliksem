#!/bin/bash
# Installs BLIKSEM for this user on Omarchy: program, app launcher entry, bar bolt. `./install.sh uninstall` reverses it.
# Needs no root. Your listen key and history in ~/.config/bliksem are never touched.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
BIN=$HOME/.local/bin/bliksem
PLUGIN=$HOME/.config/omarchy/plugins/projectmushroom.bliksem
DESKTOP=$HOME/.local/share/applications/bliksem.desktop
ICON=$HOME/.local/share/icons/bliksem.svg
PLUGIN_ID=projectmushroom.bliksem
export OMARCHY_PATH=${OMARCHY_PATH:-/usr/share/omarchy}
export PATH=$OMARCHY_PATH/bin:$PATH

if [ "${1:-}" = uninstall ]; then
  "$BIN" quit 2>/dev/null || true
  omarchy-plugin-disable "$PLUGIN_ID" >/dev/null 2>&1 || true
  rm -rf "$PLUGIN"
  omarchy-shell -q shell rescanPlugins
  rm -f "$BIN" "$DESKTOP" "$ICON"
  echo "gone. your key and history are still in ~/.config/bliksem"
  exit 0
fi

command -v cargo >/dev/null || export PATH="$HOME/.cargo/bin:$PATH"
(cd "$ROOT" && cargo build --release)
mkdir -p "$(dirname "$BIN")" "$(dirname "$DESKTOP")" "$(dirname "$ICON")" "$PLUGIN"
install -m 644 "$ROOT/assets/bliksem.svg" "$ICON"
cp "$ROOT"/packaging/omarchy/projectmushroom.bliksem/* "$PLUGIN"/
ARCH=$(uname -m)
mkdir -p "$PLUGIN/bin/$ARCH"
install -m 755 "$ROOT/target/release/bliksem" "$PLUGIN/bin/$ARCH/bliksem.new" && mv "$PLUGIN/bin/$ARCH/bliksem.new" "$PLUGIN/bin/$ARCH/bliksem"
install -m 755 "$ROOT/packaging/omarchy/launcher" "$PLUGIN/bin/bliksem"
cat > "$DESKTOP" <<D
[Desktop Entry]
Type=Application
Name=BLIKSEM
Comment=DI.fm on an amber CRT
Exec=$BIN
Icon=$ICON
Categories=AudioVideo;Audio;Player;
StartupWMClass=bliksem
D
# The command line uses the same launcher as the bar. Beside the plugin, not in it: plugins can't hold symlinks.
ln -sf "$PLUGIN/bin/bliksem" "$BIN"
omarchy-plugin-validate "$PLUGIN"
omarchy-shell -q shell rescanPlugins
sleep 1
omarchy-plugin-enable "$PLUGIN_ID" --section right >/dev/null
echo "installed. bolt is on the bar; click it, or run: bliksem"
