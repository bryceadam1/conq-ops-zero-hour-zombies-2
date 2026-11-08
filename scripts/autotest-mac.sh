#!/usr/bin/env bash
# In-game tests for Conquer Ops on macOS: the counterpart of autotest.ps1 and inputtest.ps1.
#
#   scripts/autotest-mac.sh            autopilot test (ZHBO_AUTOPILOT=1, scripted inside the mod; the
#                                      profile is seeded with a GL custom for the alt-mode check)
#   scripts/autotest-mac.sh --input    real-input test (ZHBO_AUTOPILOT=manual; T takes control,
#                                      Escape on the class screen declines the spawn (back overhead),
#                                      T again, 6 picks the SMG class, holds W, moves the mouse)
#   scripts/autotest-mac.sh --equipment
#                                      C4, Trophy System, Black Hat and Sensor Grenade against an enemy
#                                      drone and rocket troopers (three runs: C4 + Trophy, C4 + Black
#                                      Hat, C4 + Sensor Grenade, which goes off on its timed fuse where
#                                      it lies); each run's captures in its own folder
#   scripts/autotest-mac.sh --rts      the General under RTS control: his class guns armed, the rifle's
#                                      rounds counted, the command bar's orders (the lethal at a rebel,
#                                      a reload, the pistol, the knife at a close rebel), then first person
#   scripts/autotest-mac.sh --rts-dry  the same with no spare rifle rounds, then no spare pistol rounds:
#                                      the empty gun hands over to the other and he keeps firing
#   scripts/autotest-mac.sh --build    the General builds: his Build menu, a structure ordered from it,
#                                      Back; it must take about 1 / GENERAL_BUILD_RATE of a dozer's time.
#                                      Then in first person (facing the open ground the script
#                                      checked, to the degree): the hammer places one (turned with the
#                                      wheel), holding F builds it, and repairs it once damaged. Back
#                                      overhead he is an idle worker, and repairs it again when ordered.
#   scripts/autotest-mac.sh --classpick
#                                      the pick-only in-match class screen: the script seeds a custom
#                                      class ("Custom 2") in the profile; Escape on the first spawn
#                                      screen sends the classless General back overhead, the next
#                                      take-control picks the class by its row key, then the script
#                                      forces the screen open after re-taking control and Escape-closes it;
#                                      then the General dies and the next one meets the screen again,
#                                      opened on the class, and Enter re-picks it
#   scripts/autotest-mac.sh --splash   explosive splash on Zero Hour units: an enemy armour group (an
#                                      Overlord, three Crusaders) and two rebels, held still; one SMAW
#                                      into the Overlord, then a frag; every health change is logged
#                                      and the neighbours must take the blast, the frag's strike on
#                                      the Overlord its whole impact damage (MOD_IMPACT)
#   scripts/autotest-mac.sh --drops    weapon drops (BO2's dropweaponfordeath and pickups): the profile
#                                      holds two AN-94 customs (grenade launcher, reflex sight); the
#                                      first General wears the first, picks the second for his next
#                                      spawn and is killed past the grace period: his rifle drops as
#                                      its ground prop; the next General's screen opens on the second,
#                                      Enter takes it, and he walks onto it (its stock is
#                                      his, the same base weapon) and swaps for it with the use key
#   scripts/autotest-mac.sh --replay   spectating a replay: records a fresh skirmish (started from the
#                                      main menu by ZHBO_AUTOTEST_SKIRMISH: a loaded save records no
#                                      replay) with the scripted first-person run, then plays the replay
#                                      back: the viewer (the ReplayObserver, a spectator) must follow the
#                                      General at the recorded aim and send nothing. Zero Hour's Last
#                                      Replay is kept and put back.
#   scripts/autotest-mac.sh --mp       two instances in one LAN game, each player's autopilot fighting
#                                      the other's General; the logs' per-frame state hashes must agree
#                                      (runs scripts/mptest-mac.sh; see there)
#   scripts/autotest-mac.sh --smoke    the smoke screen against Zero Hour's units: the General throws a
#                                      smoke grenade; a Ranger and a rebel stand either side of it in
#                                      range and must not fight through it, and must once it clears;
#                                      a neutral Ranger ordered straight across it must walk round it
#   scripts/autotest-mac.sh --command  first-person commands (docs/COMMANDS.md): three of his Rangers
#                                      stand behind the General and an enemy rebel far ahead; typed
#                                      lines bring the Rangers to him, send the nearest at the rebel
#                                      (and nobody fights before that order), refuse a misspelt line,
#                                      give an order with no selection to the last one; then a
#                                      Barracks queues two Rangers and cancels one, his dozer lays out
#                                      a Barracks near him, and the Rangers take a right-click on open
#                                      ground
#   scripts/autotest-mac.sh --detail   the graphics detail Zero Hour picks (host patch 0025): four starts
#                                      with Options.ini set up as a new player (none), a player whose
#                                      file holds the old detector's Low, one who chose Low and one who
#                                      chose Custom; the first two must get High, the others keep theirs.
#                                      Options.ini is put back afterwards
#   scripts/autotest-mac.sh --flicker  flicker in a still scene: no enemies, the General holds his view
#                                      still and then nudges it a fifth of a pixel a frame, facing open
#                                      ground and then his Command Center; every logic frame is captured
#                                      at a 30 fps render and consecutive captures must agree in the
#                                      world (zhbo-assets flicker; FLICKER_LIMIT, FLICKER_RECT)
#   scripts/autotest-mac.sh --hipfire  hip fire against BO2's cone: the first default class (no profile
#                                      class), no enemy about; twelve taps from rest, a held string and a
#                                      running string at the ground ahead. Every shot's logged offset must
#                                      sit inside its cone with BO2's linear radius (mean r near 0.5), the
#                                      taps fly in the stance minimum and the running string at the maximum
#   scripts/autotest-mac.sh --showcase [object]
#                                      close-up captures of a General (idle, walk, death), no checks
#
# Both load a saved skirmish instead of clicking through the menus. Make the save once: start
#   conquerops -win -xres 1280 -yres 720 -quickstart -saveatframe 90 -saveto zhbo-autotest.sav
# then click Solo Play > Skirmish > Play Game. The game saves 3 seconds in and quits.
#
# Screenshots come from the engine's own capture (GGC_BGFX_SCREENSHOT_*), so the autopilot test
# needs no permissions. Its AFTER and INTERVAL count logic frames (30 a second; host patch 0027),
# whatever the render rate, and each file is named by its logic frame. The input test posts real key and mouse events and needs Accessibility
# permission for the app running this script (System Settings > Privacy & Security).
#
# Environment: ZHBO_ZH_DIR and ZHBO_BO2_DIR as for the launcher; OUT for the results folder; DIST
# for a build other than the default dist (`cargo xtask build --dist`).
set -euo pipefail

# The multiplayer leg is its own script (two instances, no save).
[[ "${1:-}" == "--mp" ]] && exec "$(dirname "$0")/mptest-mac.sh"

