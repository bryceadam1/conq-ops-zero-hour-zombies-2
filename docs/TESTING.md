# Conquer Ops — Testing

How each layer of the mod is verified, and what to run before declaring a change done.

## Layers

| Layer | What it covers | How |
|---|---|---|
| Unit tests | sim, weapons, movement, hitscan, HUD, formats, requirements | `cargo test --workspace` |
| Multiplayer content fingerprint | only the weapon table's sim fields reach it: every presentation field scrambled (a localized or DLC-richer install) leaves it alone, a sim number or a row moves it (`content::tests`); no sim-reachable source reads a presentation field's member (`the_sim_reads_no_presentation_field`); the converter's DLC lock and DLC camo passes leave it alone (`zhbo-assets` `content_tests`), and over the real install laid out without the DLC zones, with Revolution's zone present and without the english zones (ignored, real-install suite). `zhbo-assets content <weapons.ini>...` prints two tables' parts to compare | `cargo test -p zhbo-core content`, `cargo test -p zhbo-assets content_tests` |
| Multiplayer determinism (the lockstep gate) | one match on four machines fed the same replicated command stream with Zero Hour's run-ahead (changing over the match as the engine adapts it: 30 as it opens, then 12, 4, 8, 4, 6, 12 frames, once a round at its frame 90 while the players' inputs stream in and before the script's first press, so inputs pile onto one frame or leave one empty and every beat keeps its room), the players' clients predicting their own Generals with the engine at hand (`Client::frame_with`; each player's machine must have predicted most of the match): player 0's machine renders a ragged 1–3 client frames per logic frame, player 1's one, a third (a headless replay check or a minimised observer) none, so its `Sim::events` sit at their cap, and a fourth is a spectating observer (`ClientIn.spectating`) rendering 1–2 frames that follows the first-person Generals, switches with its buttons, goes overhead and back while the engine's observed seat changes, and must never send input, a class pick or a selection, capture the mouse or predict (the engine's logic at hand); each machine's own `Client` makes its own player's input from scripted keys and mouse and does its own presentation (drains the events, plays the tracers for its own viewer). Rounds (660 frames, the beats spaced for the slowest real items) cycle every slot weapon of the catalog through custom classes sent as wire words (presets by index in round 0): fire, ADS, reload, sprint, both offhands, melee, the alt mode, the Hybrid optic's aimed toggle, the swap, stances, a mid-life pick after combat, the field computer and the overhead RTS report, then a death and a fresh General. Every frame every machine's `Sim::fingerprint` (the hash Zero Hour's CRC carries, with its per-seat parts) and every engine call (objects, damage, control, spawns, destroys, disables) must agree; a single-peer run asserts the systems were reached. The hash itself: every leaf of a fully populated state (every field of two seats' records and classes, a projectile, equipment, a smoke, a deployed shield, the antilag history; ~790 leaves) changes it and exactly its own seat's or the world's part (`every_field_of_the_state_changes_the_hash_and_its_own_part`, through the test-only `StateHash::perturb`); presentation and scratch do not; equal states built apart (clones, a definition at another address) hash equal; an idle sim is not in the CRC; a mod-only divergence names its seat; the stream's hash is pinned (`statehash::tests`); `zhbo_logic_crc` reports the sim's fingerprint over the ABI (`ffi::tests`). A second match sends only hostile pick payloads (noise, an every-rule-broken class, a legal class with one field corrupted), and a 4,000-payload sweep asserts the sim wears exactly the repaired, legal class or drops the pick (and keeps a DLC item or camo the receiver lacks). The harness (`sim/harness.rs`, `sim/fakehost.rs`) also runs a short three-player match of `roster_classes` (every offered item and attachment, as legal Pick-10 classes). **The full-roster gate** runs the same harness over the player's own install (zhbo-core's `harness` feature, a zhbo-assets dev-dependency): the launcher's converted weapon table (`generals::files`), 230 classes covering every primary and secondary item with every attachment, every alt mode (the launchers, select fire, the Hybrid optic's ADS toggle), every lethal and tactical, the crossbow's explosive bolt and the stun grenades' shellshocks, DLC items included, killstreak-free, on three players' machines (seats 0–2, ragged/steady/skipping render cadences) plus one that never renders and a spectating observer, 78 rounds (~51,000 frames) with the hash and every engine call equal on every frame; it fails naming whatever the roster did not reach (~5 min release, most of it the conversion) | `cargo test -p zhbo-core lockstep statehash logic_crc` (`sim/lockstep_tests.rs`, `statehash.rs`, `ffi.rs`); `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_install_roster_plays_in_lockstep -- --ignored --nocapture` |
| First-person latency (the latency harness) | one machine at run-ahead 1 (a skirmish), 4, 8, 12 and 30, rendered at 120 and 30 Hz, with the prediction and without: the milliseconds from a key going down to the camera moving, the muzzle flash, the fire sound, the HUD's ammo, the hit marker, the zoom and the reload's clip (predicted: the same rendered frame for the cues, the next for the camera and the zoom, at every run-ahead; unpredicted: the run-ahead), and the predictions the sim disagreed with (none in a still world); a changing run-ahead mispredicts the inputs in flight at each change and snaps nothing; the antilag's hit rate on a strafing General tracked as the shooter's machine shows him (with the rewind ≥ 0.9 at 4–12 frames; without, far less). Also the prediction-is-the-sim test (`sim::predict`), the antilag's frame choice (`sim::antilag`) and the predictor's reconcile (`client::predict`) | `cargo test -p zhbo-core latency -- --nocapture` (`sim/latency_tests.rs`, prints the table) |
| Multiplayer in game | two instances of the release build host and join a LAN game on one Mac (no root: loopback instance ports), each player's autopilot fights the other's General (custom class with attachments and a camo against a preset; sidearm, lethal, primary, a death and a respawn with the class, the field computer); both logs' per-frame `state hash` lines must agree, Zero Hour's CRC check must stay quiet, and each machine must draw the other's weapon and camo | `scripts/mptest-mac.sh` (= `autotest-mac.sh --mp`; "Multiplayer in game" below) |
| Shot impacts (R8d) | the surface vocabulary and the converted table (classification, default-cell and blood-variant fallbacks); the sim's per-pellet `Impact` event and its absence on a miss; the client record and its cap; the ABI lookup; every converted gun's bucket and cells resolve to a converted FXList | `cargo test -p zhbo-core impacts`; `cargo test --workspace`; the real-install check: `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets converts_from_real_install -- --ignored` (asserts each gun's `ImpactType`, `pellets <= 8`, and that every row names an `FXList` in the effects INI) |
| Projectile damage | every projectile hurts a BO2 soldier by BO2's data at 150, 600 and 1200 inches, straight at him and into the ground beside him: the strike's `iDamage`, the impact fuse and its arming distance, a bolt's spawned grenade going off on its fuse, a sticky grenade's fuse, the blast's falloff, BO2's kill verdict (`conformance::projectile`, its synthetic crossbow/rocket/launcher-grenade/grenade tests and the built-in table); the sim's bolt striking then its grenade going off in the victim (`sim::tests::a_crossbow_bolt_strikes_then_its_spawned_grenade_goes_off_in_him`); the converter's `SpawnedGrenade` and `stickiness` (`weapons::tests::a_bolt_spawns_its_grenade_and_stickiness_is_bo2s`) | `cargo test -p zhbo-core conformance::projectile sim::tests::a_crossbow`; `cargo test -p zhbo-assets a_bolt_spawns`; the sweep over every converted projectile (the launchers, the under-barrel grenades, every crossbow combination, the ballistic knife, the thrown offhands, `explosive_bolt_mp`) and the defs' spawned grenade and stickiness against the install: `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets every_projectile_hurts_by_bo2s_data -- --ignored --nocapture` |
| Bullet penetration | BO2's walk (`FUN_005d0350`): depth table and perk, chord falloff, the 30 in FX gap and the client's two-effect budget, the ground stopping a round (`penetration::tests`); fake-host shots through a thin wall (half damage behind, the exit impact) and FMJ through a wall that stops the plain round (`sim::tests`); exit rows and exit sound families | `cargo test -p zhbo-core penetration impacts sim::tests::a_round_through`; `cargo test -p zhbo-assets impacts`; the real-install check that the tuning pack's depths are patch_mp's `info/bullet_penetration_mp` and the defs' `penetrateType`: `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_penetration_table_is_the_installs -- --ignored` |
| Sound mixer levels | BO2's mixer at conversion (`zhbo_assets::mixer`): the reference aliases' gains (group chain × bus × pan fold — the hold-breath voice line over the AN-94 shot, a footstep's foley under it, the LFE-only layer silent in stereo), the compressor squashing fx while the HDR bus skips it, start delays and distance curves, the kernel's coefficients; the master EQ (shelf/peak bands on fx, voice untouched); the curve and group walk (`sndglobals`); the one normalization (`sfx`); the third-person shot layer by layer — the crossfade, the row round-trip and the client cue (`soundlayers`) | `cargo test -p zhbo-assets mixer sfx`; `cargo test -p zhbo-formats sndglobals`; `cargo test -p zhbo-core soundlayers`; the menu cues' absence from every MP zone (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_missing_menu_cues_are_in_no_mp_zone -- --ignored`); the real-install checks of the parsed tables and alias fields against the OAT dump and of the AN-94 third-person chain layer by layer, its converted ranges and its shipped layer rows: `ZHBO_BO2_DIR=… cargo test --release -p zhbo-formats snd -- --ignored` and `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_installs_chains_follow_bo2s_mixer -- --ignored --nocapture`; probe: `zhbo-assets mix <ff> <alias> [inches...]` |
| Hit geometry and locations | BO2's ranking (`FUN_007837a0`: rifle head-first, else nearest; none/gun never hit), box pick and rotation, the `HitBox` line (`hitloc::tests`); the location multipliers' attachment rule (`attachments::tests::an_attachments_unique…`); fake-host head vs leg damage and a round through a torso box's thickness; per-stance poses — a crouched enemy General's head is lower and a shot over it misses, a prone General is hit as the prone pose, Zero Hour infantry stand (`sim::hit_tests`); the pose fallbacks (`hitloc::tests`); the player animation script's first-match pick (`playeranim::tests`) | `cargo test -p zhbo-core hitloc attachments sim::hit_tests`; `cargo test -p zhbo-assets playeranim`; the real-install checks of the posed boxes (the three faction bodies identical, head at head height, waist ~13 in deep; each state's clip is the script's pick and the crouched/prone heads sit lower): `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_soldier_hit_boxes -- --ignored` |
| Ground surfaces (the map grid) | `zhbo_core::terrain`: the `CkMp` chunk parser over synthetic maps (chunk versions 3/4 and 5–8, `getTerrainNameAt`'s cell rule, clamping, NaN, 500 corrupt files), the FFI load and query with the water table on top (`every_machine_reads_the_same_ground_from_the_map_file`); `zhbo_formats::refpack` (the engine's `REF_decode`); every map of the player's install decompresses and parses with every cell naming a `Terrain.ini` type (`every_shipped_map_reads_its_ground_surfaces`). In game the bridge logs the grid's agreement with the terrain visual's lookup at load (`terrain: the grid agrees …`) | `cargo test -p zhbo-core terrain`; `cargo test -p zhbo-formats refpack`; `ZHBO_ZH_DIR=… cargo test --release -p zhbo-core every_shipped_map_reads_its_ground_surfaces -- --ignored` |
| Shellshock | the shock file parser and the install's MP shocks (`zhbo-formats shock`); the table's `Shock` blocks equal to the install (`the_shellshocks_convert_and_the_stand_ins_match_them`); the look fade, screen layers, queued exit and the script's flashbang/blast rules (`shellshock::tests`); fake-host blasts: a concussion shellshocks a first-person General instead of paralysing him, the thrower's own is shorter, an overhead General keeps Zero Hour's stun, the flashbang depends on where he looks, a frag shakes him (`sim::shock::tests`); the client's capped look (`a_shellshock_slows_the_look`) and the HUD's whiteout, left to the renderer's layer when it takes the frame (`a_flashed_shellshock_whites_the_view_out`); the presentation — the flashed layers' ease, the blurred trail's decay, the view kick's radius, fade and continuity, the ringing's fade in/hold/fade out (`shellshock::tests`), the camera kicked but not the aim and the ringing stopped on leaving first person (`a_shellshock_kicks_the_view_not_the_aim_and_rings`), the looping event's INI (`sfx::tests::events_are_well_formed`); the renderer's shock layer headless — the trail on the backbuffer, MSAA and an offscreen scene, a fresh trail per shock, the grab plus white, refusals (`zhbo-gfx` `tests/shock.rs`; `ZHBO_SHOCK_SHOTS=<dir>` writes the frames) — and its seam from the viewmodel draw (`nativevm::tests::draws_into_a_headless_renderer`) | `cargo test -p zhbo-formats shock`; `cargo test -p zhbo-core shellshock shock`; `cargo test -p zhbo-gfx --test shock`; `cargo test -p zhbo-core --features renderer draws_into_a_headless_renderer`; `ZHBO_BO2_DIR=… cargo test --release -p zhbo-formats the_installs_mp_shocks_parse -- --ignored`; `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_shellshocks_convert_and_the_stand_ins_match_them -- --ignored` |
| Assault Shield | the held/stowed block of rounds, melee and projectiles, blast protection by arc, the deployed shield (placement, refusal, bullet-scaled wear, pick-up by switch and use key, destruction, cleanup), the CaC offer and self-melee; the real-install pose of the collmap boxes | `cargo test -p zhbo-core shield` (`shield.rs`, `sim/shield_tests.rs`, `weapons` and `classmenu` tests); `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_shield_boxes_pose_in_front_and_behind -- --ignored --nocapture` |
| Headless renderer | frame encoding, transient and dynamic buffers, read-back pixels, draw validation, ABI asserts, the native BO2 pass (skinning, sRGB, lighting, view order, refusals, the lens reticle), the shellshock layer (trail, grab, white, refusals) | `cargo test -p zhbo-gfx` (`tests/headless.rs`, `tests/ffi.rs`, `tests/native.rs`, `tests/shock.rs`) |
| Native viewmodel (Track R) | blob formats round-trip and refuse damage (`zhbo-native`); the core's request/answer loading, composition, posing and the ADS pose layer without a GPU — the 0x55/0x56 `tag_torso` overlay over the action clip, its fraction scrub and clip switch, and the clip-less eye-frame slide fallback; end to end into a headless renderer | `cargo test -p zhbo-native`; `cargo test -p zhbo-core nativevm` and `cargo test -p zhbo-core --features renderer nativevm`; the real-install conversion test checks every converted viewmodel's blobs (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets converts_from_real_install -- --ignored`); the ADS pose against BO2's measured per-family table, the twin (`ads_down` frame 0 == `ads_up` frame N) and the launchers' rotation (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_ads_clip_reproduces_bo2s_authored_pose -- --ignored`); the hold metric (`nativevm::hold`: the drawn gun's parts in the eye frame, and whether one encloses the eye — its box holds the eye and at least five of the six axis rays from the eye cross its triangles; unit-tested on box fixtures and an XPR-shaped hip layer by `cargo test -p zhbo-core hold`) over every weapon and every attachment it offers, posed by the game's own native frame — at the hip no part may enclose the eye, and at every probed pose (held slot at hip and aimed, then each action clip the weapon has — fire, reload, raise, drop, sprint, melee — at seven times into it; ~41,600 poses) no vertex beyond `hold::FAR_RADIUS` (72″) may be drawn visibly: inside the 16:9 frustum and spanning a pixel of a 1080-line screen. BO2's authored magazine parking (45–350″ out of view) and the crossbow's bolt flown in from 25,940″ (0.82 px at most) pass by design (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets every_viewmodel_is_held_in_front_of_the_eye -- --ignored --nocapture`; `ZHBO_HOLD_TSV=<file>` writes the per-pose table, `ZHBO_HOLD_ONLY=as50_mp,…` narrows it); the equipment's authored sub-bone parking and the `stored local + track` playback rule (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets equipment_subbone_tracks_are_bo2s_authored_parking -- --ignored`); the base parts — the snipers' scope is loaded at its authored offset, the ACOG/`is` replace it, the M32's hip sight loads once (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_sniper_scope_is_a_base_part the_acog_replaces_the_base_scope the_m32_sight_loads_once the_pistol_dualclip_suppresses_nothing -- --ignored`); the IR combination moves the base scope and adds the dual-band mount (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_ir_combo_moves_the_base_scope_and_adds_the_mount -- --ignored`); the combined uniques keep their `attachmentUniques` slot and `childLink`/`siblingLink`, and a kind resolves to its child BO2's way (`FUN_008eb690`; `cargo test -p zhbo-formats a_kind_resolves_to_the_first_child`, `ZHBO_BO2_DIR=… cargo test --release -p zhbo-formats reads_the_combined_children_and_their_links -- --ignored`); the AS50 base unique parser test (synthetic zone: `cargo test -q --release -p zhbo-formats the_base_unique_carries_the_overlay_material -- --nocapture`); the converter's synthesized combos' overlays (`cargo test -q -p zhbo-assets weapons::tests::the_synthesized_combos_overlay_follows_the_mounted_optic`; `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_installs_synthesized_combos_overlay_follows_the_mounted_optic -- --ignored`) |
| Native third person (Track R7) | the body and world-weapon manifests round-trip and a set's clip falls back to its own nearer state through the rifleman's same state (`zhbo-native`); the clip resolver's state scan (`zhbo-assets`); the core's third-person composition, state selection (stance, gait, sights and the held weapon's animation set) and world placement without a GPU; each General keeps his own rig (`each_general_keeps_his_own_rig`); the weapon mode is the default (`config` tests); the W3D body's Draw module plays each animation set under its model condition before the rifleman's (`each_animation_set_has_its_own_states_under_its_condition`) | `cargo test -p zhbo-native -p zhbo-assets -p zhbo-core nativebody`; every weapon held in hand plays, in every movement state, BO2's `mp/playeranim.script` pick for its `weapClass`/`playerAnimType` through its def's set, the body's clip ids and their fallback, and every combination keeps its base's set (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets every_weapon_takes_the_body_animations_bo2_gives_it -- --ignored --nocapture` prints the sweep); the real-install conversion test additionally checks every faction body (72 inches, weapon tag, all states' clips, textures) composes with every world weapon, and every set has its W3D run state and clips (`ZHBO_BO2_DIR=… cargo test -p zhbo-assets converts_from_real_install -- --ignored`); a set's poses render with `zhbo-assets pose <out.png> <frame> common_mp.ff:<clip> faction_seals_mp.ff:c_usa_mp_seal6_assault_fb common_mp.ff:<world model>@tag_weapon_right` |
| Asset conformance | BO2/ConquerOps zones load with 0 bytes left over; weapon table round-trips; shell ejection matches BO2 — the defs whose `viewShellEjectEffect` binds in the asset pool (`patch_mp` over `common_mp`'s reference stubs), their FXList oriented at `tag_brass` on BO2's axes, and every combination carrying its base's; others' brass and tracers (`shotfx`: the AN-94's placed `fx_rifle`, `assaultrifle` every 3rd bullet, the scoped LMG combos' `lmg_scope`; the sim's cadence in the fake-host `others_see_brass_every_shot_and_a_tracer_every_nth_bullet`; the viewer's friendly/enemy choice in the client's `each_viewer_sees_the_enemy_tracer_of_shooters_not_on_his_side`, the own gun's in `the_own_first_person_tracer_is_drawn_from_the_viewmodel_on_the_same_countdown` and `the_own_tracers_follow_the_sims_countdown_deterministically`, and install-gated `the_enemy_tracers_differ_only_in_material` in zhbo-formats) | `cargo test -p zhbo-assets`; install-gated: `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets every_mp_weapon_converts_and_plays_like_bo2 converts_from_real_install -- --ignored` |
| Weapon asset pool | BO2's zone priority (`patch_mp` > `common_patch_mp` > `common_mp`, stubs never override, patch-only names appended) over synthetic zones (`weapon::tests::the_pool_keeps_the_highest_priority_zones_asset`); install-gated: every patched weapon converts from `common_patch_mp`'s definition and a patched magazine empties at its patched size in the arsenal sim, every patched attachment is the patch's, and the Peacekeeper's viewmodel and clips convert from the patch zone; a locked item is never held (`classrepair::tests::a_locked_item_is_not_held`), and `converts_from_real_install` locks exactly the Peacekeeper with Revolution's hint; the CaC screenshot harness's `cac-editor-weapons-locked` shot | `cargo test -p zhbo-formats the_pool`; `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_balance_patches_convert_and_play -- --ignored` |
| BO2 conformance | Every fact of every weapon+attachment combination (and the create-a-class lists) the mod ships against BO2's own rules over the player's install: one CSV schema, `expected.csv` (BO2) vs `actual.csv` (the mod), clustered diffs. Fails while any gap exists — no allowlist | unit: `cargo test -p zhbo-conformance` (schema, CSV, differ, BO2 rules on synthetic zones), `cargo test -p zhbo-formats havok_lua big`; the gate: `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets conformance -- --ignored` (~110 s; files in `ZHBO_CONFORMANCE_OUT`, default the temp dir; it fails while gaps remain, so run it apart from `-p zhbo-formats -- --ignored` — cargo stops at the first failing crate); the files for analysis: `cargo run --release -p zhbo-assets -- conformance <dir outside the repo>` (see "BO2 conformance" below) |
| In-game scripted test | full loop: take control, move, fight, throw both offhands, toggle the grenade launcher, field computer, RTS hand-back. The script seeds a v2 profile with one custom class in the active set — the AN-94 with its under-barrel launcher, plus the frag, concussion, pistol and knife — because BO2's premades carry no attachments and the alt-mode step needs one; the autopilot picks that custom from the spawn screen by its computed row key (the match list is the set's custom slots, then the defaults, `changeclass.lua`); its rifle burst sweeps the aim ~6° down and back so it crosses the rebels' BO2 hit boxes whatever the terrain | `scripts/autotest.ps1` (Windows) / `scripts/autotest-mac.sh` (macOS; seeds `ConquerOps.profile` with the GL custom before launch — the standard autopilot picks the first profile custom when there is one, else the first default class) |
| Shot impacts in game | the default autopilot run greps `player 0 impacts:` (flesh on the rebels, a terrain surface on the misses) and the bridge's `impact fx: N played, 0 missing` line — the latter proves the name resolved and `FXList::doFXPos` ran | `scripts/autotest.ps1` / `scripts/autotest-mac.sh` (default leg) |
| Scope-overlay test | the AS50 in hand, hold ADS past the 0.40 s transition; once the reticle zoom is up the native draw logs "the viewmodel is not drawn under the scope overlay" (BO2 skips the whole viewmodel) and the HUD draws the 2D overlay | `scripts/autotest-mac.sh --scope` |
| W3D ADS-layer test | the W3D fallback (`ZHBO_NATIVE_VIEWMODEL=0`) composes the authored 0x55/0x56 clip over the action clip (`HAnimComboClass` pivot maps) and the slide is skipped; the bridge logs "viewmodel: ADS layer" + "viewmodel: ADS slide skipped" | `scripts/autotest-mac.sh --w3d-ads` (Windows: `scripts/autotest.ps1 -Mode w3d-ads`) |
| Spectator (unit) | a spectating client (`ClientIn.spectating`) never sends input, a class pick or a selection, never captures the mouse; follows the first first-person General at the sim's input angles (smoothed across the logic frame, never ahead of the sim); BO2's switch bar (attack previous, aim next, cut at once) and the view toggle (the field-computer key / middle button: overhead and back); the followed General's death cuts to the next or flies out; a player defeated as his General falls sees his own death, then follows; the engine's observed seat is adopted; the HUD is the followed General's, reduced, with BO2's dead-spectate panel | `cargo test -p zhbo-core spectat` (`client/spectate_tests.rs`, `hud/spectate.rs`, `hud/tests.rs`) |
| Spectating a replay (in game, macOS) | records a fresh skirmish (`ZHBO_AUTOTEST_SKIRMISH` starts it from the main menu; a loaded save records no replay) with the scripted first-person run, then plays it back with `-loadreplay`: the General enters first person again from the recorded messages, the viewer logs `spectator: following seat …` and, once a second, its view yaw beside the sim's recorded aim (within 0.1 rad, the aim moving), and sends nothing. Zero Hour's Last Replay is kept and put back | `scripts/autotest-mac.sh --replay` |
| Real input test | OS keyboard/mouse path the autopilot does not cover: on macOS the manual scene (`ZHBO_AUTOPILOT=manual`) selects the General, the script taps T (Take Control's hotkey), Escapes the class screen (which declines the spawn: the classless General goes back overhead, and Escape must not also open Zero Hour's game menu), taps T again once he is selected back overhead (`selected General <n> back overhead`), picks the first default class with 6, holds W and turns the mouse; checks `keys held: [W]`, a moved and a turned pose | `scripts/inputtest.ps1` / `autotest-mac.sh --input` |
| Overhead General | his class guns swapped in (`armed slot 0 with ZHBO_GeneralWeapon_`), rounds counted, the command bar's orders given the way the buttons give them — the lethal at a rebel (thrown as first person throws), a reload, the pistol, the knife at a rebel spawned close (`meleed`) — then first person keeps the pistol; the secondary in hand raises its own body animation set (`third-person animation set <k> (RIDER<k>)`, BO2's `mp/playeranim.script` stance); the overhead General's weapon is drawn natively by default (`native body: General <id> weapon <vm> camo <camo> composed (ZW<vm><faction>)`; `ZHBO_NATIVE_BODY=0` opts out). `--rts-dry` runs it with no spare rifle, then no spare pistol, rounds: the dry gun hands over | `autotest-mac.sh --rts` / `--rts-dry` (`ZHBO_AUTOPILOT=rts`; macOS only, `autotest.ps1` has no RTS mode) |
| Create-a-class run | the in-match screen is pick-only and shows the profile's ACTIVE class set: the script seeds a v2 profile with one custom (the round-2 playtest's "Custom 2": an MTAR with the Target Finder — an attachment combination, whose viewmodel number has three or more digits — a crossbow with the IR scope and all three camos, whose readable text is two bytes over the 64-word pick payload; deliberately unlike any premade kit) in set 1, picks it from the spawn screen by its row key (the custom leads the match list, `changeclass.lua`) and the sim logs `assumed the Custom 2 class`, with no profile write — after a first session that Escapes the spawn screen without picking, which must hand the General back overhead (`returned General <n> to RTS control` before any `assumed the` or `opened the field computer` line) and never show the knife in a pose line (an unclassed one reads `awaiting a class`); the once-a-second pose line then names the MTAR, proving the *kit* landed; it then leaves first person, takes control again, forces the screen open and Escapes it, and first-person play continues | `ZHBO_AUTOPILOT=classpick` with either autotest script (the macOS script seeds `ConquerOps.profile` first, then checks `assumed the Custom 2 class`, `MTAR <n>\|` in a pose line, `native viewmodel: ZV<3+ digits><U/C/G> composed` with no `native viewmodel:` line naming a `.zb*` file `not in the archive` or a `faction 0x..`, no `create-a-class: saved` line, a third `entered first-person control` with the pose changing after it, and `taking control again`; then the bridge kills the General and stands a new one in his place (`autopilot: respawned General`), whose session must have no `awaiting a class` pose line, an `MTAR <n>\|` one and a changing pose: the chosen class came back with no screen) |
| Custom-class pick payload | every class the editor can build fits the 64-word choose-class payload and lands: `classmenu::kit_coverage` visits every offered item with every legal attachment set (both slots, Overkill, the Gunfighter caps), every offhand and count, perk, camo, both optics' reticles and the colour, each with the largest value of every other field (the codec's fields are sized independently, so that bounds every legal kit); each round-trips through `classwire`, is sent by the command bar and applied by the fake-host sim; the codec's constructive bound, literals for unknown values, malformed payloads, the old text payload; the playtest's class (an attachment on each gun, three camos) picked from the class screen; a profile past the 4 KiB save payload still written | `cargo test -p zhbo-core classwire every_legal_custom the_playtest_second a_profile_larger_than_the_payload`; over the install (12,524 classes with both reticles, largest 20 words): `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets every_legal_class_fits_the_pick_payload -- --ignored --nocapture` |
| Shell create-a-class | the lobby's class screen (the menu ABI): browse, edit, save, delete, copy, close — in the shell, no match | manual only: the in-game autotest does not cover the shell (see below) |
| Class-screen screenshots | the shell list, the in-match pick list and the Pick-10 editor rendered through `classui::draw_screen` | `cargo run -p zhbo-assets -- screenshot` (uses the locally built `ConquerOps_BO2.big`; PNGs in `target/cac-shots/`) |
| Viewmodel sweep | every player weapon and every attachment it offers drawn through the game's native first-person path (composition, `nativevm::place`, the native BO2 pass), hip and aimed side by side, for a human review of the catalog | `cargo run --release -p zhbo-core --example render_viewmodels --features renderer -- <outdir>` |
| Third-person sweep | every converted weapon's world manifest (`ZW<vm>.zbw`, the overhead General's weapon) drawn through the game's native third-person path (`zhbo_native_body` in weapon mode, the native BO2 pass) from above and behind, one PNG each plus a contact sheet; a surface without its colour map shows the renderer's white stand-in | `cargo run --release -p zhbo-core --example render_world_weapons --features renderer -- <outdir> --archive <ConquerOps_BO2.big>` (`--weapon <id>`, `--all` for the combinations) |
| Weapon model colour maps | every model a BO2 MP weapon names (view, world, projectile, base parts, every attachment's view/world/additional) converts with a colour map on every kept surface: a patch zone's reference materials (`,name`) bind to the pool's definition (`viewmodels::resolve_materials`), stub images resolve by name, the colour is the `colorMap` sampler (`textures::colour_image`) | `cargo test -p zhbo-assets the_colour_image_is_the_color_map_sampler`; `cargo test -p zhbo-formats nested_materials`; `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets every_weapon_model_surface_has_its_colour_map -- --ignored` |
| Combo conversion coverage | the converter's built rows (every legal combination as the weapon a class gets — a pistol with Dual Wield is its `_dw_mp` weapon — each one's alt mode, and the two melee weapons every class has: the held knife and the class melee weapon `knife_mp`) equal BO2's own Create-a-Class rows and covers every statstable-backed base weapon; the archive carries a native viewmodel manifest per legal combination (the base set only when `ZHBO_CONVERT_ALL_COMBOS=0`), and only the base set carries the composed first-person W3D meshes and the third-person body's world meshes (the legal combinations fall back to the base mesh) | the real-install tests `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets conformance_the_enumerator_matches_the_expected_rows the_legal_combo_set_covers_every_base_weapon -- --ignored`; the build-time measurement: time `env ZHBO_CONVERT_ALL_COMBOS=0 cargo run --release -p zhbo-assets -- build-archive <base.big>` against `env ZHBO_BO2_DIR=… cargo run --release -p zhbo-assets -- build-archive <full.big>` and compare the two `.big` sizes (the full build must stay under 2^32 bytes; both the first-person split and the third-person base set keep it there) |
| Third-person world meshes | every legal combination's `.zbw` carries BO2's third-person parts (base `attachWorldModel` slots, then the resolved attachments, default tag `tag_weapon`); every base weapon's world meshes on the soldier are numbered `W<vm>00..W<vm><n-1>` with `n` its `WorldMeshes` (the bridge shows `k < n`), every vertex rides the hand and every index is valid; `number` closes the gap `finish` leaves after the dropped clan-tag/emblem surfaces | `cargo test -p zhbo-assets world_meshes_are_numbered`; `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets every_world_weapon_shows_all_its_meshes -- --ignored`; the world parts (`the_world_parts_are_the_base_slots_then_the_attachments`, `the_resolved_unique_walks_the_childs_chain`, the conformance fixture `world_parts_default_to_tag_weapon`), a combination's W3D fallback (`a_combination_shows_its_base_weapons_world_meshes`) the body-mode mount (`a_tagless_world_part_rides_the_weapon_in_body_mode`) and the third-person camo (`each_world_part_wears_its_own_camo_asset`, `the_class_camo_follows_the_primary_into_third_person`, the `.zbw` round trip) |
| Camo swap | the parser's synthetic/real camo reads; the `.zcm` round-trip and refusals; the converter's 47 blobs (every asset a def or unique points at, the seven optic sets included); each manifest part's camo asset (`each_part_carries_its_own_camo_asset`) and the per-part paint (`each_part_wears_its_own_camo_asset`); the class camo's transport and the draw-swap; the renderer's second texture and tint | `cargo test -p zhbo-formats`; `cargo test -p zhbo-native`; `cargo test -p zhbo-core`; `ZHBO_BO2_DIR=… cargo test -q --release -p zhbo-assets the_camo_assets_convert_with_their_art -- --ignored`; the sweep's `--camo` (below) |

## The BO2 reference (ground truth)

Parity work is checked against the existing reference dumps only (`~/Documents/BO2-reference/`,
the `bo2-reference` skill): the decompiled UI Lua (`lua-dump/`) and GSC (`gsc-dump/`), the
menuDefs, the create-a-class captures (`cac/`), the asset catalog, and read-only static Ghidra on
`t6mp`. No new in-game captures are taken (decision 2026-10-21), so HUD and viewmodel feel are
verified against the code that produces them (Lua layout math, weapon-def numbers, engine
functions), not against video. Ghidra answers live in `ghidra/answers/G<n>.md` (index in its
`README.md`; `dvars-t6mp.tsv` lists the engine's dvar defaults): G1 perk dvars and specialty tests,
G2 player movement, G3 weapon switch/raise/drop/ADS timing, G4 viewmodel placement, G5 kick and
recoil, G6 crosshair and hit markers, G7 damage feedback, G8 LUI easing, G9 the LUI root and safe
area, G14–G17 below. A value only the engine knows lands as a cited default in the tuning pack
(`tuning.rs` + `tuning.ini`); every parity decision cites its dump `file:line`, capture or Ghidra
function in the commit and MODLOG.md.

## BO2 conformance

Parity is proved or disproved by data, not by eye. `zhbo-conformance` holds one schema per table
— `weapons` (a row per BO2 variant and attachment combination: every combination Create-a-Class
allows, Gunfighter's third/second attachments included, each one's alt mode, the knives every class
gets) and `cac` (a row per weapon, attachment, perk, wildcard, camo/reticle option, class cap,
weapon category and dual-wield outcome) — and both sides fill exactly those columns:

* **expected** (`zhbo_conformance::bo2`): BO2's rules over the player's own install, transcribed
  from the engine — `~/Documents/BO2-reference/ghidra/answers/G14.md` (how a combination resolves:
  canonical order, single vs combined uniques, parts, per-field stacking, sounds, effects, hide
  tags, the anim table), `G15.md` (which combinations a class can carry), `G16.md` (the weapon a
  class gets, ammo, per-part camo), `G17.md` (CaC list order, attachment tokens, costs) — and the
  compiled UI Lua (`zhbo_formats::havok_lua`: the `CoD.CACUtility` caps, the weapon categories).
  It reads raw structs through the layouts and depends on `zhbo-formats` only.
* **actual** (`zhbo_assets::conformance`): what ships — the launcher's archive built in memory
  (`hud::build_archive`), its weapon table installed as the game installs it, every class the mod's
  editor accepts (`classmenu::offer`) resolved by the runtime catalog, the viewmodel/world-weapon
  manifests the def's `viewmodel` names (an unconverted combination draws its base's art, as the
  game does), the model blobs, the converter's sound events and FXLists. A fact the mod does not
  carry stays empty — it fails, never synthesized.

Units are BO2's (ms, inches, degrees; rotations as canonical quaternions; asset names lowercased, because BO2 resolves them case-insensitively); a float column's epsilon
is the only tolerance and is written in the schema (`<table>-schema.csv`). Every difference is a
gap; there are no informational columns and no allowlist. The output directory must be outside the
repository (the run refuses one inside it): per table `<t>-expected.csv`, `<t>-actual.csv`,
`<t>-diff.csv` (one line per differing cell: key, area, column, both values, the gap signature),
`<t>-schema.csv` and `<t>-report.md` (clusters by area and column, grouped by signature — "mod side
empty", a constant ratio or offset, items a set lacks or adds). The CSVs load into any SQL/CSV tool
(`duckdb`, `sqlite3 .import`) for queries such as "every row whose `view_part_models` lacks the
optic". The first run (2026-10-25) is the backlog: 211 gap clusters (see MODLOG); a parity fix is
done when its clusters leave the report.

## The shell create-a-class screen (manual)

The lobby's Create a Class screen is a shell window driven through the menu ABI
(`docs/CONTRACT.md`); no scripted run covers it (the autotest loads straight into a match), so
verify it by hand after touching the menu ABI, the shell window, or the shared class-screen code:

1. Launch the game (`conquerops`), click through: **Solo Play → Skirmish**; the **Create a Class**
   button sits at the top-right of the lobby panel.
2. Click it: the BO2 class screen opens *in the shell* (menu background art, the class list, the
   kit panel). The lobby beneath is inert (its buttons and its Escape shortcut do not react).
3. Browse with the mouse (hover highlights, the wheel scrolls) and the arrows: the list is the
   active set's five custom slots (`cacchooseclass.lua`) — **Enter/click on a filled row** opens
   the editor on that class; on an untouched **`Custom n`** row opens a fresh draft. **Escape** in
   the editor returns to the list and keeps the edits (BO2 has no save step); the editor has no
   name field or SAVE row. **`A`/`D`** page the class sets (BO2's shoulder buttons), as do
   clicks on the `‹`/`›` arrows and on a pip; **Left/Right do nothing** on the list.
   Navigation parity (BO2's LUI links, `classmenu/nav.rs`), check by hand:
   - Editor grid: Up/Down walk a column and wrap through the bonus-card row (left column via
     cards 1-2, right column via card 3); Left/Right jump
     between PRIMARY↔PERK 1/2, PRIMARY ATTS↔PERK 3, SECONDARY↔LETHAL, SECONDARY ATTS↔TACTICAL; going
     PERK 2 → Left → Right returns to PERK 2; the three cards are separate highlights walked with
     Left/Right (no wrap); the wheel moves like Up/Down; hovering a box highlights it.
   - Weapon picker: Left/Right walk the guns and stop at the category's ends; Up/Down and the
     wheel change category (wrapping) and come back to the gun last shown there; hovering a tile
     does nothing, clicking a tile makes it the big card, clicking the big card equips it.
   - Option grids (attachments, perks, grenades, wildcards, camo/reticle): one wheel notch moves
     one row; Left/Right wrap within a row.
   - The S/O dialogs: hover highlights an entry, the wheel walks them.
   - Camo (`C` on PRIMARY): every tile shows its thumbnail; the title reads `PERSONALIZE <GUN>`;
     the tabs (CAMO, DLC, DLC2 when the install has a DLC2 camo) page with `A`/`D` (wrapping) and
     by clicking a tab; the panel under the grid shows the focused camo's name, the gun's picture
     and the `Equipped` box. Equip a camo: the editor's PRIMARY button and the class list's preview
     show its swatch in the corner; the reticle page has the same RETICLES/DLC tabs.
   - Knife (`TAB` in the editor): the same page over the knife — `PERSONALIZE KNIFE`, the
     CAMO/DLC(/DLC2) tabs with the knife's own converted camos, the knife's picture and the
     `Equipped` box; a pick sets the class's knife camo, not the gun's.
   - Secondary (`C` on SECONDARY, only while it holds a gun): the same page over the sidearm —
     `PERSONALIZE <PISTOL>`, the sidearm's own converted camos, its picture; a pick sets the
     class's secondary camo (the SECONDARY button and the preview's secondary block show its
     swatch), not the primary's. In a match, switch to the sidearm: it wears that camo.
   - Pick a different gun in a personalised slot: that slot's attachments are removed and its camo
     (and the primary's reticle and reticle colour) go back to stock, as BO2's `SetClassItem`
     does, even when the new gun offers the same attachments; re-picking the same gun keeps them.
4. Edit something and press **Escape**: `ConquerOps.profile` (in the game's user-data directory) is
   rewritten with the new class, and the list shows it highlighted. Opening an untouched slot and
   backing out writes nothing (the slot stays `Custom n`). Rename a class with **O** → RENAME (the
   only rename): type, Enter, and the list and profile show the new name.
5. On a custom row: **C** copies it (`NAME COPY` appears), **Backspace** deletes it — both rewrite
   the profile file.
6. **Escape** on the list closes back to the skirmish lobby (and the lobby's Escape works again).
   The LAN lobby (**Network → LAN → Create Game**) and the online game setup screen carry the same
   button in the same corner (`create-a-class: lobby button added to LanGameOptionsMenu.wnd` /
   `GameSpyGameOptionsMenu.wnd` in `ConquerOps.log`), opening the same screen.
7. Start a match: the saved class appears on the in-game pick screen (the set's five custom
   slots, then the default classes), which is **pick-only** (no editor, no delete/copy, untouched
   slots are inert — BO2 creates classes only in the lobby); the
   spawn's screen is up in first person while unclassed and cannot be played around: Escape on it
   declines the spawn and the General goes back overhead, still unclassed (BO2 never spawns a
   classless player) — the next take-control puts it up again; the command-bar button forces it
   for a classed General, whose Escape keeps playing. After a death the next General comes in with
   the class and no screen; a pick after his first combat (or past the grace period) shows "Your
   class will change the next time you spawn" and keeps the kit. `scripts/autotest-mac.sh --classpick` covers that flow.

Unit tests drive the same screen headlessly through `MenuState` (`crates/zhbo-core/src/menu.rs`:
browse → edit → save → file text out; delete; copy; close; the set pager's keys and clicks), the
navigation through `classmenu/tests.rs` (every BO2 grid link, the back-links) and
`classui/tests.rs` (carousel axes and mouse, wheel per screen, card focus), and the FFI layer
(`crates/zhbo-core/src/ffi.rs`: open/frame/close, and `zhbo_reset` dropping the screen). The
command-bar class buttons are covered headlessly the same way: `client::tests` drives a
`ZhboClientIn.class_button` one-shot through `Client::frame` into `Sim::choose_class` and asserts
the kit lands (preset and profile custom, and that an empty slot sends nothing), and
`ffi::tests::a_command_bar_class_button_reaches_the_client_out_through_the_abi` covers the ABI hop.

## The HUD contact sheet (atlas forensics)

`cargo run -p zhbo-assets -- hud-sheet [out-dir] [--archive <ConquerOps_BO2.big>] [--cols N]` renders
EVERY MappedImage in the converted archive to one labelled PNG grid (`hud-sheet.png` plus
`hud-sheet.txt` with rects/opaque-% and the raw `atlas-page-N.png` pages). Use it when an icon looks
stitched or mis-framed: a tile showing two images means a bad rect or a page mix-up, and the sheet
says which. It reads the already-converted archive (no BO2 install needed, never launches the game).

## Headless class-screen screenshots

`cargo run -p zhbo-assets -- screenshot [out-dir] [--archive <ConquerOps_BO2.big>] [--size WxH]`
renders the class screens through the same drawing code the game runs (`classui::draw_screen`) and
writes these shots: `cac-browse.png` (the lobby list: the active set's five custom slots, the set
header and the S/O footer), `cac-set-pager.png` (a browsed mid-row set), `cac-pick.png` (the
in-match list: the slots, then the default classes), `cac-editor.png` (a populated Pick-10 draft
with the primary option list open), `cac-editor-empty.png` (BO2's pristine empty class),
`cac-editor-weapons.png` (the category column and stat bars), `cac-editor-attachments.png` (the
attachment strip), the personalize pages `cac-personalize.png` (camo, CAMO tab),
`cac-personalize-dlc.png` (its DLC tab), `cac-personalize-knife.png` (the knife page),
`cac-editor-secondary.png` (the grid with SECONDARY focused: its camo swatch and the `C` prompt),
`cac-personalize-secondary.png` (the sidearm's own camo page),
`cac-reticles.png` (and `-dlc`/`-dlc2`) and one page per other custom-reticle optic
(`cac-reticles-holo`/`-acog`/`-dualoptic`/`-dualoptic-dlc`, the sidearm's `cac-reticles-secondary`),
`cac-editor-reticle-prompt.png` (the attachment box's `V Select Reticle`), `cac-reticle-color.png`, `cac-editor-lethal.png` (the lethal list: grenades and placed equipment), `cac-editor-overkill.png` (the secondary list under Overkill: primary groups only), `cac-editor-danger-close.png`/`cac-editor-danger-close-x2.png` (the lethal button's Danger Close box, empty then filled), `cac-editor-wildcards.png` (the wildcard grid: the table's bonus cards and the
highlighted card's name and description), `cac-editor-perks.png` (the Perk 1 grid: BO2's perk names and the highlighted perk's name and description) and `cac-dialog-set.png`/`cac-dialog-class.png` (the S/O dialogs), and the HUD's scope overlays
(`hud::scope_overlay`) as `hud-<material>-<w>x<h>.png` for every converted overlay at 1280x720,
1280x800, 1024x768 and 1720x720 (the scope must stay round at every aspect). Default size
is 1280x720 (BO2's 720p; the captures are 3024x1890). The default output is `target/cac-shots/`,
which is inside the ignored build directory.

It reads the player's already-converted `ConquerOps_BO2.big` (the launcher's normal output; found in
the dist folder, or pass `--archive`/set `ZHBO_BO2_ARCHIVE`) and the `weapons.ini` inside it, so it
needs no Black Ops II install and never launches the game. Sprites resolve through the archive's
MappedImage INI and TGA pages; art the archive lacks draws its fallback prims exactly as
`hud::render` does, and the harness prints those sprite names. Use it to iterate on layout and art
without starting a match; the unit tests cover the same views with fallbacks only (no archive).

## The headless viewmodel sweep

`cargo run --release -p zhbo-core --example render_viewmodels --features renderer -- <outdir>`
draws every player weapon of the installed table — the bare weapon first, then the weapon with each
attachment its `Allowed` list offers, one at a time — through the game's own native first-person
path: `zhbo_native_viewmodel` and the file protocol `ZhboViewmodel.cpp` drives, the core's
composition and posing, `nativevm::place` (`ads = 0` hip, `ads = 1` aimed — the 0x55 ADS layer's
last key, or the clip-less slide for a weapon without one) and `nativevm::gpu`'s
submit into a headless `zhbo-gfx` renderer (the real native BO2 pass). Each image is 1280x360: the
hip pose on the left, the aimed pose on the right, 640x360 each, on a flat 0.72 grey with the
game's field of view and a fixed eye-frame camera. `manifest.tsv` records where each render's art
came from, its part count and the viewmodel's log lines.

Base weapons come from the player's converted `ConquerOps_BO2.big` (the launcher's dist folder, or
`--archive`/`ZHBO_BO2_ARCHIVE`); a combination the archive does not hold (anything but the
loadout's own kit) is converted on demand from the Black Ops II install (`ZHBO_BO2_DIR`) by
`zhbo_assets::viewmodel_sweep`, in memory for that render. `--weapon <id>` (repeatable) and
`--no-attachments` narrow a run while iterating. The output is a review tool's images, not source:
keep the folder outside the repository.

**The poses and the notes.** A gun's pair samples its `IDLE` clip at frame 0 with the ADS layer's hip key over it (the layer is on at every fraction, BO2's rule — the XPR-50's hip rig root lives only there); its aimed half is
BO2's own ADS animation — the 0x55/0x56 `tag_torso` layer over that idle, scrubbed to the up
clip's last key (`nativevm::Loaded::pose`, the state the game draws at `ads = 1`), including the
authored eye relief and the launchers' rotation. An off-hand item's game path plays `HOLD_FIRE`
while it is held (`viewmodel.rs:181-190`), so the sweep samples that
slot at frame 0 too; when the manifest lacks the 0x5 clip (or the def's `anims` bit is clear) the
sweep stays on `IDLE` and the note says so — the native path silently falls back
(`nativevm/mod.rs:304-310`). The melee knife stays on `IDLE`: its game slot is `MELEE`, a
pre-existing sweep inaccuracy. An off-hand's aimed half is a **harness artifact**, not a game
pose: equipment has no ADS state (an item that carries the authored empty torso-only clip — the
trophy, say — now stands at its hip pose there, and one without any ADS clip keeps the fallback
lift), and `place::aligned` lifts the whole rig by the item's palm-as-aim-marker, so its
`manifest.tsv` note starts with the stable marker `ads half: harness artifact`.

**Reviewing the aimed halves.** Judge the ADS half by BO2's authored line, never by the frame
centre: `the_ads_clip_reproduces_bo2s_authored_pose` pins each family's measured value, and the
authored residuals must *show* — the KSG's `tag_sights` 2.2″ below the axis, the AS50's 1.3″ (its
clip is authored to a different reference), the launchers' 1.7–2.9″ marker offset, the dual-wield
pair's zero move (its hip and aimed halves are the same picture). The gun pose adds nothing at
full ADS: `the_aimed_sight_lies_on_the_shot_for_every_gun` (install-gated, `zhbo-assets`) projects
every gun's and optic's aim marker at `ads = 1` through the game's projection with the settled
`gunpos` pose, and asserts the sight's NDC minus the shot's NDC is the authored clip's NDC alone
(within 0.002); `--nocapture` prints the per-weapon table. Eye relief and the launchers'
rotation do the rest: the rifles/SMGs come closer to the eye, the SMAW/FHJ-18 tubes straighten.

**Verifying the equipment pose.** The hip half's pixels are descriptive, not a pass/fail: the
held clips author the item at the waist, so its palm is outside the game's first-person cone and
the hip crop is expected to be blank for most items. The pass/fail is the ignored, install-gated
test `the_equipment_palm_is_outside_the_hip_frame` in `zhbo-assets`, which composes each of the
13 off-hand manifests and asserts the mount in model space: the item's part hangs from
`tag_knife_attach`, its root coincides with the palm, the palm is within 8″ of `j_wrist_le`, and
the palm is outside the hip view volume (`forward <= 0` or `ndc_y < −1`, the NDC from the
harness's own projection). It prints one row per item (id, slot, clip name, forward/left/up,
`ndc_y`). Run it with
`ZHBO_BO2_DIR=… cargo test -q --release -p zhbo-assets the_equipment_palm_is_outside_the_hip_frame -- --ignored --nocapture`;
the mount core is also covered by `the_offhand_items_ride_the_knife_attach`. For the human review
of the held pose, run the sweep with `ZHBO_SWEEP_BACK=72`: the argument is in inches (the code
converts to world units at 6 inches each), so 72 pulls the review camera 72″ back and the whole
arm/item rig is in frame. A smaller value catches only the rig's bottom sliver.

**Verifying the dual-wield left gun.** A dual-wield weapon's viewmodel is two rigs: the base gun
(`t6_wpn_pistol_<x>_view`) on `tag_weapon` and its `_lh` companion (`t6_wpn_pistol_<x>_view_lh`) on
`tag_weapon1`, the viewhands' second weapon mount that the dual-wield left clip animates to the
left gun's authored pose. The native pose samples the right/base clip and overlays the left clip
(its `_left_*` tracks drive the left arm, `tag_weapon1` and the `_lh` gun); without the overlay the
left gun stays at the base clip's bind pose. The left gun is the one the weapon's def names
(`szDualWieldWeaponName`, `fnp45_dw_mp` → `fnp45_lh_mp`; `viewmodels::left_gun`). A class never
carries a pistol with a `dw` attachment: Dual Wield gives the separate `_dw_mp` weapon (G16,
`classmenu::items::given_weapon`), which the fake-host test `a_dual_wield_class_spawns_with_the_dw_weapon`
pins; the same test then fires it through the event stream (the ADS button fires the left gun,
the attack button the right, each from its own magazine). The arsenal's two-gun rules (each hand
fires its own magazine, no ADS, reload refills both, an empty hand reloads alone) are the
`weapons::tests` dual-wield tests; the left gun's `_LEFT` clips are
`viewmodel::tests::the_left_gun_plays_its_own_slots`; the HUD's `dw | clip / stock` counter is
`hud::ammo::tests::a_dual_wield_weapon_shows_both_clips`. The
install-gated test `the_dual_wield_composite_loads_both_guns` in `zhbo-assets` composes each of the
five DW pistols and asserts the composite adds the left gun's bones, vertices, surfaces and part
over the base composite. Run it with
`ZHBO_BO2_DIR=… cargo test -q --release -p zhbo-assets the_dual_wield_composite_loads_both_guns -- --ignored --nocapture`.
For a visual check, run the headless sweep with `--weapon fnp45_dw_mp` and inspect the hip pose's
left-palm region (x ≈ 180–360 px, y ≈ 220–340 px in the 640×360 slot): the left gun's geometry
should sit there, not at the torso center or floating near the base gun. The autotest's
`--classpick` leg keeps its MTAR seed (`tar21_mp+rangefinder`) — changing the primary would break the `MTAR <n>\|` kit
assertion — so to exercise the DW path in-game, edit the seeded profile's `secondary=` to a DW
pistol (e.g., `secondary=fnp45_dw_mp`) in a local capture run and confirm the left gun renders in
the left palm.

**The camo sweep.** `--camo <id>` (repeatable) draws the weapon with that class camo set and writes
`<weapon>/camo_<id>.png` (`att_<short>_camo_<id>.png` for an attachment job); the manifest's `camo`
column names the option. `--reticle <id>` (repeatable, instead of the camos) draws the optic's lens
with that class reticle (`reticle_3` is `weaponoptions.csv` row 3) into `<weapon>/reticle_<N>.png` the
same way. Compare it with the stock
`base.png` and the BO2 captures (`~/Documents/BO2-reference/cac/sweep/i-personalize/`). C0's
evidence: the atacs solid is 32×32 with alpha 41–47, the pattern 256×256 with alpha 0–110 and
edge-tiling (edge mean difference 4.2/3.2); `CAMO_GAIN` is 2.0 (1.0 is the fallback if the
comparison rejects it). A style camo's colour is its material's `colorMap` sampler (the layered
toxic's `layer1Map`) and its specular the `SpecularAndGloss` spec-rgb map; `shaderConsts` are
carried but not interpreted (D8). Metals keep their hue in the coloured specular (gold's colour map
is a flat 21/20/13, its spec-rgb gold), which the converter carries as the material's specular
colour map and the native pass uses as the reflectance colour (RENDERER.md; pinned headless by
`a_coloured_spec_map_tints_the_metal`). The real-install test
`the_camo_assets_convert_with_their_art` pins the converter's unresolved list to the 20 provably
unreachable names (DLC-only stubs and diamond's colour-less technique).

## The native-art audit

`cargo run --release -p zhbo-assets -- audit-native <ConquerOps_BO2.big>` loads every converted
weapon of the archive's own table — the base set, every attachment combination create-a-class can
build (the archive holds a `ZV<vm><F>.zbv` for each), alt modes — the way the game does:
`nativevm::load` for each viewmodel and each faction's arms over the bridge's file protocol, and
`nativebody::load` for each world manifest and each faction's body. It lists every one that fails to
compose or composes no surface at hip or at full ADS (the ADS part list), and exits non-zero if any
does. Every composed mesh's textures are checked too: a surface with no colour map (other than the
ui3d reticle and the MMS screen, which draw code-made inputs), a colour, maps, specular or camo
file the archive lacks or that does not decode, or a solid-white colour map is a problem — each
draws the renderer's white stand-in in game (the SMAW's third-person weapon did, overhead). The same
audit runs as the real-install test `native_audit::every_converted_weapon_draws`
(over the converter's output in memory, so it needs only `ZHBO_BO2_DIR`). It checks the art and the
core's loading, not the bridge: the bridge's half is the `ZV<vm><F>` name and the faction letter it
passes (`Zhbo_GetViewmodel`), which the core validates (`nativevm` refuses a non-letter faction
with a log line). The sweep above passes its faction directly, so it cannot catch a bridge naming
bug; the game log's `native viewmodel:` lines can.

## Autopilot and the input path

The autopilot replaces all input inside the Rust core, so `autotest.ps1` never exercises the OS
input path. `ZHBO_AUTOPILOT=manual` sets up the scene without scripted input, and
`scripts/inputtest.ps1` drives real keys and mouse through it.

Scripting real keys: `hold <VK> <ms>` needs a hex VK code (`hold 0x57 2500` for W); on SDL,
`scanmode on` switches `hold`/`press` to scan codes. macOS holds keys by code with
`osascript key code`, or `cliclick` for the mouse.

## Multiplayer in game (two instances on one Mac)

The headless gate (`cargo test -p zhbo-core lockstep`, the Layers table) is the first multiplayer
check; `the_multiplayer_autopilot_fights_in_lockstep` runs the in-game test's own script on it.
The in-engine test is `scripts/mptest-mac.sh` (or `scripts/autotest-mac.sh --mp`): two instances
of the staged release build host and join one LAN game by themselves on Tournament Desert, and
each player's autopilot plays his own General.

How two instances run on one machine (host patch 0022 and the bridge):

* **A second instance, a self-starting LAN game.** A playable run takes the single-instance lock
  (`rts::ClientInstance`). The engine's network auto-start driver (`NetworkAutoStart`,
  `-autoNetworkMode direct -autoNetworkHost <n>` / `-autoNetworkJoin <ip>`, `-autoNetworkMap`) is a
  debug-build feature that also turns on multi-instance; the mod's build compiles it into release
  (`RTS_NETWORK_AUTOSTART`, `host/zhbo/CMakeLists.txt`) and patch 0022 makes it act only with
  `GGC_NETWORK_AUTOSTART` set, so a player's run is unchanged. The host is instance 2 (127.0.0.2),
  the joiner 3 (127.0.0.3).
* **An address per instance, no root.** macOS only answers 127.0.0.1 unless each extra address is
  aliased (`sudo ifconfig lo0 alias 127.0.0.2 up`). With `GGC_LOOPBACK_INSTANCE_PORTS` the
  transport keeps 127.0.0.<id> as the instance's identity and binds 127.0.0.1 at port + 100 × id
  instead (sends to 127.0.0.<id> and broadcasts are mapped to the offset ports, the senders back
  to their identities), so the lobby and the game run their normal code. No alias is needed; with
  aliases in place (or on Linux, which routes all of 127/8) the variable can be left out.
* **Separate files.** Each instance after the first writes `ConquerOps_InstanceNN.log` and reads
  `ConquerOps_InstanceNN.profile` (the engine does the same for its `Options_InstanceNN.ini`).
* **Lockstep-safe scripting.** `ZHBO_AUTOPILOT=mp` spawns nothing: the bridge orders the General
  from the Command Center (`MSG_QUEUE_UNIT_CREATE`), walks him to a meeting point between the bases
  (`MSG_DO_MOVETO`) and takes control; the core's `mp_input` aims at the other first-person General
  and plays the fight; a dead General is ordered again and taken once more. Every step travels as
  a replicated message. `ZHBO_AUTOPILOT_ROLE` (`shooter` for the host, `target` for the joiner) is
  the only difference between the machines and only scripts the local input.

What it checks (both logs): the LAN game started with one content CRC; both players took control;
the host's seeded custom class (AN-94 + launcher + reflex, gold camo) and the joiner's preset
landed; swaps, a lethal, damage, a death, the joiner's next General coming in with his class
(`first-person session 2`, no second pick), the field computer; Zero Hour's own CRC check never
reported a mismatch (the bridge logs `multiplayer: Zero Hour reported a CRC mismatch`); the
`state hash: frame N …` lines (the fingerprint with its seat and world parts; a mismatch prints both machines' parts at the first differing frame) (`ZHBO_STATE_HASH_LOG`) agree on every frame both ran (at least
1,500; only the network game's, after the bridge's `game start: a network game` line — the shell
map's logic logs its own frames from 0 too); the CRC frames' `crc frame F: …` lines (Zero Hour's logic CRC carrying `MARKER:ConquerOps`, host patch 0020) are present and equal on both machines; a round resolved against a rewound General landed (`antilag: General S's round hit General T where he stood at frame F`); and each machine composed the other player's General's native third-person weapon with
the weapon and camo that player holds (`native body: General G weapon V camo C composed` against
the other log's `autopilot mp: General G weapon V camo C in hand`). Results go to `$OUT`
(default `$TMPDIR/ConquerOps-mp`): `host.log`, `join.log`, both stdouts, `hashes-joined.txt`.

Two machines (or the Windows VM and the Mac) need none of the above, only the same build: set
`ZHBO_STATE_HASH_LOG=1` on both and compare the `state hash` lines afterwards. Zero Hour's online
lobby also compares the exe CRC.

* **The replay check runs the hook.** Record a skirmish started from the main menu (a loaded
  save records no replay; `ZHBO_AUTOTEST_SKIRMISH=1` starts one), with real input or the
  autopilot — the autopilot's scene is spawned from the logic step for the first human seat, which
  is the recording's local player in playback too, so its games replay (`autotest-mac.sh
  --replay`'s recording passes) — then, with the same `ZHBO_AUTOPILOT` (it is in the content CRC),
  `generalszh -useCwd -mod <dist>/ -headless -replay <file>` from the Zero Hour folder: exit 0 and
  no `CRC Mismatch in Frame N`; the replay's `crc frame` lines must equal the live game's (a
  headless run takes multi-instance slot 2, so they are in `ConquerOps_Instance02.log`). The CRC carries the sim's state (`MARKER:ConquerOps`, host patch 0020), so this
  checks the mod's determinism too, on one machine.

## macOS test notes

- The fork's `-loadsave` skips the menus, so macOS tests start straight into the save.
- `ConquerOps.log` survives between runs: delete it before launching, or a wait for
  "entered first-person" matches the previous run and input is sent during the loading screen.
- The macOS runner holds keys with `osascript key code` (mouse via `cliclick`); `hold` timing is
  looser than Windows' SendInput, so movement distances differ.
- Some saves end with the General dying before re-entry; the "re-entered" check then only proves
  the line is logged, not that first person came back.

## Windows notes

- Both builds (DX8 fork and wgpu) pass `autotest.ps1` and `inputtest.ps1`; run the DX8 build for
  the baseline and the wgpu build to compare rendering.
