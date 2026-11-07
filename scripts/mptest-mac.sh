#!/usr/bin/env bash
# Conquer Ops two-player LAN test on one Mac: two game instances host and join a LAN game by
# themselves, each player's autopilot plays his own General through the network messages only, and
# both logs must agree on the sim's state hash for every logic frame they both ran.
#
#   scripts/mptest-mac.sh            (also: scripts/autotest-mac.sh --mp)
#
# What each instance does (ZHBO_AUTOPILOT=mp; host/zhbo/ZhboAutopilot.cpp mpAutopilotFrame and
# zhbo-core client/autopilot.rs mp_input): order a General from its Command Center, walk him to a
# meeting point between the two bases, take control, pick a class (the host the custom class this
# script seeds, with attachments and a camo; the joiner the first preset), step forward and aside,
# swap to the sidearm and fire it, swap back, throw the lethal, fire the primary at the other
# General (the host a long aimed burst, the joiner a single round), and open the field computer. The
# joiner's General dies; a new one comes from his Command Center and is taken again: every General
# is picked for (the deliberate break from BO2's respawn, sim/spawn.rs), so his class screen is up,
# opened on the last class, and the script's Enter re-picks it.
#
# Pass: both instances entered the network game with the same content CRC, every scripted step
# shows in both logs (each machine logs every player's sim events), Zero Hour never reported a CRC
# mismatch, the "state hash: frame N <fingerprint>" lines (ZHBO_STATE_HASH_LOG) agree on every common frame,
# and each machine drew the other player's General with the weapon and camo that player holds
# ("native body: General G weapon V camo C composed" against the other log's "... in hand").
#
# How two instances fit on one Mac (host patch 0022, docs/TESTING.md "Multiplayer in game"):
#   * GGC_NETWORK_AUTOSTART=1 lets the release build use the engine's -autoNetworkMode driver, which
#     also turns on multi-instance (instance ids 2 and 3, addresses 127.0.0.2 and 127.0.0.3);
#   * GGC_LOOPBACK_INSTANCE_PORTS=1 keeps those addresses as identities but puts both instances on
#     127.0.0.1 with per-instance ports, so no `sudo ifconfig lo0 alias` is needed.
#   * Each instance after the first writes ConquerOps_InstanceNN.log / .profile (user data dir).
#
# The long run (ZHBO_MPTEST_ROUNDS=N, any N > 1; ZHBO_AUTOPILOT_ROUNDS in both instances): the joiner
# plays N lives and the host plays until the joiner has left, every life walked to the meeting point. The target first declines his spawn (Escape
# on the class screen; the bridge takes control again). Every life after the first fight throws the
# tactical at the other General (a concussion: a shellshock on both machines' sims), fires, reloads,
# knifes and goes back overhead by the field computer, unless he dies (the host ends each round with a
# long aimed burst). In his third life (his next-to-last when he has fewer than four: the first fight
# for N = 2) the target changes class mid-life after his throw and stays for the bursts (the next
# General's screen opens on it, and his Enter wears it). Every life of both players picks. In his last life he leaves the
# game in first person (the quit menu's Exit), and the host must carry on alone. The long run's own
# checks: the decline, the rounds, the same shellshocks on both machines, the class change worn,
# and the host past the leave (his General dies, the host's sim runs on, no mismatch).
#
# Both runs also check that each machine presented the other player: his gunshots and footsteps
# were played ("sound ZHBO_BO2_FireNpc_…: playing", "sound ZHBO_BO2_StepNpc: playing|no sound").
#
# Environment: ZHBO_ZH_DIR / ZHBO_BO2_DIR as for the launcher; DIST for a staged build other
# than the default dist; OUT for the results folder; ZHBO_MPTEST_MAP (default Tournament Desert);
# ZHBO_MPTEST_ROUNDS (default 1, the short run); ZHBO_MPTEST_SHOTS=1 captures both screens (every
# ZHBO_MPTEST_SHOT_INTERVAL logic frames, default 150); ZHBO_MPTEST_SECONDS, the wall-clock limit (default
# 480, the long run 1800).
set -euo pipefail