mode=autopilot
[[ "${1:-}" == "--input" ]] && mode=input
[[ "${1:-}" == "--showcase" ]] && mode=showcase
[[ "${1:-}" == "--equipment" ]] && mode=equipment
[[ "${1:-}" == "--rts" ]] && mode=rts
[[ "${1:-}" == "--rts-dry" ]] && mode=rts-dry
[[ "${1:-}" == "--build" ]] && mode=build
[[ "${1:-}" == "--classpick" ]] && mode=classpick
[[ "${1:-}" == "--scope" ]] && mode=scope
[[ "${1:-}" == "--smoke" ]] && mode=smoke
[[ "${1:-}" == "--w3d-ads" ]] && mode=w3d-ads
[[ "${1:-}" == "--replay" ]] && mode=replay
[[ "${1:-}" == "--splash" ]] && mode=splash
[[ "${1:-}" == "--drops" ]] && mode=drops
[[ "${1:-}" == "--detail" ]] && mode=detail
[[ "${1:-}" == "--flicker" ]] && mode=flicker
[[ "${1:-}" == "--hipfire" ]] && mode=hipfire
[[ "${1:-}" == "--command" ]] && mode=command

repo="$(cd "$(dirname "$0")/.." && pwd)"
dist="${DIST:-$HOME/Library/Application Support/ConquerOps/dist}"
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

# Stop the game and wait until it is gone: after SIGTERM it takes ~10 s to shut down and holds the
# engine's single-instance lock meanwhile, so a leg launched right after an earlier one bailed with
# "Generals is already running". SIGKILL if it outlives 30 s.
stop_game() {
    pkill -f "$dist/generalszh" 2>/dev/null || true
    for _ in $(seq 60); do pgrep -f "$dist/generalszh" >/dev/null || return 0; sleep 0.5; done
    pkill -9 -f "$dist/generalszh" 2>/dev/null || true
    sleep 1
}
stop_game
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
    stop_game
    cp "$log" "$out/" 2>/dev/null || true
    # `if`, not `&&`: with no capture left (the equipment leg files each run's away) a false test
    # as the loop's last command would make `finish` fail and, under `set -e`, the leg exit 1.
    for f in "$out"/shot.*.bmp; do
        if [[ -f "$f" ]]; then sips -s format png "$f" --out "${f%.bmp}.png" >/dev/null && rm "$f"; fi
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
if [[ $mode == detail ]]; then
    # Options.ini records the detector's pick (IdealStaticGameLOD) and the level in use
    # (StaticGameLOD). Each case starts the game with one such file, waits for the bridge's
    # "graphics detail:" line (logged once GameLOD has applied the level) and checks the line and
    # what the file holds afterwards.
    options="$userdata/Options.ini"
    keep="$out/Options.ini.player"
    rm -f "$keep"
    [[ -f "$options" ]] && cp -p "$options" "$keep"
    put_back() { if [[ -f "$keep" ]]; then cp -p "$keep" "$options"; else rm -f "$options"; fi; }
    # finish's capture loop returns non-zero when there are no captures; under set -e that would
    # turn a passing run's exit status into 1.
    trap 'finish || true; put_back' EXIT
    ok=0
    check() { # name, file, pattern
        if grep -qE "$3" "$2"; then printf '%-40s PASS\n' "$1"; else printf '%-40s FAIL\n' "$1"; ok=1; fi
    }
    run_case() { # name, Options.ini text ("" for no file)
        rm -f "$log" "$options"
        [[ -n "$2" ]] && printf '%b' "$2" >"$options"
        export ZHBO_AUTOPILOT=manual GGC_AUTO_EXIT_SECONDS=60
        "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout-$1.txt" 2>&1
        wait_for_log "graphics detail:" 90 || { echo "$1: no graphics detail line; see $log" >&2; ok=1; }
        stop_game
        cp "$log" "$out/ConquerOps-$1.log" 2>/dev/null || true
        cp "$options" "$out/Options-$1.ini" 2>/dev/null || true
    }
    run_case new ""
    check "new player: High" "$out/ConquerOps-new.log" "graphics detail: High \(the recommended level"
    check "  ...with the High particle cap" "$out/ConquerOps-new.log" "graphics detail: High .*particle cap 3000"
    check "  ...recorded in Options.ini" "$out/Options-new.ini" "^StaticGameLOD = High"
    run_case detected-low "IdealStaticGameLOD = Low\nStaticGameLOD = Low\n"
    check "old detector's Low: High" "$out/ConquerOps-detected-low.log" "graphics detail: High \(the recommended level"
    check "  ...recorded in Options.ini" "$out/Options-detected-low.ini" "^StaticGameLOD = High"
    run_case chose-low "IdealStaticGameLOD = High\nStaticGameLOD = Low\n"
    check "player's Low: kept" "$out/ConquerOps-chose-low.log" "graphics detail: Low \(the player's choice"
    check "  ...with the Low particle cap" "$out/ConquerOps-chose-low.log" "graphics detail: Low .*particle cap 500"
    check "  ...still in Options.ini" "$out/Options-chose-low.ini" "^StaticGameLOD = Low"
    run_case chose-custom "IdealStaticGameLOD = High\nStaticGameLOD = Custom\nMaxParticleCount = 1234\n"
    check "player's Custom: kept" "$out/ConquerOps-chose-custom.log" "graphics detail: Custom \(the player's choice.*particle cap 1234"
    check "  ...still in Options.ini" "$out/Options-chose-custom.ini" "^StaticGameLOD = Custom"
    grep -h "graphics detail:" "$out"/ConquerOps-*.log
    exit $ok
