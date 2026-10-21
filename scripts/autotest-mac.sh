#!/usr/bin/env bash
# In-game tests for Conquer Ops on macOS: the counterpart of autotest.ps1 and inputtest.ps1.
#
#   scripts/autotest-mac.sh            autopilot test (ZHBO_AUTOPILOT=1, scripted inside the mod; the
#                                      profile is seeded with a GL custom for the alt-mode check)
#   scripts/autotest-mac.sh --input    real-input test (ZHBO_AUTOPILOT=manual; holds W, moves the mouse)
#   scripts/autotest-mac.sh --equipment
#                                      C4, Trophy System and Black Hat against an enemy drone and
#                                      rocket troopers (two runs: C4 + Trophy, C4 + Black Hat)
#   scripts/autotest-mac.sh --rts      the General under RTS control: his class guns armed, the rifle's
#                                      rounds counted, the command bar's orders (the lethal at a rebel,
#                                      a reload, the pistol, the knife at a close rebel), then first person
#   scripts/autotest-mac.sh --rts-dry  the same with no spare rifle rounds, then no spare pistol rounds:
#                                      the empty gun hands over to the other and he keeps firing
#   scripts/autotest-mac.sh --build    the General builds: his Build menu, a structure ordered from it,
#                                      Back; it must take about 1 / GENERAL_BUILD_RATE of a dozer's time.
#                                      Then in first person: the hammer places one (turned with the
#                                      wheel), holding F builds it, and repairs it once damaged. Back
#                                      overhead he is an idle worker, and repairs it again when ordered.
#   scripts/autotest-mac.sh --classpick
#                                      the pick-only in-match class screen: the script seeds a custom
#                                      class ("AB") in the profile, picks it by its row key, then forces
#                                      the screen open after re-taking control and Escape-closes it
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
[[ "${1:-}" == "--build" ]] && mode=build
[[ "${1:-}" == "--classpick" ]] && mode=classpick

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
    profile="$userdata/ConquerOps.profile"
    for tactical in trophy_system_mp pda_hack_mp; do
        rm -f "$log"
        # A General spawns unarmed, so the run needs the C4/tactical class on the spawn list:
        # seed it in the profile (the autopilot picks the first custom by its computed row key).
        # ZHBO_LOADOUT still drives the converted art and the base kit. The profile file's v2
        # format: `v2`, the active set, then each set's `class`/name/spec/`end` records
        # (crates/zhbo-core/src/profile.rs).
        rm -f "$profile"
        cat >"$profile" <<EOF
