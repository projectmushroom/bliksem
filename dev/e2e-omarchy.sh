#!/bin/bash
# Full chain on an Omarchy box with no key and no sound: real app, real decode, real PipeWire into an invisible sink,
# real D-Bus and real bar, on a throwaway monitor. Everything it creates is removed on exit, and it works while locked.
# Install the plugin first, with `omarchy plugin add` or packaging/omarchy/install.sh. Writes frames to /tmp/bliksem-e2e-*.png.
set -u
export XDG_RUNTIME_DIR=/run/user/$(id -u) OMARCHY_PATH=${OMARCHY_PATH:-/usr/share/omarchy}
export HYPRLAND_INSTANCE_SIGNATURE=$(ls "$XDG_RUNTIME_DIR/hypr" | head -1)
export WAYLAND_DISPLAY=wayland-1 PATH=$OMARCHY_PATH/bin:$PATH
export DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}
ROOT=$(cd "$(dirname "$0")/.." && pwd); B=${BLIKSEM_BIN:-$HOME/.config/omarchy/plugins/projectmushroom.bliksem/bin/bliksem}; OUT=bliksem-test; PORT=18765
export BLIKSEM_DATA=$(mktemp -d) PIPEWIRE_NODE=bliksem_test
CAP=/tmp/bliksem-e2e-cap.png; FAILED=0
check() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got '$2', wanted '$3'"; FAILED=1; fi; }
field() { "$B" status | python3 -c "import json,sys;print(json.load(sys.stdin)['$1'])"; }
mpris() { busctl --user get-property org.mpris.MediaPlayer2.bliksem /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player "$1" 2>&1; }
windows() { hyprctl -j clients | python3 -c 'import json,sys;print(sum(c["title"]=="BLIKSEM" for c in json.load(sys.stdin)))'; }
bar_width() { omarchy-shell shell debugBarGeometry | python3 -c 'import json,sys;print(max([w["width"] for w in json.load(sys.stdin) if w["id"]=="projectmushroom.bliksem"] or [0]))'; }
cleanup() {
  "$B" quit >/dev/null 2>&1; sleep .5; pkill -x bliksem 2>/dev/null
  kill "${FAKE:-}" "${REC:-}" 2>/dev/null
  [ -n "${SINK:-}" ] && pactl unload-module "$SINK" 2>/dev/null
  hyprctl output remove $OUT >/dev/null 2>&1
  hyprctl eval "hl.dispatch(hl.dsp.focus({ monitor = 'eDP-1' }))" >/dev/null 2>&1
  rm -rf "$BLIKSEM_DATA" /tmp/bliksem-e2e.wav "$CAP"
}
trap cleanup EXIT
python3 "$ROOT/dev/fake-icecast.py" "$ROOT/assets/test-tone.mp3" $PORT & FAKE=$!
SINK=$(pactl load-module module-null-sink sink_name=bliksem_test sink_properties=node.description=bliksem_test)
default_before=$(pactl get-default-sink)
hyprctl output create headless $OUT >/dev/null; sleep 1
hyprctl eval 'hl.monitor({ output = "bliksem-test", mode = "2448x1600@60", position = "auto", scale = 2 })' >/dev/null; sleep 1
WS=$(hyprctl -j monitors | python3 -c 'import json,sys;print(next(m["activeWorkspace"]["name"] for m in json.load(sys.stdin) if m["name"]=="bliksem-test"))')
pw-record --target bliksem_test --format f32 --rate 44100 --channels 2 -P stream.capture.sink=true /tmp/bliksem-e2e.wav & REC=$!
hyprctl eval "hl.exec_cmd('sh -c \"BLIKSEM_DATA=$BLIKSEM_DATA PIPEWIRE_NODE=bliksem_test exec $B --stream http://127.0.0.1:$PORT/techno_hi --capture $CAP > /tmp/bliksem-e2e.log 2>&1\"', { workspace = '$WS silent' })" >/dev/null
sleep 6
check "window opened" "$(windows)" 1
check "starts idle" "$(field playing)" False
width_idle=$(bar_width)
"$B" next >/dev/null; sleep 4
check "plays from next" "$(field playing)" True
check "MPRIS says playing" "$(mpris PlaybackStatus)" 's "Playing"'
sleep 3; width_playing=$(bar_width)
if [ "$width_playing" -gt "$width_idle" ]; then echo "ok    bar shows the track ($width_idle -> $width_playing px)"; else echo "FAIL  bar width $width_idle -> $width_playing"; FAILED=1; fi
sleep 3; cp "$CAP" /tmp/bliksem-e2e-playing.png
A=$(hyprctl -j clients | python3 -c 'import json,sys;print(next((c["address"] for c in json.load(sys.stdin) if c["title"]=="BLIKSEM"),""))')
hyprctl eval "hl.dispatch(hl.dsp.window.close({ window = 'address:$A' }))" >/dev/null; sleep 2
check "closing the window keeps the player" "$(pgrep -xc bliksem)" 1
check "no window left" "$(windows)" 0
check "music keeps playing" "$(field playing)" True
sleep 5
"$B" show >/dev/null; sleep 3
check "bar brings the window back" "$(windows)" 1
check "still one player" "$(pgrep -xc bliksem)" 1
sleep 3; cp "$CAP" /tmp/bliksem-e2e-reopened.png 2>/dev/null
busctl --user call org.mpris.MediaPlayer2.bliksem /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player Pause; sleep 1
check "media key stops it" "$(field playing)" False
check "default output untouched" "$(pactl get-default-sink)" "$default_before"
sleep 1; kill $REC 2>/dev/null; REC=; sleep .4
python3 - <<'P'
import math, struct
raw = open('/tmp/bliksem-e2e.wav','rb').read(); pcm = raw[raw.find(b'data')+8:]
n = len(pcm)//4; v = struct.unpack('<%df'%n, pcm[:n*4]); L, R = v[0::2], v[1::2]
loud = sum(1 for i in range(0, len(L)-4410, 4410) if max(abs(s) for s in L[i:i+4410]) > 0.01)
def tone(x, f):
    k = 2*math.cos(2*math.pi*f/44100); a = b = 0.0
    for s in x: a, b = s+k*a-b, a
    return math.sqrt(max(a*a+b*b-k*a*b, 0))*2/len(x)
seg = [i for i in range(0, len(L)-44100, 44100) if max(abs(s) for s in L[i:i+4410]) > 0.01]
i = seg[len(seg)//2]; l, r = L[i:i+44100], R[i:i+44100]
print("ok   " if tone(l,220) > 0.03 and tone(r,330) > 0.03 and tone(l,330) < 0.005 else "FAIL ", "real audio: %.1fs with sound, L 220Hz=%.3f R 330Hz=%.3f" % (loud/10, tone(l,220), tone(r,330)))
P
"$B" status >/dev/null
[ $FAILED = 0 ] && echo "ALL GOOD" || echo "SOME FAILED"
exit $FAILED