fi
if [[ $mode == equipment ]]; then
    export ZHBO_AUTOPILOT=equipment GGC_AUTO_EXIT_SECONDS=40
    ok=0
    check() { # log, name, pattern
        if grep -qE "$3" "$1"; then printf '%-28s PASS\n' "$2"; else printf '%-28s FAIL\n' "$2"; ok=1; fi
    }
    profile="$userdata/ConquerOps.profile"
    for tactical in trophy_system_mp pda_hack_mp sensor_grenade_mp; do
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
        stop_game
        cp "$log" "$out/ConquerOps-$tactical.log"
        mkdir -p "$out/$tactical"
        for f in "$out"/shot.*.bmp; do
            if [[ -f "$f" ]]; then sips -s format png "$f" --out "$out/$tactical/$(basename "${f%.bmp}").png" >/dev/null && rm "$f"; fi
        done
        l="$out/ConquerOps-$tactical.log"
        echo "== C4 + $tactical"
        check "$l" "C4 thrown and stuck" "put down equipment \(C4\)"
        check "$l" "C4 detonated" "set off their C4"
        if [[ $tactical == trophy_system_mp ]]; then
            check "$l" "Trophy System planted" "put down equipment \(Trophy\)"
            check "$l" "Trophy shot down a rocket" "Trophy System shot down a projectile"
        elif [[ $tactical == sensor_grenade_mp ]]; then
            # The sensor lands, then its timed fuse sets it off where it lies (t6mp FUN_00607c00,
            # FUN_0060a480): a blast logged after it was put down.
            check "$l" "Sensor Grenade landed" "put down equipment \(Sensor\)"
            if awk '/put down equipment \(Sensor\)/ { down = 1 } down && /grenade went off at/ { found = 1 } END { exit !found }' "$l"; then
                printf '%-28s PASS\n' "Sensor went off on its fuse"
            else
                printf '%-28s FAIL\n' "Sensor went off on its fuse"; ok=1
            fi
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
    # First person starts facing the open ground the script checked: nothing turned him while the
    # Take Control order was on its way (else the hammer aims at ground nobody checked).
    if awk '/autopilot: facing open ground at/ { for (i = 1; i <= NF; ++i) if ($i == "at") want = $(i + 1); armed = 1 }
            armed && /player [0-9]+ pos .* yaw / { for (i = 1; i <= NF; ++i) if ($i == "yaw") got = $(i + 1) * 180 / 3.14159265; armed = 0; seen = 1 }
            END { d = got - want; while (d > 180) d -= 360; while (d < -180) d += 360; exit !(seen && d < 1 && d > -1) }' "$out/ConquerOps.log"; then
        printf '%-28s PASS\n' "first person: faces it"
    else
        printf '%-28s FAIL\n' "first person: faces it"; ok=1
    fi
    check "first person: hammer placed" "placed .* in first person"
    check "  ...turned with the wheel" "placed .* turned [1-9][0-9]* degrees"
    check "  ...held F to build it" "finished building .* in first person"
    check "  ...held F to repair it" "repaired .* in first person"
    # The hammer draws in the guns' native pass (own depth: the site's timbers never cut it).
    check "  ...hammer drawn natively" "viewmodel: ZHBO_VM..._HAM drawn by the native path"
    if grep -q "viewmodel: ZHBO_VM..._HAM drawn by the W3D path" "$out/ConquerOps.log"; then
        printf '%-28s FAIL\n' "  ...never by W3D"; ok=1
    else
        printf '%-28s PASS\n' "  ...never by W3D"
    fi
    if grep -q "native viewmodel: .* played before its file arrived" "$out/ConquerOps.log"; then
        printf '%-28s FAIL\n' "  ...every clip on hand"; ok=1
    else
        printf '%-28s PASS\n' "  ...every clip on hand"
    fi
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
    # the profile with one custom class ("Custom 2") in the active class set, then let the autopilot
    # pick it from the spawn screen by its row key (5 default classes + Custom 2). A pick must not
    # write the profile. Before that, the first take-control Escapes the spawn screen: BO2 never
    # spawns a classless player, so the General goes back overhead and the host takes control
    # again. After the pick the script leaves first person, takes control again, forces the
    # screen open and Escapes it, proving first-person play continues. Then the General is killed,
    # a new one takes his place, and the host takes control of him: every General is picked for, so
    # he meets the spawn screen, opened on Custom 2, and the script's Enter re-picks it (one press).
    # The profile file's v2
    # format: `v2`, the active set, then each set's `class`/name/spec/`end` records (profile.rs).
    profile="$userdata/ConquerOps.profile"
    rm -f "$profile"
    # The playtest's own class (round 2: "I couldn't select my second custom class"): an
    # attachment on each gun and all three camos, whose readable text is 258 bytes, two over the
    # 64-word pick payload the text form used to ride (the pick now rides as catalog tokens,
    # classwire.rs). Deliberately NOT the stock Assault kit: the kit check greps the MTAR, so a pick
    # that logs the name but never applies the kit cannot pass. The MTAR carries the Target Finder:
    # an attachment combination's viewmodel has a number of three or more digits, which the
    # base-weapon legs never exercise (the bridge once misread its faction letter and every
    # combination drew nothing in first person).
    cat >"$profile" <<'EOF'