v2
active 1
set 1 CLASS SET 1
class
C4 Test
primary=an94_mp+gl+reflex;secondary=fnp45_mp;lethal=satchel_charge_mp;tactical=$tactical;melee=knife_held_mp;perks=Lightweight,FastHands,Dexterity
end
EOF
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
if [[ $mode == build ]]; then
    export ZHBO_AUTOPILOT=build GGC_AUTO_EXIT_SECONDS=${GGC_AUTO_EXIT_SECONDS:-150}
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "spawned General" 90 || { echo "the General never appeared; see $log" >&2; exit 1; }
    for _ in $(seq 170); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    ok=0
    check() { # name, pattern
        if grep -qE "$2" "$out/ConquerOps.log"; then printf '%-28s PASS\n' "$1"; else printf '%-28s FAIL\n' "$1"; ok=1; fi
    }
    check "build menu opened" "build menu ZHBO_Build_"
    check "  ...with Back" "build menu ZHBO_Build_.*Command_ZHBO_BuildBack"
    check "structure ordered" "autopilot: ordered "
    check "General started building" "started building"
    check "General finished it" "finished building"
    check "back to his commands" "closed the build menu"
    check "first person: hammer placed" "placed .* in first person"
    check "  ...turned with the wheel" "placed .* turned [1-9][0-9]* degrees"
    check "  ...held F to build it" "finished building .* in first person"
    check "  ...held F to repair it" "repaired .* in first person"
    # Not an idle worker while in first person; one again once back overhead.
    if awk '/entered first-person control/ { in_fps = 1 } /left first-person control/ { in_fps = 0; back = 1 } /joined the idle workers/ { if (in_fps) bad = 1; else if (back) ok = 1 } END { exit !(ok && !bad) }' "$out/ConquerOps.log"; then
        printf '%-28s PASS\n' "back overhead: idle worker"
    else
        printf '%-28s FAIL\n' "back overhead: idle worker"; ok=1
    fi
    check "  ...ordered to repair" "ordered the General to repair"
    check "  ...repaired it" "General repaired .* overhead"
    # Each faction's General builds from its own worker's menu (the Toxin General's, not GLA's).
    check "factions' own build menus" "build menu: FactionGLAToxinGeneral builds as Chem_GLAInfantryWorker"
    check "  ...for every faction" "build menu: .* builds as "
    if grep -q "builds as (no worker)" "$out/ConquerOps.log"; then
        printf '%-28s FAIL\n' "  ...none without a worker"; ok=1
    fi
    # Built at about GENERAL_BUILD_RATE of a dozer's speed (0.5: twice the time, within 15%): the
    # first build, the overhead one.
    if awk '/a dozer:/ && !seen { seen = 1; gsub(/[()s:]/, " "); for (i = 1; i <= NF; ++i) if ($i == "in") t = $(i + 1); else if ($i == "dozer") d = $(i + 1) } END { exit !(d > 0 && t / d > 1.7 && t / d < 2.3) }' "$out/ConquerOps.log"; then
        printf '%-28s PASS\n' "  ...at half a dozer's speed"
    else
        printf '%-28s FAIL\n' "  ...at half a dozer's speed"; ok=1
    fi
    # Repaired overhead at the same share of a dozer's speed.
    if awk '/repaired .* overhead/ { gsub(/[()s:]/, " "); for (i = 1; i <= NF; ++i) if ($i == "in") t = $(i + 1); else if ($i == "dozer") d = $(i + 1) } END { exit !(d > 0 && t / d > 1.7 && t / d < 2.3) }' "$out/ConquerOps.log"; then
        printf '%-28s PASS\n' "  ...at half a dozer's speed"
    else
        printf '%-28s FAIL\n' "  ...at half a dozer's speed"; ok=1
    fi
    grep -E "build menu: Faction|ordered|building|first person|repair|damaged|idle workers|placed" "$out/ConquerOps.log" | head -40
    echo "screenshots and log: $out"
    exit $ok
fi
if [[ $mode == classpick ]]; then
    # The in-match create-a-class run is pick-only (BO2 only creates classes in the lobby): seed
    # the profile with one custom class ("AB") in the active class set, then let the autopilot
    # pick it from the spawn screen by its row key (5 default classes + AB). A pick must not
    # write the profile. The script then leaves first person, takes control again, forces the
    # screen open and Escapes it, proving first-person play continues. The profile file's v2
    # format: `v2`, the active set, then each set's `class`/name/spec/`end` records (profile.rs).
    profile="$userdata/ConquerOps.profile"
    rm -f "$profile"
    # Deliberately NOT the stock Assault kit: the kit check below greps for the MP7 and the
    # claymore, so a pick that logs the name but never applies the kit cannot pass.
    cat >"$profile" <<'EOF'
