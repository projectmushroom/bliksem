#!/bin/bash
# Counts audible clicks in what a player build really sends to PipeWire: hopping streams, then steady play with every
# CPU core busy. Fake stream, invisible sink, throwaway monitor: no key and no sound. Usage: dev/audio-glitches.sh BIN
set -u
export XDG_RUNTIME_DIR=/run/user/$(id -u); export HYPRLAND_INSTANCE_SIGNATURE=$(ls "$XDG_RUNTIME_DIR/hypr" | head -1)
export WAYLAND_DISPLAY=wayland-1 BLIKSEM_DATA=$(mktemp -d) PIPEWIRE_NODE=bliksem_test
ROOT=$(cd "$(dirname "$0")/.." && pwd); B=${1:?player binary}; OUT=bliksem-test; PORT=18766; WAV=$(mktemp --suffix=.wav); BURN=()
HOME_MONITOR=$(hyprctl -j monitors | python3 -c 'import json,sys;print(next((m["name"] for m in json.load(sys.stdin) if m["focused"]),""))')
cleanup() {
  "$B" quit >/dev/null 2>&1; sleep .5; pkill -x bliksem 2>/dev/null
  kill "${FAKE:-}" "${REC:-}" "${BURN[@]}" 2>/dev/null
  [ -n "${SINK:-}" ] && pactl unload-module "$SINK" 2>/dev/null
  hyprctl output remove $OUT >/dev/null 2>&1
  hyprctl eval "hl.dispatch(hl.dsp.focus({ monitor = '$HOME_MONITOR' }))" >/dev/null 2>&1
  rm -rf "$BLIKSEM_DATA" "$WAV"
}
trap cleanup EXIT
python3 "$ROOT/dev/fake-icecast.py" "$ROOT/assets/test-tone.mp3" $PORT >/dev/null 2>&1 & FAKE=$!
SINK=$(pactl load-module module-null-sink sink_name=bliksem_test sink_properties=node.description=bliksem_test)
hyprctl output create headless $OUT >/dev/null; sleep 1
hyprctl eval "hl.monitor({ output = '$OUT', mode = '2448x1600@60', position = 'auto', scale = 2 })" >/dev/null; sleep 1
WS=$(hyprctl -j monitors | python3 -c "import json,sys;print(next(m['activeWorkspace']['name'] for m in json.load(sys.stdin) if m['name']=='$OUT'))")
pw-record --target bliksem_test --format f32 --rate 44100 --channels 2 -P stream.capture.sink=true "$WAV" & REC=$!
hyprctl eval "hl.exec_cmd('sh -c \"BLIKSEM_DATA=$BLIKSEM_DATA PIPEWIRE_NODE=bliksem_test exec $B --stream http://127.0.0.1:$PORT/techno_hi > $BLIKSEM_DATA/stderr.log 2>&1\"', { workspace = '$WS silent' })" >/dev/null
sleep 6
"$B" next >/dev/null; sleep 5
HOP_START=$(date +%s.%N)
for _ in 1 2 3 4 5 6; do "$B" next >/dev/null; sleep 4; done
"$B" toggle >/dev/null; sleep 2; "$B" toggle >/dev/null; sleep 4
LOAD_START=$(date +%s.%N)
for _ in $(seq "$(nproc)"); do sh -c 'while :; do :; done' & BURN+=($!); done
sleep 30
kill "${BURN[@]}" 2>/dev/null; BURN=()
kill $REC; REC=; sleep .5
python3 - "$WAV" "$(python3 -c "print($LOAD_START-$HOP_START)")" <<'P'
import struct, sys
raw = open(sys.argv[1],'rb').read(); pcm = raw[raw.find(b'data')+8:]
n = len(pcm)//4; L = struct.unpack('<%df'%n, pcm[:n*4])[0::2]
# The tone is a quiet sine, so its second difference is tiny. Anything far above that is a step in the wave: a click.
clicks, last = [], -99999
for i in range(2, len(L)):
    if abs(L[i]-2*L[i-1]+L[i-2]) > 0.004 and i-last > 2205:
        clicks.append(i); last = i
split = len(L)-30*44100
sound = sum(max(abs(s) for s in L[i:i+4410]) > 0.01 for i in range(split, len(L)-4410, 4410)) / 10
print("recorded %.0fs | clicks while hopping and stopping: %d | clicks in 30s steady play with all cores busy: %d (%.0fs of those 30 had sound)" % (len(L)/44100, sum(c<split for c in clicks), sum(c>=split for c in clicks), sound))
P
cat "$BLIKSEM_DATA/bliksem.log" 2>/dev/null | grep -c audio_health | sed 's/^/log lines reporting audio problems: /'
grep audio_health "$BLIKSEM_DATA/bliksem.log" 2>/dev/null | tail -3
grep audio_open "$BLIKSEM_DATA/bliksem.log" 2>/dev/null | tail -1