v2
active 1
set 1 CLASS SET 1
class
Custom 2
primary=tar21_mp+rangefinder;secondary=crossbow_mp+ir;lethal=frag_grenade_mp;tactical=willy_pete_mp;lethal_count=1;tactical_count=1;melee=knife_held_mp;perks=Lightweight,Toughness,Dexterity;camo=camo_gold;knife=camo_gold;secondary_camo=camo_artofwar
end
EOF
    export ZHBO_AUTOPILOT=classpick GGC_AUTO_EXIT_SECONDS=75
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "entered first-person" 90 || { echo "the General never entered first person; see $log" >&2; exit 1; }
    for _ in $(seq 85); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    ok=0
    check() { # name, pattern
        if grep -qE "$2" "$out/ConquerOps.log"; then printf '%-28s PASS\n' "$1"; else printf '%-28s FAIL\n' "$1"; ok=1; fi
    }
    # The Escape-first leg: the first session ends with the General handed back overhead before
    # any class landed and without the field computer (he was never spawned), and never with the
    # knife in hand (the classless kit is not playable).
    if awk '/opened the field computer|assumed the/ { exit 1 } /returned General [0-9]+ to RTS control/ { found = 1; exit 0 } END { exit !found }' "$out/ConquerOps.log"; then
        printf '%-28s PASS\n' "Escape declined the spawn"
    else
        printf '%-28s FAIL\n' "Escape declined the spawn"; ok=1
    fi
    if grep -qE " Combat Knife [0-9]+\|" "$out/ConquerOps.log"; then
        printf '%-28s FAIL\n' "  ...never played classless"; ok=1
    else
        printf '%-28s PASS\n' "  ...never played classless"
    fi
    check "picked the seeded class" "assumed the Custom 2 class"
    # The KIT, not just the name: the seeded class's primary is the MTAR, and the sim's once-a-second
    # pose line names the weapon in hand ("... MTAR 30|120"). An older seed was the Assault kit, so a
    # pick that never applied the kit still passed the name-only check.
    check "  ...kit applied (MTAR)" "MTAR [0-9]+\|"
    # The combination's own native viewmodel composed (`ZV<vm><faction letter>`, vm >= 100), and no
    # viewmodel failed to load or got a non-letter faction.
    check "  ...combination viewmodel drawn" "native viewmodel: ZV[0-9]{3,}[UCG] composed"
    if grep -qE "native viewmodel: .*(\.zb[a-z] is not in the archive|faction 0x)" "$out/ConquerOps.log"; then
        printf '%-28s FAIL\n' "  ...no viewmodel load failure"; ok=1
    else
        printf '%-28s PASS\n' "  ...no viewmodel load failure"
    fi
    # A pick never writes the profile (the Phase-2 save-vs-pick split).
    if grep -q "create-a-class: saved a custom class" "$out/ConquerOps.log"; then
        printf '%-28s FAIL\n' "  ...pick did not re-save"; ok=1
    else
        printf '%-28s PASS\n' "  ...pick did not re-save"
    fi
    # The script's second phase: after the field computer hands the General back, the host takes
    # control again and the screen is forced open, then Escape closes it while first person plays.
    check "  ...re-entered first person" "taking control again"
    # The third session: force the screen open, Escape it, then hold W. The local General's pose
    # must change after the third entry, which only happens if the screen really closed and the
    # game got the input again.
    if awk '/entered first-person control/ { n++; next } /autopilot: killing General/ { stop = 1 } n == 3 && !stop && / pos \(/ { if (first == "") first = $0; last = $0 } END { exit !(first != "" && last != "" && first != last) }' "$out/ConquerOps.log"; then
        printf '%-28s PASS\n' "  ...Escape kept playing"
    else
        printf '%-28s FAIL\n' "  ...Escape kept playing"; ok=1
    fi
    # The fourth session: the General is killed and a new one takes his place. Every General is
    # picked for (the deliberate break from BO2's pers["class"] respawn): the new General waits
    # behind the screen (a pose line awaiting a class), the script's Enter takes the pre-selected
    # Custom 2 (a second "assumed" line, after the respawn), then a pose line names the MTAR and the
    # pose changes (the script walks, which a screen would block).
    check "  ...respawned a General" "autopilot: respawned General"
    if awk '/autopilot: respawned General/ { r = 1; next } r && /entered first-person control/ { e = 1; next } e && /assumed the Custom 2 class/ { p = 1; next } e && / pos \(/ { if (!p && $0 ~ /awaiting a class/) waited = 1; if (p && $0 ~ /MTAR [0-9]+\|/) { mtar = 1; if (first == "") first = $0; last = $0 } } END { exit !(e && waited && p && mtar && first != last) }' "$out/ConquerOps.log"; then
        printf '%-28s PASS\n' "  ...next General re-picked"
    else
        printf '%-28s FAIL\n' "  ...next General re-picked"; ok=1
    fi
    if grep -q "assumed the ??? class" "$out/ConquerOps.log"; then
        printf '%-28s FAIL\n' "  ...not as ???"; ok=1
    else
        printf '%-28s PASS\n' "  ...not as ???"
    fi
    grep -E "create-a-class|assumed the|taking control again|first-person control|to RTS control|respawned|killing|next spawn" "$out/ConquerOps.log" | head -30
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
        stop_game
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
    check "  ...as first person throws" "player [0-9]+ threw a lethal"
    check "back to the rifle" "locked to weapon slot 0"
    check "reload ordered" "General [0-9]+ reloads slot 0"
    check "knife ordered" "General [0-9]+ ordered to use the melee"
    check "  ...he closed in and struck" "player [0-9]+ meleed"
    # Slot 1 is the class's secondary. The run picks BO2's default Assault class at spawn, which
    # carries the SMAW there (statstable.csv), not the base kit's Tac-45.
    check "secondary picked" "uses weapon slot 1"
    check "secondary rounds counted" "fired slot 1"
    # BO2 animates the body by the weapon in hand (mp/playeranim.script): the SMAW raises its own
    # animation set's model condition on the W3D body (ZhboRtsWeapons.cpp applyWorldPoses).
    check "  ...in its own body stance" "third-person animation set [1-9] \(RIDER[1-8]\)"
    check "first person keeps the secondary" "entered first-person control"
    check "  ...holding the SMAW" "SMAW [0-9]+\|"
    # The native third person is the default (ZHBO_NATIVE_BODY unset = 2, the body; 1 the weapon
    # only): the overhead General's gun composes through nativebody (ZHBO_NATIVE_BODY=0 opts out,
    # and this fails).
    check "native third-person weapon" "native body: General [0-9]+ weapon [0-9]+ camo [^ ]+ composed \\(ZW[0-9]+[A-Z]\\)"
    # The native body stands in for the hidden W3D one overhead (host patch 0029): a click on his
    # drawn body picks him, his body walks while Zero Hour moves him (movement state 2 walk, 3 run),
    # and his shots' flash, brass and tracer leave his drawn gun's tag_flash, not his feet.
    check "click on his body picks him" "autopilot: a click on the General's body at \\([0-9-]+, [0-9-]+\\) picks General [0-9]+"
    check "walks as Zero Hour moves him" "native body: General [0-9]+ body clip [^ ]+ \\(movement state [23]\\)"
    check "shots leave his drawn muzzle" "General [0-9]+ fires from his drawn muzzle \\(tag_flash ([3-9]|[1-9][0-9])\\.[0-9] units above his feet\\)"
    # The render runs at the display's rate (host patch 0026): presentation written per 30 Hz frame
    # (tracers, recoil, vehicle springs and treads, ropes; host patch 0031) steps by the W3D clock in
    # logic frames, which must keep logic time while drawing more often than the logic steps.
    clock=$(grep -E "W3D clock: [0-9.]+ logic frames of W3D time in [0-9]+ logic frames, [0-9]+ draws" "$out/ConquerOps.log" | tail -1)
    if awk '{w = $4; l = $11; d = $14; exit !(l > 0 && w / l > 0.95 && w / l < 1.05 && d > l)}' <<<"$clock"; then
        printf '%-28s PASS\n' "W3D clock keeps logic time"
    else
        printf '%-28s FAIL  (%s)\n' "W3D clock keeps logic time" "${clock:-no W3D clock line}"; ok=1
    fi
    echo "screenshots and log: $out"
    exit $ok
fi
if [[ $mode == splash ]]; then
    # The SMAW in the secondary slot: seed it in the profile (the autopilot picks the first custom
    # by its computed row key; see the autopilot mode below for the format).
    profile="$userdata/ConquerOps.profile"
    rm -f "$profile"
    cat >"$profile" <<'EOF'
v2
active 1
set 1 CLASS SET 1
class
Splash Test
primary=an94_mp;secondary=smaw_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Lightweight,FastHands,Dexterity
end
EOF
    export ZHBO_AUTOPILOT=splash GGC_AUTO_EXIT_SECONDS=${GGC_AUTO_EXIT_SECONDS:-60}
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "entered first-person" 90 || { echo "the General never entered first person; see $log" >&2; exit 1; }
    for _ in $(seq 70); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    ok=0
    check() { # name, pattern
        if grep -qE "$2" "$out/ConquerOps.log"; then printf '%-34s PASS\n' "$1"; else printf '%-34s FAIL\n' "$1"; ok=1; fi
    }
    grep -E "autopilot splash:" "$out/ConquerOps.log" || true
    check "SMAW fired" "fired slot 1|SMAW"
    # BO2's vehicle rule (getvehicleprojectilescalar): the struck tank takes a fifth of the SMAW's
    # 1000, x1.5 for Zero Hour (300; TankArmor takes EXPLOSION at 100%), and is spared the radius.
    check "the struck Overlord took 300" "autopilot splash: ChinaTankOverlord [0-9]+ took 300.0 "
    check "a Crusader beside it took the blast" "autopilot splash: AmericaTankCrusader [0-9]+ took [1-9][0-9.]* .*, 36 from the target"
    check "a rebel took the blast" "autopilot splash: GLAInfantryRebel [0-9]+ took [1-9]"
    # The frag thrown after it strikes the Overlord before it goes off: BO2's MOD_IMPACT, its whole
    # iDamage (15 x 1.5), which the tuning pack's [MeansOfDeath] sends as EXPLOSION (TankArmor
    # 100 %), not as small arms (25 %: 5.6).
    check "the frag struck it as an impact" "autopilot splash: ChinaTankOverlord [0-9]+ took 22.5 "
    echo "screenshots and log: $out"
    exit $ok
fi
if [[ $mode == drops ]]; then
    # Two AN-94 customs (the autopilot picks them by their computed row keys; see the autopilot
    # mode below for the format): the dropped rifle and the one that walks over it share a base.
    profile="$userdata/ConquerOps.profile"
    rm -f "$profile"
    cat >"$profile" <<'EOF'