v2
active 1
set 1 CLASS SET 1
class
AB
primary=mp7_mp;secondary=fnp45_mp;lethal=claymore_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Toughness,FastHands,Dexterity
end
EOF
    export ZHBO_AUTOPILOT=classpick GGC_AUTO_EXIT_SECONDS=60
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "entered first-person" 90 || { echo "the General never entered first person; see $log" >&2; exit 1; }
    for _ in $(seq 70); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    ok=0
    check() { # name, pattern
        if grep -qE "$2" "$out/ConquerOps.log"; then printf '%-28s PASS\n' "$1"; else printf '%-28s FAIL\n' "$1"; ok=1; fi
    }
    check "picked the seeded class" "assumed the AB class"
    # The KIT, not just the name: the seeded class's primary is the MP7, and the sim's once-a-second
    # pose line names the weapon in hand ("... MP7 40|80"). The old seed was the Assault kit, so a
    # pick that never applied the kit still passed the name-only check.
    check "  ...kit applied (MP7)" "MP7 [0-9]+\|"
    # A pick never writes the profile (the Phase-2 save-vs-pick split).
    if grep -q "create-a-class: saved a custom class" "$out/ConquerOps.log"; then
        printf '%-28s FAIL\n' "  ...pick did not re-save"; ok=1
    else
        printf '%-28s PASS\n' "  ...pick did not re-save"
    fi
    # The script's second phase: after the field computer hands the General back, the host takes
    # control again and the screen is forced open, then Escape closes it while first person plays.
    check "  ...re-entered first person" "taking control again"
    # The second session: force the screen open, Escape it, then hold W. The local General's pose
    # must change after the second entry, which only happens if the screen really closed and the
    # game got the input again.
    if awk '/entered first-person control/ { n++; next } n >= 2 && / pos \(/ { if (first == "") first = $0; last = $0 } END { exit !(first != "" && last != "" && first != last) }' "$out/ConquerOps.log"; then
        printf '%-28s PASS\n' "  ...Escape kept playing"
    else
        printf '%-28s FAIL\n' "  ...Escape kept playing"; ok=1
    fi
    if grep -q "assumed the ??? class" "$out/ConquerOps.log"; then
        printf '%-28s FAIL\n' "  ...not as ???"; ok=1
    else
        printf '%-28s PASS\n' "  ...not as ???"
    fi
    grep -E "create-a-class|assumed the|taking control again|first-person control" "$out/ConquerOps.log" | head -20
    echo "screenshots and log: $out"
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
    # The timeline (ZhboAutopilot.cpp rtsAutopilotFrame) takes control at 690 logic frames.
    export ZHBO_AUTOPILOT=rts GGC_AUTO_EXIT_SECONDS=45
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "autopilot: taking control" 90 || { echo "the script never reached first person; see $log" >&2; exit 1; }
    for _ in $(seq 40); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    ok=0
    check() { # name, pattern
        if grep -qE "$2" "$out/ConquerOps.log"; then printf '%-28s PASS\n' "$1"; else printf '%-28s FAIL\n' "$1"; ok=1; fi
    }
    check "class guns armed" "General [0-9]+ armed slot 0 with ZHBO_GeneralWeapon_"
    check "rifle rounds counted" "fired slot 0 \([0-9]+/[0-9]+ left\)"
    check "lethal ordered" "General [0-9]+ ordered to use the lethal"
    check "  ...third slot armed" "General [0-9]+ armed slot 2 with ZHBO_GeneralWeapon_"
    check "lethal thrown" "fired slot 2 \(0/0 left\)"
    check "  ...as first person throws" "player 0 threw a lethal"
    check "back to the rifle" "locked to weapon slot 0"
    check "reload ordered" "General [0-9]+ reloads slot 0"
    check "knife ordered" "General [0-9]+ ordered to use the melee"
    check "  ...he closed in and struck" "player 0 meleed"
    # Slot 1 is the class's secondary. The run picks BO2's default Assault class at spawn, which
    # carries the SMAW there (statstable.csv), not the base kit's Tac-45.
    check "secondary picked" "uses weapon slot 1"
    check "secondary rounds counted" "fired slot 1"
    check "first person keeps the secondary" "entered first-person control"
    check "  ...holding the SMAW" "SMAW [0-9]+\|"
    echo "screenshots and log: $out"
    exit $ok
fi
if [[ $mode == autopilot ]]; then
    # BO2's premades carry no attachments, so none can toggle an alt mode; the "grenade launcher"
    # step needs a primary with its under-barrel launcher. Seed the profile with one custom class
    # in the active set (the AN-94 and its launcher, plus the frag, concussion, pistol and knife
    # the script's throws and swaps expect) and the autopilot picks it by its computed row key
    # (5 premades + this custom). The profile file's v2 format: `v2`, the active set, then each
    # set's `class`/name/spec/`end` records (crates/zhbo-core/src/profile.rs).
    profile="$userdata/ConquerOps.profile"
    rm -f "$profile"
    cat >"$profile" <<'EOF'
v2
active 1
set 1 CLASS SET 1
class
GL Test
primary=an94_mp+gl;secondary=fnp45_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Lightweight,FastHands,Dexterity
end
EOF
    # Autopilot timeline: enters first person about 3 s in, field computer about 12 s after that.
    # The exit is in wall seconds; under render hitches logic time (the scripts' clock) runs well
    # behind it, so leave enough wall time for the whole timeline to finish.
    export ZHBO_AUTOPILOT=1 GGC_AUTO_EXIT_SECONDS=120
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
