#!/usr/bin/env bash
# In-game tests for Conquer Ops on macOS: the counterpart of autotest.ps1 and inputtest.ps1.
#
#   scripts/autotest-mac.sh            autopilot test (ZHBO_AUTOPILOT=1, scripted inside the mod)
#   scripts/autotest-mac.sh --input    real-input test (ZHBO_AUTOPILOT=manual; holds W, moves the mouse)
#
# Both load a saved skirmish instead of clicking through the menus. Make the save once: start
#   conquerops -win -xres 1280 -yres 720 -quickstart -saveatframe 90 -saveto zhbo-autotest.sav
# then click Solo Play > Skirmish > Play Game. The game saves 3 seconds in and quits.
#
# Screenshots come from the engine's own capture (GGC_BGFX_SCREENSHOT_*), so the autopilot test
# needs no permissions. The input test posts real key and mouse events and needs Accessibility
# permission for the app running this script (System Settings > Privacy & Security).
#
# Environment: ZHBO_ZH_DIR and ZHBO_BO2_DIR as for the launcher; OUT for the results folder.
set -euo pipefail

mode=autopilot
[[ "${1:-}" == "--input" ]] && mode=input

repo="$(cd "$(dirname "$0")/.." && pwd)"
dist="$HOME/Library/Application Support/ConquerOps/dist"
userdata="$HOME/Library/Application Support/Command and Conquer Generals Zero Hour Data"
log="$userdata/ConquerOps.log"
out="${OUT:-${TMPDIR:-/tmp}/ConquerOps-$mode}"
save=zhbo-autotest.sav
mkdir -p "$out"
rm -f "$out"/shot.* "$out"/*.png

if [[ ! -f "$userdata/Save/$save" ]]; then
    echo "missing $userdata/Save/$save; see the top of this script for how to make it" >&2
    exit 2
fi

pkill -f "$dist/generalszh" 2>/dev/null || true
# A stale log from the previous run would satisfy the waits below too early.
rm -f "$log"

game_pid() { pgrep -f "$dist/generalszh" | head -1; }
wait_for_log() { # pattern, seconds
    for _ in $(seq "$2"); do
        grep -q "$1" "$log" 2>/dev/null && return 0
        sleep 1
    done
    return 1
}
finish() {
    pkill -f "$dist/generalszh" 2>/dev/null || true
    cp "$log" "$out/" 2>/dev/null || true
    for f in "$out"/shot.*.bmp; do
        [[ -f "$f" ]] && sips -s format png "$f" --out "${f%.bmp}.png" >/dev/null && rm "$f"
    done
}
trap finish EXIT

export GGC_BGFX_SCREENSHOT_AFTER=60 GGC_BGFX_SCREENSHOT_INTERVAL=150 GGC_BGFX_SCREENSHOT_PATH="$out/shot.bmp"
if [[ $mode == autopilot ]]; then
    # Autopilot timeline: enters first person about 3 s in, field computer about 12 s after that.
    export ZHBO_AUTOPILOT=1 GGC_AUTO_EXIT_SECONDS=45
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "entered first-person" 90 || { echo "the General never entered first person; see $log" >&2; exit 1; }
    for _ in $(seq 60); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    ok=0
    check() { # name, pattern
        if grep -qE "$2" "$out/ConquerOps.log"; then printf '%-22s PASS\n' "$1"; else printf '%-22s FAIL\n' "$1"; ok=1; fi
    }
    check "entered first person" "entered first-person control"
    check "moved" "pos \("
    check "killed a target" "killed a target"
    check "field computer" "opened the field computer"
    check "returned to RTS" "returned General [0-9]+ to RTS control"
    check "re-entered" "taking control again"
    check "took enemy fire" "General took [0-9]+ damage"
    echo "screenshots and log: $out"
    exit $ok
fi

# Real-input test.
drive="$out/drive"
[[ "$drive" -nt "$repo/scripts/macos/drive.swift" ]] || swiftc -O "$repo/scripts/macos/drive.swift" -o "$drive"
export ZHBO_AUTOPILOT=manual GGC_AUTO_EXIT_SECONDS=60
"$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
wait_for_log "entered first-person" 90 || { echo "the General never entered first person; see $log" >&2; exit 1; }
pid="$(game_pid)"
"$drive" "focus:$pid" sleep:1500
front="$(osascript -e 'tell application "System Events" to get unix id of first process whose frontmost is true')"
if [[ "$front" != "$pid" ]]; then
    echo "the game is not the frontmost app; not sending input" >&2
    exit 1
fi
screencapture -x "$out/window-before.png" 2>/dev/null || true
# W for 2 s, then turn right with relative mouse motion. (macOS virtual key 13 is W.)
"$drive" down:13 sleep:2000 up:13 sleep:1200 look:12,0,40 sleep:1500
screencapture -x "$out/window-after.png" 2>/dev/null || true
finish
trap - EXIT

poses="$(grep "pos (" "$out/ConquerOps.log" || true)"
first="$(head -1 <<<"$poses")"
last="$(tail -1 <<<"$poses")"
ok=0
if grep -q "keys held: \[W\]" "$out/ConquerOps.log"; then echo "W reached the game    PASS"; else echo "W reached the game    FAIL"; ok=1; fi
if [[ "$(cut -d' ' -f5-6 <<<"$first")" != "$(cut -d' ' -f5-6 <<<"$last")" ]]; then echo "moved                 PASS"; else echo "moved                 FAIL"; ok=1; fi
if [[ "$(awk '{print $9}' <<<"$first")" != "$(awk '{print $9}' <<<"$last")" ]]; then echo "turned                PASS"; else echo "turned                FAIL"; ok=1; fi
echo "first pose: $first"
echo "last pose:  $last"
echo "screenshots and log: $out"
exit $ok