v2
active 1
set 1 CLASS SET 1
class
Drop GL
primary=an94_mp+gl;secondary=fnp45_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Lightweight,FastHands,Dexterity;camo=camo_gold
end
class
Drop Reflex
primary=an94_mp+reflex;secondary=fnp45_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Lightweight,FastHands,Dexterity
end
EOF
    export ZHBO_AUTOPILOT=drops GGC_AUTO_EXIT_SECONDS=${GGC_AUTO_EXIT_SECONDS:-75}
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "entered first-person" 90 || { echo "the General never entered first person; see $log" >&2; exit 1; }
    for _ in $(seq 85); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    ok=0
    check() { # name, pattern
        if grep -qE "$2" "$out/ConquerOps.log"; then printf '%-34s PASS\n' "$1"; else printf '%-34s FAIL\n' "$1"; ok=1; fi
    }
    grep -E "drops|dropped|picked up|took ammo|next spawn" "$out/ConquerOps.log" || true
    check "class for the next spawn" "comes in at the next spawn"
    check "the General was killed" "autopilot drops: killing General"
    # Every General is picked for: the next one's screen opened on the class changed for him, and
    # the script's Enter took it.
    check "the next General picked it" "General [0-9]+ assumed the Drop Reflex class"
    check "his rifle dropped" "General dropped his an94_mp\+gl at"
    check "  ...as its ground prop" "drops: an94_mp\+gl down at .* as ZHBO_Drop_an94_mp object [1-9]"
    # The native third person (ZHBO_NATIVE_BODY, on by default) draws the combination on the
    # ground in its camo and hides the W3D prop.
    check "  ...drawn natively in its camo" "native body: General [0-9]+ weapon [0-9]+ camo camo_gold composed"
    check "  ...lying, its prop hidden" "native body: dropped weapon [0-9]+ \(weapon [0-9]+\) drawn lying"
    check "the next took its stock" "took ammo from a dropped an94_mp\+gl"
    check "  ...and swapped for it" "picked up the an94_mp\+gl"
    check "  ...putting his own down" "drops: an94_mp\+reflex down at .* as ZHBO_Drop_an94_mp object [1-9]"
    echo "screenshots and log: $out"
    exit $ok
fi
if [[ $mode == replay ]]; then
    replays="$userdata/Replays"
    last="$replays/00000000.rep"
    mkdir -p "$replays"
    # The recording overwrites Zero Hour's Last Replay: keep the player's and put it back.
    [[ -f "$last" ]] && cp "$last" "$out/LastReplay.kept.rep"
    put_back() { if [[ -f "$out/LastReplay.kept.rep" ]]; then cp "$out/LastReplay.kept.rep" "$last"; fi; rm -f "$replays/zhbo-spectate.rep"; }
    # The recorded run is the default scripted one, with its GL custom class (see the autopilot
    # mode below for the profile's format).
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
    # Both runs need the same content CRC (ZHBO_AUTOPILOT is folded into it), and the playback's
    # logic step builds the same scene for the recording's human seat.
    export ZHBO_AUTOPILOT=1
    secs=${GGC_AUTO_EXIT_SECONDS:-90}
    rm -f "$last"
    ZHBO_AUTOTEST_SKIRMISH=1 GGC_AUTO_EXIT_SECONDS=$secs "$dist/conquerops" -win -xres 1280 -yres 720 -quickstart >"$out/stdout-record.txt" 2>&1
    wait_for_log "entered first-person" 150 || { stop_game; put_back; echo "the recorded General never entered first person; see $log" >&2; exit 1; }
    for _ in $(seq 120); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    stop_game
    cp "$log" "$out/ConquerOps-record.log"
    if [[ ! -f "$last" ]]; then
        put_back; echo "no replay was recorded" >&2; exit 1
    fi
    cp "$last" "$replays/zhbo-spectate.rep"
    cp "$last" "$out/zhbo-spectate.rep"
    rm -f "$log"
    GGC_AUTO_EXIT_SECONDS=$secs "$dist/conquerops" -win -xres 1280 -yres 720 -quickstart -loadreplay zhbo-spectate.rep >"$out/stdout-replay.txt" 2>&1
    wait_for_log "spectator: following" 150 || true
    for _ in $(seq 120); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    put_back
    trap - EXIT
    ok=0
    check() { # log, name, pattern
        if grep -qE "$3" "$1"; then printf '%-34s PASS\n' "$2"; else printf '%-34s FAIL\n' "$2"; ok=1; fi
    }
    rec="$out/ConquerOps-record.log"
    play="$out/ConquerOps.log"
    check "$rec" "recorded: first person" "entered first-person control"
    check "$play" "replay: the General came back" "entered first-person control"
    check "$play" "replay: the viewer follows him" "spectator: following seat [0-9]+'s General"
    # While following, the view is the recorded aim interpolated across one logic frame: at a render
    # rate above 30 Hz it trails the aim by at most the frame's own turn (from its previous yaw).
    # And the recorded aim moves (the scripted mouse).
    if awk 'function wrap(d) { while (d > 3.14159) d -= 6.28318; while (d < -3.14159) d += 6.28318; return d < 0 ? -d : d }
            /spectator: still following/ { for (i = 1; i <= NF; ++i) { if ($i == "view" && $(i+1) == "yaw") v = $(i+2); if ($i == "aim" && $(i+1) == "yaw") a = $(i+2); if ($i == "from" && $(i+1) == "yaw") f = $(i+2) } v += 0; a += 0; f += 0; if (wrap(v - a) > wrap(a - f) + 0.02) bad = 1; n++; seen[a] = 1 } END { k = 0; for (x in seen) k++; exit !(n >= 3 && k >= 2 && !bad) }' "$play"; then
        printf '%-34s PASS\n' "replay: view = recorded aim"
    else
        printf '%-34s FAIL\n' "replay: view = recorded aim"; ok=1
    fi
    if grep -qE "choose class: General|selected General .* back overhead" "$play"; then
        printf '%-34s FAIL\n' "replay: the viewer sent nothing"; ok=1
    else
        printf '%-34s PASS\n' "replay: the viewer sent nothing"
    fi
    echo "logs and the replay: $out"
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
    # The tablet draws in the guns' native pass (their materials, lights and own depth), never the
    # W3D fallback (lit and depth-tested apart from every gun: the dark, flat tablet).
    check "  ...tablet drawn natively" "viewmodel: ZHBO_VMUSA_TAB drawn by the native path"
    if grep -q "viewmodel: ZHBO_VM..._TAB drawn by the W3D path" "$out/ConquerOps.log"; then
        printf '%-22s FAIL\n' "  ...never by W3D"; ok=1
    else
        printf '%-22s PASS\n' "  ...never by W3D"
    fi
    # Every clip is on hand when it first plays (composing asks for them all): a clip posed before
    # its file arrived drew a frame of the bind pose (the tablet at the foot of the screen as its
    # idle took over from the pullout, on every opening).
    if grep -q "native viewmodel: .* played before its file arrived" "$out/ConquerOps.log"; then
        printf '%-22s FAIL (%s)\n' "  ...every clip on hand" "$(grep -m1 -o "native viewmodel: .* played before its file arrived" "$out/ConquerOps.log")"; ok=1
    else
        printf '%-22s PASS\n' "  ...every clip on hand"
    fi
    check "returned to RTS" "returned General [0-9]+ to RTS control"
    check "re-entered" "taking control again"
    check "took enemy fire" "General took [0-9]+ damage"
    # His health is Zero Hour's heroes' (the converter reads Burton's, Black Lotus's and Kell's 200
    # from INIZH.big; GeneralHeroToughness 1) before the scene raises it for the script's length.
    check "  ...at a hero's health" "the General's own health 200 \(a hero's\)"
    # R8d: impacts land. The sim's once-a-second line says what each pellet hit (the three rebels
    # in front are flesh; the pellets that miss them stop on the ground, whose class comes from the
    # new terrain query), and the bridge's line says the name resolved and the FXList played.
    check "impacts: flesh" "player [0-9]+ impacts:.*flesh [1-9]"
    check "impacts: terrain" "player [0-9]+ impacts:.*(concrete|dirt|grass|asphalt|rock|sand|snow|wood) [1-9]"
    check "impact fx played" "impact fx: [1-9][0-9]* played, 0 missing"
    # The own footsteps fall on BO2's bob cycle, each from the first-person footstep table for the
    # movement on the ground's surface (`stepsounds`); never the old grass-only events.
    check "own footsteps" "sound ZHBO_BO2_Step_1st(q)?_(sprint|run|walk|crouch_run|crouch_walk|prone)_[a-z]+: playing"
    if grep -qE "sound ZHBO_BO2_(Step|SprintStep|Land): " "$out/ConquerOps.log"; then
        printf '%-22s FAIL\n' "  ...no old events"; ok=1
    else
        printf '%-22s PASS\n' "  ...no old events"
    fi
    echo "screenshots and log: $out"
    exit $ok