dist="${DIST:-$HOME/Library/Application Support/ConquerOps/dist}"
userdata="$HOME/Library/Application Support/Command and Conquer Generals Zero Hour Data"
out="${OUT:-${TMPDIR:-/tmp}/ConquerOps-mp}"
map="${ZHBO_MPTEST_MAP:-maps/tournament desert/tournament desert.map}"
rounds="${ZHBO_MPTEST_ROUNDS:-1}"
long=0
(( rounds > 1 )) && long=1
limit="${ZHBO_MPTEST_SECONDS:-$(( long ? 1800 : 480 ))}"
# -autoNetworkMode skips the primary instance slot: the host is instance 2, the joiner 3.
host_log="$userdata/ConquerOps_Instance02.log"
join_log="$userdata/ConquerOps_Instance03.log"
host_profile="$userdata/ConquerOps_Instance02.profile"
join_profile="$userdata/ConquerOps_Instance03.profile"
mkdir -p "$out"
rm -f "$out"/*.log "$out"/*.txt "$out"/*.csv

if [[ ! -x "$dist/conquerops" ]]; then
    echo "no staged build at $dist (cargo xtask build, or set DIST)" >&2
    exit 2
fi

game_pids() { pgrep -f "$dist/generalszh" || true; }
stop_games() {
    pkill -f "$dist/generalszh" 2>/dev/null || true
    for _ in $(seq 60); do [[ -z "$(game_pids)" ]] && return 0; sleep 0.5; done
    pkill -9 -f "$dist/generalszh" 2>/dev/null || true
    sleep 1
}
finish() {
    stop_games
    cp "$host_log" "$out/host.log" 2>/dev/null || true
    cp "$join_log" "$out/join.log" 2>/dev/null || true
    rm -f "$host_profile" "$join_profile"
    # (Without captures the glob matches nothing: the loop must not end on a failed test, or
    # `set -e` ends the script here, before any check.)
    for f in "$out"/*-shot.bmp.*.bmp; do
        [[ -f "$f" ]] || continue
        sips -s format png "$f" --out "${f%.bmp}.png" >/dev/null 2>&1 && rm "$f"
    done
    return 0
}
stop_games
rm -f "$host_log" "$join_log"
trap finish EXIT

# The host's custom class: attachments and a camo, so the other machine's third person has
# something to get right. The joiner has no custom (he picks the first preset).
cat >"$host_profile" <<'EOF'
v2
active 1
set 1 CLASS SET 1
class
MP Test
primary=an94_mp+gl+reflex;secondary=fnp45_mp;lethal=frag_grenade_mp;tactical=concussion_grenade_mp;melee=knife_held_mp;perks=Lightweight,FastHands,Dexterity;camo=camo_gold
end
EOF
rm -f "$join_profile"

# The launcher converts the BO2 art on its first run; do it once here, not in both instances.
"$dist/conquerops" --check >"$out/check.txt" 2>&1 || { cat "$out/check.txt" >&2; exit 2; }

# Both instances: the same ZHBO_AUTOPILOT and ZHBO_LOADOUT (both are in the content CRC the LAN
# join compares); the role only scripts the local player's own input.
export GGC_NETWORK_AUTOSTART=1 GGC_LOOPBACK_INSTANCE_PORTS=1 GGC_NO_FOCUS=1 GGC_AUTO_EXIT_SECONDS="$limit"
export ZHBO_AUTOPILOT=mp ZHBO_STATE_HASH_LOG=1 ZHBO_AUTOPILOT_ROUNDS="$rounds"
unset ZHBO_LOADOUT
window=(-win -xres 960 -yres 540)

wait_for() { # file, pattern, seconds
    for _ in $(seq "$3"); do
        grep -q "$2" "$1" 2>/dev/null && return 0
        sleep 1
    done
    return 1
}

# ZHBO_MPTEST_SHOTS=1: each instance captures its own screen every few seconds (the engine's
# capture, as the autotest's), host-shot.* and join-shot.*: what each machine drew of the other.
# ZHBO_MPTEST_SHOT_INTERVAL sets the logic frames between captures (default 150, 5 s; host patch
# 0027 counts logic frames).
shots() { # name
    if [[ -n "${ZHBO_MPTEST_SHOTS:-}" ]]; then
        echo GGC_BGFX_SCREENSHOT_AFTER=300 GGC_BGFX_SCREENSHOT_INTERVAL="${ZHBO_MPTEST_SHOT_INTERVAL:-150}" "GGC_BGFX_SCREENSHOT_PATH=$out/$1-shot.bmp"
    fi
}
rm -f "$out"/*-shot.*

env $(shots host) ZHBO_AUTOPILOT_ROLE=shooter "$dist/conquerops" "${window[@]}" -autoNetworkMode direct -autoNetworkHost 2 \
    -autoNetworkMap "$map" -autoNetworkName MPHost -autoNetworkTimeout 240 >"$out/host-stdout.txt" 2>&1
# The joiner must start after the host holds instance slot 2 (its log opens after that).
wait_for "$host_log" "zhbo-core" 120 || { echo "the host never started; see $out/host-stdout.txt" >&2; exit 1; }
env $(shots join) ZHBO_AUTOPILOT_ROLE=target "$dist/conquerops" "${window[@]}" -autoNetworkMode direct -autoNetworkJoin 127.0.0.2 \
    -autoNetworkName MPJoin -autoNetworkTimeout 240 >"$out/join-stdout.txt" 2>&1

# Until both runs are done (or one fails to start a game, or the limit).
started=$(date +%s)
hit_limit=0
while true; do
    if grep -q "NetworkAutoStart failed" "$out/host-stdout.txt" "$out/join-stdout.txt" 2>/dev/null; then
        grep -h "NetworkAutoStart failed" "$out/host-stdout.txt" "$out/join-stdout.txt" >&2
        exit 1
    fi
    grep -q "autopilot mp: done" "$host_log" 2>/dev/null && grep -q "autopilot mp: done" "$join_log" 2>/dev/null && break
    # The long run's joiner has left: the host plays on alone for a minute at most.
    if (( long )) && grep -q "leaving the game in first person" "$join_log" 2>/dev/null; then
        left_at=${left_at:-$(date +%s)}
        (( $(date +%s) - left_at > 60 )) && break
    fi
    [[ -z "$(game_pids)" ]] && break
    if (( $(date +%s) - started > limit )); then hit_limit=1; break; fi
    sleep 2
done
# A few seconds for the last frames' hashes on both sides.
sleep 3
finish
trap - EXIT

h="$out/host.log"
j="$out/join.log"
ok=0
pass() { printf '%-46s PASS\n' "$1"; }
fail() { printf '%-46s FAIL%s\n' "$1" "${2:+  ($2)}"; ok=1; }
both() { # name, pattern
    if grep -qE "$2" "$h" 2>/dev/null && grep -qE "$2" "$j" 2>/dev/null; then pass "$1"; else fail "$1"; fi
}
in_log() { # name, log, pattern
    if grep -qE "$3" "$2" 2>/dev/null; then pass "$1"; else fail "$1"; fi
}

if grep -q "NetworkAutoStart entered the network game" "$out/host-stdout.txt" && grep -q "NetworkAutoStart entered the network game" "$out/join-stdout.txt"; then
    pass "both entered the LAN game"
else
    fail "both entered the LAN game"
fi
crc_h=$( (grep -o "content CRC [0-9A-F]*" "$h" 2>/dev/null || true) | head -1)
crc_j=$( (grep -o "content CRC [0-9A-F]*" "$j" 2>/dev/null || true) | head -1)
if [[ -n "$crc_h" && "$crc_h" == "$crc_j" ]]; then pass "same content CRC ($crc_h)"; else fail "same content CRC" "$crc_h / $crc_j"; fi
both "both players took control" "General [0-9]+ entered first-person control"
for side in "$h" "$j"; do
    n=$( (grep -oE "General [0-9]+ entered first-person control" "$side" 2>/dev/null || true) | awk '{print $2}' | sort -u | wc -l | tr -d ' ')
    if [[ $n -ge 3 ]]; then pass "both Generals and the respawn in first person (${side##*/})"; else fail "both Generals and the respawn in first person (${side##*/})" "$n Generals"; fi
done
both "the custom class (attachments, camo)" "assumed the MP Test class"
both "a preset class" "assumed the (SMG|[A-Z][A-Za-z ]*) class"
both "swapped weapons" "swapped weapons"
both "threw a lethal" "threw a lethal"
both "a General took fire" "General took [0-9]+ damage"
both "a General died" "General [0-9]+ died"
in_log "the target ordered the next General" "$j" "my General died; ordering the next one"
in_log "the next General was taken" "$j" "first-person session 2"
# Every General is picked for: the joiner's log names a class worn for two Generals at least (the
# script logs "General N wears class C" as each record's class lands).
picked=$( (grep -oE "autopilot mp: General [0-9]+ wears class" "$j" 2>/dev/null || true) | awk '{print $4}' | sort -u | wc -l | tr -d ' ')
if [[ $picked -ge 2 ]]; then pass "the next General picked his class ($picked Generals)"; else fail "the next General picked his class" "$picked Generals classed"; fi
both "opened the field computer" "opened the field computer"
# The long run's host plays until the target has left and Zero Hour has ended the won game.
(( long )) || in_log "the host's run finished" "$h" "autopilot mp: done"
in_log "the joiner's run finished" "$j" "autopilot mp: done"
# Zero Hour's logic CRC carries the sim (host patch 0020: `MARKER:ConquerOps` after the AI): its
# CRC frames log the fingerprint, and the two machines' lines for the same frames are equal.
crc_lines() { grep -oE "crc frame [0-9]+: .*" "$1" 2>/dev/null | sort -u || true; }
crc_lines "$h" >"$out/host-crc.txt"; crc_lines "$j" >"$out/join-crc.txt"
crc_common=$(comm -12 "$out/host-crc.txt" "$out/join-crc.txt" | wc -l | tr -d ' ')
crc_frames_h=$(awk '{print $3}' "$out/host-crc.txt" | sort -u); crc_frames_j=$(awk '{print $3}' "$out/join-crc.txt" | sort -u)
crc_shared=$(comm -12 <(echo "$crc_frames_h") <(echo "$crc_frames_j") | grep -c . || true)
if [[ "$crc_common" -ge 3 && "$crc_common" -eq "$crc_shared" ]]; then
    pass "the logic CRC carries the sim, alike ($crc_common CRC frames)"
else
    fail "the logic CRC carries the sim, alike" "$crc_common equal of $crc_shared shared CRC frames"
fi
# A round at the other first-person General resolved against where the shooter saw him (antilag).
both "a rewound round hit the other General (antilag)" "antilag: General [0-9]+'s round hit General [0-9]+ where he stood"
# Each machine presented the other player's General: his gunshots and his footsteps were played
# here (the core plays both only for another player; the engine logs each event's first play per
# outcome). A step is positional and short-ranged: from overhead or across the map the engine
# answers "no sound" (out of range), which still shows the core played it; "error" (a missing
# event) does not count.
both "another player's gunshot was heard" "sound ZHBO_BO2_FireNpc_[^ ]*: playing"
both "another player's footsteps were played" "sound ZHBO_BO2_StepNpc: (playing|no sound)"
if grep -q "CRC mismatch" "$h" "$j" 2>/dev/null; then fail "no Zero Hour CRC mismatch" "$(grep -h "CRC mismatch" "$h" "$j" | head -1)"; else pass "no Zero Hour CRC mismatch"; fi

# The sim's state hash, frame by frame, on the frames both machines ran: the network game's lines
# only (the shell map's logic logs its own from frame 0 too; the bridge marks each game's start).
match_hashes() { # log -> "frame<TAB>hash world … seats …", by frame
    awk '/game start: a network game/ {keep = 1; n = 0; next} /game start:/ {keep = 0}
         keep && match($0, /state hash: frame [0-9]+ /) {
             head = substr($0, RSTART + 18, RLENGTH - 19); rest = substr($0, RSTART + RLENGTH)
             lines[n++] = head "\t" rest
         }
         END {for (i = 0; i < n; i++) print lines[i]}' "$1" | sort -t $'\t' -k1,1
}
match_hashes "$h" >"$out/host-hashes.txt" || true
match_hashes "$j" >"$out/join-hashes.txt" || true
# The long run's leaver: once his own machine has quit (`leaving the game`, then the bridge makes him
# a spectator as his General goes), it steps its last frames without the other player's commands
# (the network game is over for it), so his seat's input there is not the host's. Compare up to the
# frame before he turned spectator.
gone=$( (awk '/leaving the game in first person/ {l = 1} l && /\] spectator:/ {print; exit}' "$j" 2>/dev/null || true) | sed -E 's/^\[([0-9]+)\].*/\1/')
if [[ -n "$gone" ]]; then
    awk -F '\t' -v g="$gone" '$1 + 0 < g' "$out/join-hashes.txt" >"$out/join-hashes.cut" && mv "$out/join-hashes.cut" "$out/join-hashes.txt"
fi
# Any machine leaving by any other path (the wall-clock limit's exit or kill, a closed window): the
# bridge logs where its quit began. Its MSG_SELF_DESTRUCT still runs in lockstep, but from there on
# that machine steps without the other's commands, so neither log's later frames are comparable.
quit_frame=$( (cat "$h" "$j" 2>/dev/null | sed -nE 's/.*network game: this machine is leaving the game \(frame ([0-9]+)\).*/\1/p' || true) | sort -n | head -1)
if [[ -n "$quit_frame" ]]; then
    for f in "$out/host-hashes.txt" "$out/join-hashes.txt"; do
        awk -F '\t' -v g="$quit_frame" '$1 + 0 < g' "$f" >"$f.cut" && mv "$f.cut" "$f"
    done
fi
join -t $'\t' "$out/host-hashes.txt" "$out/join-hashes.txt" >"$out/hashes-joined.txt" || true
common=$(wc -l <"$out/hashes-joined.txt" | tr -d ' ')
differ=$(awk -F '\t' '$2 != $3' "$out/hashes-joined.txt" | wc -l | tr -d ' ')
first=$(awk -F '\t' '$2 != $3 {print $1}' "$out/hashes-joined.txt" | sort -n | head -1)
if [[ "$common" -ge 1500 && "$differ" -eq 0 ]]; then
    pass "state hash agrees on all $common common frames"
else
    fail "state hash agrees frame by frame" "$common common, $differ differ${first:+, first at frame $first}"
    # The fingerprint's parts name the seat (or the world) that went apart.
    [[ -n "$first" ]] && awk -F '\t' -v f="$first" '$1 == f {print "    host: " $2; print "    join: " $3}' "$out/hashes-joined.txt"
fi

# Each machine draws the other player's General with what that player holds.
# (Up to the long run's leave: after it the joiner's machine is gone.)
left=$( (grep -oE "leaving the game in first person \(General [0-9]+, frame [0-9]+\)" "$j" 2>/dev/null || true) | head -1)
left_general=$(sed -E 's/.*General ([0-9]+),.*/\1/' <<<"$left")
left_frame=$(sed -E 's/.*frame ([0-9]+)\)/\1/' <<<"$left")
remote_look() { # name, the holder's log, the viewer's log
    local held seen=0 missing=0 unseen= line g v c
    held=$( (grep -E "autopilot mp: General [0-9]+ weapon [0-9]+ camo [A-Za-z0-9_]+ in hand" "$2" || true) | awk -v f="${left_frame:-999999999}" -F'[][]' '$2 + 0 < f' | grep -oE "autopilot mp: General [0-9]+ weapon [0-9]+ camo [A-Za-z0-9_]+ in hand" | sort -u || true)
    while read -r line; do
        [[ -z "$line" ]] && continue
        g=$(awk '{print $4}' <<<"$line"); v=$(awk '{print $6}' <<<"$line"); c=$(awk '{print $8}' <<<"$line")
        if grep -q "native body: General $g weapon $v camo $c composed" "$3"; then seen=$((seen + 1));
        # Zero Hour's shroud hides a General the viewer's units cannot see, and the native pass with it
        # (as it hides the W3D body): a look he never showed in sight is not a missing draw.
        elif grep -q "native body: General $g out of the local player's sight" "$3"; then unseen=$((unseen + 1)); echo "    out of sight on the other machine: General $g weapon $v camo $c";
        else missing=$((missing + 1)); echo "    not drawn on the other machine: General $g weapon $v camo $c"; fi
    done <<<"$held"
    if [[ $seen -ge 1 && $missing -eq 0 ]]; then pass "$1 ($seen looks${unseen:+, $unseen out of sight})"; else fail "$1" "$seen drawn, $missing missing"; fi
}
remote_look "the joiner draws the host's weapon and camo" "$h" "$j"
remote_look "the host draws the joiner's weapon and camo" "$j" "$h"
if grep -qE "autopilot mp: General [0-9]+ weapon [0-9]+ camo camo_gold in hand" "$h"; then pass "the host's custom wears its camo"; else fail "the host's custom wears its camo"; fi

if (( long )); then
    in_log "the target declined his first spawn" "$j" "declining the spawn"
    in_log "and was taken into first person again" "$j" "back overhead; taking control again"
    # The joiner has N - 1 lives after the first fight (the host plays his alongside): at least
    # two rounds once there is room for them, else the one.
    want_rounds=$(( rounds - 1 < 2 ? rounds - 1 : 2 ))
    for side in "$h" "$j"; do
        n=$( (grep -oE "autopilot mp: round [0-9]+ starts" "$side" 2>/dev/null || true) | wc -l | tr -d ' ')
        if [[ $n -ge $want_rounds ]]; then pass "rounds after the first fight (${side##*/}: $n)"; else fail "rounds after the first fight (${side##*/})" "$n of $want_rounds"; fi
    done
    # The same shellshocks, frame for frame, in both logs (up to the joiner's leave).
    shocks() { grep -oE "^\[[0-9]+\] player [0-9]+'s General is shellshocked.*" "$1" 2>/dev/null | sort -u || true; }
    shocks "$h" | awk -v f="${left_frame:-999999999}" -F'[][]' '$2 + 0 < f' >"$out/host-shocks.txt"
    shocks "$j" >"$out/join-shocks.txt"
    n=$(wc -l <"$out/join-shocks.txt" | tr -d ' ')
    # At N = 2 the joiner's only round is his last life, which he leaves a few seconds in, before
    # the round's concussions land: the two logs must still agree, but may hold none.
    want_shocks=$(( rounds >= 3 ? 1 : 0 ))
    if [[ $n -ge $want_shocks ]] && cmp -s "$out/host-shocks.txt" "$out/join-shocks.txt"; then
        pass "the same shellshocks on both machines ($n)"
    else
        fail "the same shellshocks on both machines" "$(wc -l <"$out/host-shocks.txt" | tr -d ' ') vs $n"
    fi
    in_log "the target changed class mid-life" "$j" "changing class mid-life after combat"
    # Both machines told him; his next General's pick wore it (his screen opened on it: the first
    # class line after the change is the next General's, since the kit stays for this one).
    next=$( (grep -oE "the [A-Za-z0-9 ]+ class comes in at the next spawn" "$j" || true) | tail -1 | sed -E 's/^the (.*) class comes in.*/\1/')
    worn=$(awk '/class comes in at the next spawn/ {after = 1} after && /autopilot mp: General [0-9]+ wears class / {sub(/.*wears class /, ""); print; exit}' "$j")
    if [[ -n "$next" ]] && grep -q "the $next class comes in at the next spawn" "$h" && [[ "$(tr '[:upper:]' '[:lower:]' <<<"$worn")" == "$(tr '[:upper:]' '[:lower:]' <<<"$next")" ]]; then
        pass "the next General wore the new class ($next)"
    else
        fail "the next General wore the new class" "${next:-no next-spawn line} / worn ${worn:-nothing}"
    fi
    if [[ -n "$left_frame" ]]; then
        pass "the joiner left in first person (General $left_general, frame $left_frame)"
        if grep -qE "General $left_general died|returned General $left_general to RTS" "$h"; then pass "the host let go of the leaver's General"; else fail "the host let go of the leaver's General"; fi
        # With nobody left against him the host wins and Zero Hour ends his game a few seconds on.
        last=$(awk '/game start: a network game/ {keep = 1} keep && match($0, /state hash: frame [0-9]+ /) {f = substr($0, RSTART + 18, RLENGTH - 19) + 0; if (f > m) m = f} END {print m + 0}' "$h")
        if (( last >= left_frame + 60 )); then pass "the host played on alone (to frame $last)"; else fail "the host played on alone" "last frame $last"; fi
    else
        fail "the joiner left in first person"
    fi
fi

# A run the wall-clock limit ended (ZHBO_MPTEST_SECONDS; the engine's GGC_AUTO_EXIT_SECONDS at the
# same limit): the checks above judged a cut-short match. Say so, with where each log stopped.
if (( hit_limit )); then
    last_of() { awk '/game start: a network game/ {keep = 1} keep && match($0, /state hash: frame [0-9]+ /) {f = substr($0, RSTART + 18, RLENGTH - 19) + 0; if (f > m) m = f} END {print m + 0}' "$1" 2>/dev/null; }
    last_h=$(last_of "$h"); last_j=$(last_of "$j")
    fail "finished inside the wall-clock limit" "${limit} s ran out at host frame ${last_h:-?}, joiner frame ${last_j:-?}${quit_frame:+; a machine began leaving at frame $quit_frame}"
fi

echo "logs: $out (host.log, join.log, *-stdout.txt, hashes-joined.txt)"
exit $ok
