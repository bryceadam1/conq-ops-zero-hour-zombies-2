#!/usr/bin/env bash
# In-game tests for Conquer Ops on macOS: the counterpart of autotest.ps1 and inputtest.ps1.
#
#   scripts/autotest-mac.sh            autopilot test (ZHBO_AUTOPILOT=1, scripted inside the mod)
#   scripts/autotest-mac.sh --input    real-input test (ZHBO_AUTOPILOT=manual; holds W, moves the mouse)
#   scripts/autotest-mac.sh --equipment
#                                      C4, Trophy System and Black Hat against an enemy drone and
#                                      rocket troopers (two runs: C4 + Trophy, C4 + Black Hat)
#   scripts/autotest-mac.sh --rts      the General under RTS control: rifle, the lethal and the pistol
#                                      picked from the command bar, ammo counted, then first person
#   scripts/autotest-mac.sh --rts-dry  the same with no spare rifle rounds, then no spare pistol rounds:
#                                      the empty gun hands over to the other and he keeps firing
#   scripts/autotest-mac.sh --showcase [object]
#                                      close-up captures of a General (idle, walk, death), no checks
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
[[ "${1:-}" == "--showcase" ]] && mode=showcase
[[ "${1:-}" == "--equipment" ]] && mode=equipment
[[ "${1:-}" == "--rts" ]] && mode=rts
[[ "${1:-}" == "--rts-dry" ]] && mode=rts-dry

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

export GGC_BGFX_SCREENSHOT_AFTER=${GGC_BGFX_SCREENSHOT_AFTER:-60} GGC_BGFX_SCREENSHOT_INTERVAL=${GGC_BGFX_SCREENSHOT_INTERVAL:-150} GGC_BGFX_SCREENSHOT_PATH="$out/shot.bmp"
if [[ $mode == showcase ]]; then
    export ZHBO_AUTOPILOT=showcase GGC_AUTO_EXIT_SECONDS=30 GGC_BGFX_SCREENSHOT_INTERVAL=45
    [[ -n "${2:-}" ]] && export ZHBO_SHOWCASE_OBJECT="$2"
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "showcase General" 90 || { echo "the showcase never started; see $log" >&2; exit 1; }
    for _ in $(seq 45); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    grep -E "showcase" "$out/ConquerOps.log"
    echo "captures: $out"
    exit 0
fi
if [[ $mode == equipment ]]; then
    export ZHBO_AUTOPILOT=equipment GGC_AUTO_EXIT_SECONDS=40
    ok=0
    check() { # log, name, pattern
        if grep -qE "$3" "$1"; then printf '%-28s PASS\n' "$2"; else printf '%-28s FAIL\n' "$2"; ok=1; fi
    }
    for tactical in trophy_system_mp pda_hack_mp; do
        rm -f "$log"
        ZHBO_LOADOUT="lethal=satchel_charge_mp;tactical=$tactical" "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
        wait_for_log "entered first-person" 120 || { echo "the General never entered first person; see $log" >&2; exit 1; }
        for _ in $(seq 60); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
        pkill -f "$dist/generalszh" 2>/dev/null || true
        cp "$log" "$out/ConquerOps-$tactical.log"
        l="$out/ConquerOps-$tactical.log"
        echo "== C4 + $tactical"
        check "$l" "C4 thrown and stuck" "put down equipment \(C4\)"
        check "$l" "C4 detonated" "set off their C4"
        if [[ $tactical == trophy_system_mp ]]; then
            check "$l" "Trophy System planted" "put down equipment \(Trophy\)"
            check "$l" "Trophy shot down a rocket" "Trophy System shot down a projectile"
        else
            check "$l" "Black Hat hack" "hacked with the Black Hat"
            check "$l" "drone changed sides" "captured by General"
        fi
    done
    finish
    trap - EXIT
    echo "screenshots and logs: $out"
    exit $ok
fi
if [[ $mode == rts-dry ]]; then
    export ZHBO_AUTOPILOT=rts GGC_AUTO_EXIT_SECONDS=35
    ok=0
    # The first line matching $3 comes after the first matching $2 in log $1.
    after() { awk -v a="$2" -v b="$3" '$0 ~ a && !s { s = 1; next } s && $0 ~ b { f = 1 } END { exit !f }' "$1"; }
    check() { # name, command...
        local name=$1; shift
        if "$@"; then printf '%-28s PASS\n' "$name"; else printf '%-28s FAIL\n' "$name"; ok=1; fi
    }
    for dry in primary secondary; do
        rm -f "$log"
        ZHBO_LOADOUT="${dry}_reserve=0" "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
        wait_for_log "autopilot: taking control" 90 || { echo "the script never reached first person; see $log" >&2; exit 1; }
        for _ in $(seq 40); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
        pkill -f "$dist/generalszh" 2>/dev/null || true
        l="$out/ConquerOps-$dry.log"
        cp "$log" "$l"
        if [[ $dry == primary ]]; then
            echo "== no spare rifle rounds"
            check "rifle ran dry: pistol" after "$l" "fired slot 0 [(]0/0 left[)]" "locked to weapon slot 1"
            check "  ...attack goes on" after "$l" "locked to weapon slot 1" "keeps attacking"
            check "  ...pistol keeps firing" after "$l" "locked to weapon slot 1" "fired slot 1"
        else
            echo "== no spare pistol rounds"
            check "pistol ran dry: rifle" after "$l" "fired slot 1 [(]0/0 left[)]" "locked to weapon slot 0"
            check "  ...rifle keeps firing" after "$l" "fired slot 1 [(]0/0 left[)]" "fired slot 0"
        fi
    done
    finish
    trap - EXIT
    echo "screenshots and logs: $out"
    exit $ok
fi
if [[ $mode == rts ]]; then
    export ZHBO_AUTOPILOT=rts GGC_AUTO_EXIT_SECONDS=35
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "autopilot: taking control" 90 || { echo "the script never reached first person; see $log" >&2; exit 1; }
    for _ in $(seq 40); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    ok=0
    check() { # name, pattern
        if grep -qE "$2" "$out/ConquerOps.log"; then printf '%-28s PASS\n' "$1"; else printf '%-28s FAIL\n' "$1"; ok=1; fi
    }
    check "rifle rounds counted" "fired slot 0 \([0-9]+/[0-9]+ left\)"
    check "lethal picked" "uses weapon slot 2"
    check "lethal thrown" "fired slot 2 \(0/0 left\)"
    check "back to the rifle" "locked to weapon slot 0"
    check "pistol picked" "uses weapon slot 1"
    check "pistol rounds counted" "fired slot 1"
    check "first person keeps pistol" "entered first-person control"
    check "  ...holding the pistol" "Tac-45 [0-9]+\|"
    echo "screenshots and log: $out"
    exit $ok
fi
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
    check "swapped weapons" "swapped weapons"
    check "meleed" "meleed"
    check "threw a lethal" "threw a lethal"
    check "threw a tactical" "threw a tactical"
    check "grenade went off" "grenade went off"
    check "grenade launcher" "toggled the weapon's alt mode"
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