fi

if [[ "$mode" == "smoke" ]]; then
    # The class the script picks carries the smoke grenade (Willy Pete) as its tactical; the bridge's
    # smoke scene (ZhboAutopilot.cpp) places the units once the screen has grown and logs what they do.
    profile="$userdata/ConquerOps.profile"
    rm -f "$profile"
    cat >"$profile" <<'EOF'
v2
active 1
set 1 CLASS SET 1
class
Smoke Test
primary=an94_mp;secondary=fnp45_mp;lethal=frag_grenade_mp;tactical=willy_pete_mp;melee=knife_held_mp;perks=Lightweight,FastHands,Dexterity
end
EOF
    export ZHBO_AUTOPILOT=smoke GGC_AUTO_EXIT_SECONDS=${GGC_AUTO_EXIT_SECONDS:-45} GGC_BGFX_SCREENSHOT_INTERVAL=${GGC_BGFX_SCREENSHOT_INTERVAL:-60}
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "entered first-person" 120 || { echo "the General never entered first person; see $log" >&2; exit 1; }
    for _ in $(seq 60); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    ok=0
    l="$out/ConquerOps.log"
    check() { # name, pattern
        if grep -qE "$2" "$l"; then printf '%-34s PASS\n' "$1"; else printf '%-34s FAIL\n' "$1"; ok=1; fi
    }
    check "smoke grenade went off" "autopilot: smoke went off at"
    check "  ...its screen stood" "smoke: screen at .* blocks sight"
    check "scene placed across it" "autopilot: smoke scene: Ranger [0-9]+ and rebel [0-9]+"
    check "units can't see through it" "smoke: .* can't see .* through it"
    if grep -q "attacks .* through the screen" "$l"; then
        printf '%-34s FAIL\n' "  ...nobody fired through it"; ok=1
    else
        printf '%-34s PASS\n' "  ...nobody fired through it"
    fi
    # The positive control: once the screen is gone (12 s) they see each other and fight.
    if awk '/autopilot: smoke cleared: .* attacks/ { for (i = 1; i <= NF; ++i) if ($i == "s" && $(i - 1) ~ /^\(/) t = substr($(i - 1), 2); found = 1 } END { exit !(found && t + 0 >= 11.9) }' "$l"; then
        printf '%-34s PASS\n' "  ...and fought once it cleared"
    else
        printf '%-34s FAIL\n' "  ...and fought once it cleared"; ok=1
    fi
    # The move straight across it went round: never inside three quarters of the blocker's radius.
    if awk '/the Ranger ordered across arrived; he passed/ { gsub(/[()]/, " "); for (i = 1; i <= NF; ++i) { if ($i == "passed") d = $(i + 1); if ($i == "radius") r = $(i + 1) } found = 1 } END { exit !(found && r > 0 && d >= 0.75 * r) }' "$l"; then
        printf '%-34s PASS\n' "move order went round it"
    else
        printf '%-34s FAIL\n' "move order went round it"; ok=1
    fi
    grep -E "smoke" "$l" | head -40
    echo "screenshots and log: $out"
    exit $ok
fi

if [[ "$mode" == "command" ]]; then
    # The core's command run (client/autopilot.rs COMMAND_LINES) submits typed lines; the bridge's
    # scene (ZhboAutopilot.cpp commandAutopilotFrame) logs what the Rangers do; the core logs each
    # line's outcome and the bridge each order it sends (ZhboCommand.cpp).
    profile="$userdata/ConquerOps.profile"
    rm -f "$profile"
    cat >"$profile" <<'EOF'
v2
active 1
set 1 CLASS SET 1
class
Command Test
primary=an94_mp;secondary=fnp45_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Lightweight,FastHands,Dexterity
end
EOF
    export ZHBO_AUTOPILOT=command GGC_AUTO_EXIT_SECONDS=${GGC_AUTO_EXIT_SECONDS:-55} GGC_BGFX_SCREENSHOT_INTERVAL=${GGC_BGFX_SCREENSHOT_INTERVAL:-150}
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "entered first-person" 120 || { echo "the General never entered first person; see $log" >&2; exit 1; }
    for _ in $(seq 60); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    ok=0
    l="$out/ConquerOps.log"
    check() { # name, pattern
        if grep -qE "$2" "$l"; then printf '%-34s PASS\n' "$1"; else printf '%-34s FAIL\n' "$1"; ok=1; fi
    }
    check "scene placed" "autopilot: command scene: Rangers [0-9]+, [0-9]+, [0-9]+ 150 behind the General, rebel [1-9][0-9]*"
    check "move line resolved" "command: 'rangers move to me' -> 3 units: rangers move to me, 1 order"
    check "  ...order sent" "command order: move to 3 object\(s\)"
    check "  ...the Rangers came" "autopilot: command scene: the Rangers came to the General"
    check "attack line resolved" "command: 'the nearest ranger attack the nearest enemy rebel' -> 1 unit"
    check "  ...order sent" "command order: attack to 1 object\(s\), target [1-9]"
    check "  ...a Ranger attacks the rebel" "autopilot: command scene: Ranger [0-9]+ attacks rebel [0-9]+ \(attack order sent: yes\)"
    if grep -q "attack order sent: no" "$l"; then
        printf '%-34s FAIL\n' "  ...nobody fought before it"; ok=1
    else
        printf '%-34s PASS\n' "  ...nobody fought before it"
    fi
    check "misspelt line refused" "command: 'rangres stop' refused: didn't expect 'rangres'"
    check "no selection: the last one" "command: 'stop' -> 1 unit: stop, 1 order"
    check "  ...order sent" "command order: stop to 1 object\(s\)"
    check "build line: two presses" "command: 'build 2 rangers' -> 1 unit: build 2 rangers, 2 order\(s\)"
    check "  ...the queue holds two" "autopilot: command scene: the Barracks' queue holds 2"
    check "cancel line: one cancelled" "command: 'cancel ranger' -> 1 unit: cancel ranger, 1 order\(s\)"
    check "  ...the queue holds one" "autopilot: command scene: the Barracks' queue holds 1"
    check "construct line: placed" "command order: construct to 1 object\(s\).*: 1$"
    check "  ...a Barracks site" "autopilot: command scene: a Barracks site [0-9]+"
    check "right-click on open ground" "command order: right-click to 3 object\(s\).*: 1$"
    grep -E "command" "$l" | head -60
    echo "screenshots and log: $out"
    exit $ok
