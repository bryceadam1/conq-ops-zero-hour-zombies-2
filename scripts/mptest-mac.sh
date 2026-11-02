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
# General (the host a long aimed burst, the joiner a short one), and open the field computer. The
# joiner's General dies; a new one comes from his Command Center with the class and is taken again.
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
# Environment: ZHBO_ZH_DIR / ZHBO_BO2_DIR as for the launcher; DIST for a staged build other
# than the default dist; OUT for the results folder; ZHBO_MPTEST_MAP (default Tournament Desert);
# ZHBO_MPTEST_SECONDS, the wall-clock limit (default 480).
set -euo pipefail

dist="${DIST:-$HOME/Library/Application Support/ConquerOps/dist}"
userdata="$HOME/Library/Application Support/Command and Conquer Generals Zero Hour Data"
out="${OUT:-${TMPDIR:-/tmp}/ConquerOps-mp}"
map="${ZHBO_MPTEST_MAP:-maps/tournament desert/tournament desert.map}"
limit="${ZHBO_MPTEST_SECONDS:-480}"
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
export ZHBO_AUTOPILOT=mp ZHBO_STATE_HASH_LOG=1
unset ZHBO_LOADOUT
window=(-win -xres 960 -yres 540)

wait_for() { # file, pattern, seconds
    for _ in $(seq "$3"); do
        grep -q "$2" "$1" 2>/dev/null && return 0
        sleep 1
    done
    return 1
}

ZHBO_AUTOPILOT_ROLE=shooter "$dist/conquerops" "${window[@]}" -autoNetworkMode direct -autoNetworkHost 2 \
    -autoNetworkMap "$map" -autoNetworkName MPHost -autoNetworkTimeout 240 >"$out/host-stdout.txt" 2>&1
# The joiner must start after the host holds instance slot 2 (its log opens after that).
wait_for "$host_log" "zhbo-core" 120 || { echo "the host never started; see $out/host-stdout.txt" >&2; exit 1; }
ZHBO_AUTOPILOT_ROLE=target "$dist/conquerops" "${window[@]}" -autoNetworkMode direct -autoNetworkJoin 127.0.0.2 \
    -autoNetworkName MPJoin -autoNetworkTimeout 240 >"$out/join-stdout.txt" 2>&1

# Until both runs are done (or one fails to start a game, or the limit).
started=$(date +%s)
while true; do
    if grep -q "NetworkAutoStart failed" "$out/host-stdout.txt" "$out/join-stdout.txt" 2>/dev/null; then
        grep -h "NetworkAutoStart failed" "$out/host-stdout.txt" "$out/join-stdout.txt" >&2
        exit 1
    fi
    grep -q "autopilot mp: done" "$host_log" 2>/dev/null && grep -q "autopilot mp: done" "$join_log" 2>/dev/null && break
    [[ -z "$(game_pids)" ]] && break
    (( $(date +%s) - started > limit )) && break
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
in_log "the next General came in with the class" "$j" "first-person session 2"
both "opened the field computer" "opened the field computer"
in_log "the host's run finished" "$h" "autopilot mp: done"
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
remote_look() { # name, the holder's log, the viewer's log
    local held seen=0 missing=0 line g v c
    held=$(grep -oE "autopilot mp: General [0-9]+ weapon [0-9]+ camo [A-Za-z0-9_]+ in hand" "$2" | sort -u || true)
    while read -r line; do
        [[ -z "$line" ]] && continue
        g=$(awk '{print $4}' <<<"$line"); v=$(awk '{print $6}' <<<"$line"); c=$(awk '{print $8}' <<<"$line")
        if grep -q "native body: General $g weapon $v camo $c composed" "$3"; then seen=$((seen + 1)); else missing=$((missing + 1)); echo "    not drawn on the other machine: General $g weapon $v camo $c"; fi
    done <<<"$held"
    if [[ $seen -ge 1 && $missing -eq 0 ]]; then pass "$1 ($seen looks)"; else fail "$1" "$seen drawn, $missing missing"; fi
}
remote_look "the joiner draws the host's weapon and camo" "$h" "$j"
remote_look "the host draws the joiner's weapon and camo" "$j" "$h"
if grep -qE "autopilot mp: General [0-9]+ weapon [0-9]+ camo camo_gold in hand" "$h"; then pass "the host's custom wears its camo"; else fail "the host's custom wears its camo"; fi

echo "logs: $out (host.log, join.log, *-stdout.txt, hashes-joined.txt)"
exit $ok
