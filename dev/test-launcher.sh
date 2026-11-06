#!/bin/sh
# Tests packaging/omarchy/launcher with fake players and a fake notify-send. Runs anywhere, needs no audio or display.
root=$(cd "$(dirname "$0")/.." && pwd)
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
arch=$(uname -m); failed=0
mkdir -p "$t/fakebin" "$t/plugin/bin/$arch"
cp "$root/packaging/omarchy/launcher" "$t/plugin/bin/bliksem"
printf '#!/bin/sh\necho "$3|$4" >> "%s/notes"\n' "$t" > "$t/fakebin/notify-send"; chmod +x "$t/fakebin/notify-send"
player() { printf '#!/bin/sh\n%s\n' "$1" > "$t/plugin/bin/$arch/bliksem"; chmod +x "$t/plugin/bin/$arch/bliksem"; }
run() { rm -f "$t/notes"; PATH="$t/fakebin:$PATH" XDG_STATE_HOME="$t/state" "$t/plugin/bin/bliksem" "$@" >"$t/out" 2>/dev/null; code=$?; }
check() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got '$2', wanted '$3'"; failed=1; fi; }
noted() { [ -f "$t/notes" ] && echo yes || echo no; }

player 'echo "args:$*"'
run next; check "passes the word through" "$(cat "$t/out")" "args:next"
run window; check "a clean launch is quiet" "$(noted)" no; check "a clean launch exits 0" "$code" 0

player 'echo boom >&2; exit 101'
run window; check "a dead player notifies" "$(noted)" yes; check "and fails" "$code" 1
check "and logs why" "$(cat "$t/state/bliksem.log")" boom
grep -q 'exit 101' "$t/notes"; check "the note says how it died" "$?" 0

player 'exit 143'
run window; check "being told to stop is not a failure" "$(noted)" no

player 'exit 1'
run status; check "other words don't notify" "$(noted)" no; check "and keep their exit code" "$code" 1

rm "$t/plugin/bin/$arch/bliksem"
run window; check "a missing build notifies" "$(noted)" yes
grep -q "no player built for $arch" "$t/notes"; check "and names the architecture" "$?" 0

# Through a symlink, as ~/.local/bin/bliksem does.
player 'echo linked'; ln -s "$t/plugin/bin/bliksem" "$t/fakebin/bliksem"
check "works through a symlink" "$("$t/fakebin/bliksem" next)" linked
[ $failed = 0 ] && echo "ALL GOOD"; exit $failed