fi

if [[ "$mode" == "hipfire" ]]; then
    # BO2's hip-fire cone in game (weapons::aim_spread): the bridge logs each first-person shot as
    # `[<frame>] … hip shot: <weapon> spread <s> deg (cone <min>..<max>, ads <a>, moving <m>) offset <o>
    # deg r <r>`, the offset measured from the aim to where the engine's trace put the round. A shot
    # 10 or more frames after the last, standing still, is a first shot from rest (a tap may fire a
    # second round on the next frame, at the first's add, as BO2's next command would). No profile
    # class: the spawn pick takes the first default class.
    rm -f "$userdata/ConquerOps.profile"
    # A capture every half second (the shared default above is every 5 s), to catch the impacts.
    export ZHBO_AUTOPILOT=hipfire GGC_AUTO_EXIT_SECONDS=${GGC_AUTO_EXIT_SECONDS:-40} GGC_BGFX_SCREENSHOT_INTERVAL=${ZHBO_HIPFIRE_SHOT_INTERVAL:-15}
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "entered first-person" 120 || { echo "the General never entered first person; see $log" >&2; exit 1; }
    for _ in $(seq 60); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    l="$out/ConquerOps.log"
    grep "hip shot:" "$l" >"$out/hipshots.txt" || true
    awk '
        { for (i = 1; i <= NF; i++) { if ($i == "spread") s = $(i + 1); if ($i == "(cone") { split($(i + 1), c, "\\.\\."); mn = c[1]; mx = c[2] }
              if ($i == "moving") m = $(i + 1) + 0; if ($i == "r") r = $(i + 1) } }
        { f = substr($1, 2) + 0; rest = (n == 0 || f - last >= 10); last = f }
        { n++; sum += r; if (r > 1.001) out++; if (m) { mv++; if (mx - s < 0.01) wide++ } else if (rest) { firsts++; if (s - mn < 0.01) atmin++ } }
        END {
            ok = 1
            printf "%-46s %s (%d)\n", "hip shots logged", (n >= 40 ? "PASS" : "FAIL"), n; if (n < 40) ok = 0
            printf "%-46s %s (%d outside)\n", "every round inside its cone", (out == 0 ? "PASS" : "FAIL"), out; if (out) ok = 0
            mean = n ? sum / n : 0
            printf "%-46s %s (mean r %.3f)\n", "BO2 linear radius (mean r 0.5 +- 0.15)", (mean > 0.35 && mean < 0.65 ? "PASS" : "FAIL"), mean; if (!(mean > 0.35 && mean < 0.65)) ok = 0
            printf "%-46s %s (%d of %d)\n", "a first shot from rest flies in the minimum", (firsts >= 12 && atmin == firsts ? "PASS" : "FAIL"), atmin, firsts; if (!(firsts >= 12 && atmin == firsts)) ok = 0
            printf "%-46s %s (%d of %d)\n", "running opens the cone to its maximum", (mv > 0 && wide * 2 >= mv ? "PASS" : "FAIL"), wide, mv; if (!(mv > 0 && wide * 2 >= mv)) ok = 0
            exit ok ? 0 : 1
        }' "$out/hipshots.txt" && ok=0 || ok=1
    head -16 "$out/hipshots.txt"
    echo "screenshots, log and hipshots.txt: $out"
    exit $ok
fi

if [[ "$mode" == "scope" ]]; then
    # The AS50's scope overlay: the autopilot holds ADS past the 0.40 s transition; once BO2's
    # reticle zoom is up the native draw logs "the viewmodel is not drawn under the scope overlay"
    # (the whole viewmodel, as BO2's drawGun = 0) and the HUD draws the 2D overlay (when its art
    # converted). Seed the profile with an AS50 primary ("SC") so the
    # script's pick key selects it (it reuses the classpick row key).
    profile="$userdata/ConquerOps.profile"
    rm -f "$profile"
    cat >"$profile" <<'EOF'
v2
active 1
set 1 CLASS SET 1
class
SC
primary=as50_mp;secondary=fnp45_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Toughness,FastHands,Dexterity
end
EOF
    export ZHBO_AUTOPILOT=scope GGC_AUTO_EXIT_SECONDS=${GGC_AUTO_EXIT_SECONDS:-30}
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "the viewmodel is not drawn under the scope overlay" 90 || { echo "the scope overlay never came up; see $log" >&2; exit 1; }
    for _ in $(seq 20); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    echo "scope overlay: the viewmodel is not drawn under the overlay   PASS"
    echo "screenshots and log: $out"
    exit 0
fi

if [[ "$mode" == "flicker" ]]; then
    # Flicker in a still scene: no enemies; the General (an MP7, whose sights carry BO2's
    # polygon-offset tritium decals) stands and the autopilot holds the view still, then nudges it
    # a fifth of a pixel every logic frame, facing the open ground (terrain, roads) and then the
    # Command Center (its shadow and ground decals). Every logic frame is captured; the render is
    # capped at the logic rate so each capture is one rendered frame. Consecutive captures must
    # agree in the world (the region left of the gun, below the HUD): z-fighting flips whole
    # patches, a correct picture moves by a fraction of a pixel (`zhbo-assets flicker`).
    profile="$userdata/ConquerOps.profile"
    options="$userdata/Options.ini"
    rm -f "$profile"
    cat >"$profile" <<'EOF'
v2
active 1
set 1 CLASS SET 1
class
Flicker
primary=mp7_mp;secondary=fiveseven_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Lightweight,FastHands,Dexterity
end
EOF
    keep="$out/Options.ini.player"
    rm -f "$keep"
    [[ -f "$options" ]] && cp -p "$options" "$keep"
    put_back() { if [[ -f "$keep" ]]; then cp -p "$keep" "$options"; else rm -f "$options"; fi; }
    trap 'finish || true; put_back' EXIT
    { [[ -f "$keep" ]] && grep -v '^MaxRenderFPS' "$keep"; echo "MaxRenderFPS = 30"; } >"$options"
    export ZHBO_AUTOPILOT=flicker GGC_AUTO_EXIT_SECONDS=${GGC_AUTO_EXIT_SECONDS:-60}
    export GGC_BGFX_SCREENSHOT_AFTER=${FLICKER_AFTER:-60} GGC_BGFX_SCREENSHOT_INTERVAL=1
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "autopilot flicker: phase 5" 120 || { echo "the flicker run never finished; see $log" >&2; exit 1; }
    finish
    put_back
    trap - EXIT
    l="$out/ConquerOps.log"
    grep -E "autopilot flicker|render rate" "$l"
    ok=0
    phase_frame() { sed -nE "s/.*autopilot flicker: phase $1 .* from logic frame ([0-9]+).*/\1/p" "$l" | head -1; }
    # The world: the ground left of the gun and below the skyline (the distant trees sway and the
    # base's flag waves: motion, not flicker). The sight: the gun's quarter, where the MP7's tritium
    # dots must glow in every frame (a dot without BO2's polygon offset loses to the sight under it).
    rect="${FLICKER_RECT:-0 300 680 620}"
    sight="${FLICKER_SIGHT:-640 300 1280 720}"
    limit=${FLICKER_LIMIT:-40}
    for p in 0 1 3 4; do
        from=$(phase_frame $p); to=$(phase_frame $((p + 1)))
        name=$(sed -nE "s/.*autopilot flicker: phase $p \(([^)]*)\).*/\1/p" "$l" | head -1)
        if [[ -z "$from" || -z "$to" ]]; then
            printf '%-40s FAIL (no phase lines)\n' "phase $p"; ok=1; continue
        fi
        # Three frames for the view to settle on the phase's look.
        # shellcheck disable=SC2086
        summary=$(cd "$repo" && cargo run -q --release -p zhbo-assets -- flicker "$out" $((from + 3)) $((to - 1)) $rect | tee "$out/flicker-phase$p.txt" | tail -1)
        worst=$(echo "$summary" | sed -nE 's/.*worst ([0-9]+) flipped.*/\1/p')
        if [[ -n "$worst" && "$worst" -le "$limit" ]]; then
            printf '%-40s PASS (%s)\n' "$name: no flicker" "$summary"
        else
            printf '%-40s FAIL (%s)\n' "$name: no flicker" "$summary"; ok=1
        fi
        # shellcheck disable=SC2086
        glow=$(cd "$repo" && cargo run -q --release -p zhbo-assets -- flicker "$out" $((from + 3)) $((to - 1)) $sight --glow | tee "$out/flicker-sight$p.txt" | tail -1)
        least=$(echo "$glow" | sed -nE 's/.*least ([0-9]+) glowing.*/\1/p')
        most=$(echo "$glow" | sed -nE 's/.*most ([0-9]+) at.*/\1/p')
        if [[ -n "$least" && -n "$most" && "$most" -ge 20 && $((least * 10)) -ge $((most * 6)) ]]; then
            printf '%-40s PASS (%s)\n' "  ...the sight's tritium steady" "$glow"
        else
            printf '%-40s FAIL (%s)\n' "  ...the sight's tritium steady" "$glow"; ok=1
        fi
    done
    echo "screenshots and log: $out"
    exit $ok
fi

if [[ "$mode" == "w3d-ads" ]]; then
    # The W3D fallback's ADS layer: the authored 0x55/0x56 clip composed over the action clip
    # (ZHBO_NATIVE_VIEWMODEL=0 forces the W3D path). The scope autopilot enters ADS; the bridge
    # logs the layer when it plays, and the slide is skipped because the clip owns the pose.
    export ZHBO_NATIVE_VIEWMODEL=0 ZHBO_AUTOPILOT=scope GGC_AUTO_EXIT_SECONDS=${GGC_AUTO_EXIT_SECONDS:-30}
    "$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
    wait_for_log "entered first-person" 90 || { echo "the General never entered first person; see $log" >&2; exit 1; }
    for _ in $(seq 20); do [[ -n "$(game_pid)" ]] || break; sleep 1; done
    finish
    trap - EXIT
    ok=0
    check() { # name, pattern
        if grep -qE "$2" "$out/ConquerOps.log"; then printf '%-28s PASS\n' "$1"; else printf '%-28s FAIL\n' "$1"; ok=1; fi
    }
    check "w3d ADS layer played" "viewmodel: ADS layer"
    check "w3d slide skipped" "viewmodel: ADS slide skipped"
    echo "screenshots and log: $out"
    exit $ok
fi

# Real-input test.
drive="$out/drive"
[[ "$drive" -nt "$repo/scripts/macos/drive.swift" ]] || swiftc -O "$repo/scripts/macos/drive.swift" -o "$drive"
export ZHBO_AUTOPILOT=manual GGC_AUTO_EXIT_SECONDS=60
"$dist/conquerops" -win -xres 1280 -yres 720 -loadsave "$save" >"$out/stdout.txt" 2>&1
# The manual scene leaves the player overhead with his General selected; take control the way a
# player does, with the Take Control button's hotkey (T, macOS virtual key 17).
wait_for_log "autopilot: selected the General" 90 || { echo "the manual scene never selected the General; see $log" >&2; exit 1; }
pid="$(game_pid)"
"$drive" "focus:$pid" sleep:1500
front="$(osascript -e 'tell application "System Events" to get unix id of first process whose frontmost is true')"
if [[ "$front" != "$pid" ]]; then
    echo "the game is not the frontmost app; not sending input" >&2
    exit 1
fi
"$drive" tap:17 sleep:500
wait_for_log "entered first-person" 30 || { echo "T did not take control of the General; see $log" >&2; exit 1; }
# First person opens on the class screen (pick-only in a match). Escape on it declines the spawn
# (BO2 never spawns a classless player): the General goes back overhead, and Escape must not also
# open Zero Hour's game menu, or the second T could not take control. Coming back overhead selects
# him again (the core's select_object), so his command bar's T is up; T, then 6 picks the first default class (rows 1-5 are the custom slots;
# macOS virtual keys: Escape 53, 6 is 22).
"$drive" sleep:1500 tap:53 sleep:500
wait_for_log "returned General [0-9]* to RTS control" 30 || { echo "Escape did not send the classless General back overhead; see $log" >&2; exit 1; }
wait_for_log "selected General [0-9]* back overhead" 30 || { echo "the General was not selected again back overhead; see $log" >&2; exit 1; }
"$drive" sleep:1500 tap:17 sleep:500
for _ in $(seq 30); do [[ "$(grep -c "entered first-person" "$log")" -ge 2 ]] && break; sleep 1; done
[[ "$(grep -c "entered first-person" "$log")" -ge 2 ]] || { echo "the second T did not take control (did Escape open the game menu?); see $log" >&2; exit 1; }
"$drive" sleep:1500 tap:22 sleep:1000
wait_for_log "assumed the" 30 || { echo "6 did not pick a class; see $log" >&2; exit 1; }
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
if grep -qE "keys held: \[W ?\]" "$out/ConquerOps.log"; then echo "W reached the game    PASS"; else echo "W reached the game    FAIL"; ok=1; fi
if [[ "$(cut -d' ' -f5-6 <<<"$first")" != "$(cut -d' ' -f5-6 <<<"$last")" ]]; then echo "moved                 PASS"; else echo "moved                 FAIL"; ok=1; fi
if [[ "$(awk '{print $9}' <<<"$first")" != "$(awk '{print $9}' <<<"$last")" ]]; then echo "turned                PASS"; else echo "turned                FAIL"; ok=1; fi
echo "first pose: $first"
echo "last pose:  $last"
echo "screenshots and log: $out"
exit $ok
