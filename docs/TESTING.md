# Conquer Ops — Testing

How each layer of the mod is verified, and what to run before declaring a change done.

## Layers

| Layer | What it covers | How |
|---|---|---|
| Unit tests | sim, weapons, movement, hitscan, HUD, formats, requirements | `cargo test --workspace` |
| Shot impacts (R8d) | the surface vocabulary and the converted table (classification, default-cell and blood-variant fallbacks); the sim's per-pellet `Impact` event and its absence on a miss; the client record and its cap; the ABI lookup; every converted gun's bucket and cells resolve to a converted FXList | `cargo test -p zhbo-core impacts`; `cargo test --workspace`; the real-install check: `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets converts_from_real_install -- --ignored` (asserts each gun's `ImpactType`, `pellets <= 8`, and that every row names an `FXList` in the effects INI) |
| Headless renderer | frame encoding, transient and dynamic buffers, read-back pixels, draw validation, ABI asserts, the native BO2 pass (skinning, sRGB, lighting, view order, refusals) | `cargo test -p zhbo-gfx` (`tests/headless.rs`, `tests/ffi.rs`, `tests/native.rs`) |
| Native viewmodel (Track R) | blob formats round-trip and refuse damage (`zhbo-native`); the core's request/answer loading, composition, posing and the ADS pose layer without a GPU — the 0x55/0x56 `tag_torso` overlay over the action clip, its fraction scrub and clip switch, and the clip-less eye-frame slide fallback; end to end into a headless renderer | `cargo test -p zhbo-native`; `cargo test -p zhbo-core nativevm` and `cargo test -p zhbo-core --features renderer nativevm`; the real-install conversion test checks every converted viewmodel's blobs (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets converts_from_real_install -- --ignored`); the ADS pose against BO2's measured per-family table, the twin (`ads_down` frame 0 == `ads_up` frame N) and the launchers' rotation (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_ads_clip_reproduces_bo2s_authored_pose -- --ignored`); the equipment's authored sub-bone parking and the `stored local + track` playback rule (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets equipment_subbone_tracks_are_bo2s_authored_parking -- --ignored`); the base parts — the snipers' scope is loaded at its authored offset, the ACOG/`is` replace it, the M32's hip sight loads once (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_sniper_scope_is_a_base_part the_acog_replaces_the_base_scope the_m32_sight_loads_once the_pistol_dualclip_suppresses_nothing -- --ignored`); the IR combination moves the base scope and adds the dual-band mount (`ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_ir_combo_moves_the_base_scope_and_adds_the_mount -- --ignored`); the AS50 base unique parser test (synthetic zone: `cargo test -q --release -p zhbo-formats the_base_unique_carries_the_overlay_material -- --nocapture`); the converter's synthesized combos' overlays (`cargo test -q -p zhbo-assets weapons::tests::the_synthesized_combos_overlay_follows_the_mounted_optic`; `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_installs_synthesized_combos_overlay_follows_the_mounted_optic -- --ignored`) |
| Native third person (Track R7) | the body and world-weapon manifests round-trip (`zhbo-native`); the clip resolver's state scan (`zhbo-assets`); the core's third-person composition, state selection and world placement without a GPU | `cargo test -p zhbo-native -p zhbo-assets -p zhbo-core nativebody`; the real-install conversion test additionally checks every faction body (72 inches, weapon tag, all states' clips, textures) composes with every world weapon (`ZHBO_BO2_DIR=… cargo test -p zhbo-assets converts_from_real_install -- --ignored`) |
| Asset conformance | BO2/ConquerOps zones load with 0 bytes left over; weapon table round-trips; shell ejection matches BO2 — exactly the 41 defs that define `viewShellEjectEffect` and their FXList oriented at `tag_brass` on BO2's axes; others' brass and tracers (`shotfx`: the AN-94's placed `fx_rifle`, `assaultrifle` every 3rd bullet, the scoped LMG combos' `lmg_scope`; the sim's cadence in the fake-host `others_see_brass_every_shot_and_a_tracer_every_nth_bullet`) | `cargo test -p zhbo-assets`; install-gated: `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets every_mp_weapon_converts_and_plays_like_bo2 converts_from_real_install -- --ignored` |
| BO2 conformance | Every fact of every weapon+attachment combination (and the create-a-class lists) the mod ships against BO2's own rules over the player's install: one CSV schema, `expected.csv` (BO2) vs `actual.csv` (the mod), clustered diffs. Fails while any gap exists — no allowlist | unit: `cargo test -p zhbo-conformance` (schema, CSV, differ, BO2 rules on synthetic zones), `cargo test -p zhbo-formats havok_lua big`; the gate: `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets conformance -- --ignored` (~110 s; files in `ZHBO_CONFORMANCE_OUT`, default the temp dir; it fails while gaps remain, so run it apart from `-p zhbo-formats -- --ignored` — cargo stops at the first failing crate); the files for analysis: `cargo run --release -p zhbo-assets -- conformance <dir outside the repo>` (see "BO2 conformance" below) |
| In-game scripted test | full loop: take control, move, fight, throw both offhands, toggle the grenade launcher, field computer, RTS hand-back. The script seeds a v2 profile with one custom class in the active set — the AN-94 with its under-barrel launcher, plus the frag, concussion, pistol and knife — because BO2's premades carry no attachments and the alt-mode step needs one; the autopilot picks that custom from the spawn screen by its computed row key (the match list is the set's custom slots, then the defaults, `changeclass.lua`) | `scripts/autotest.ps1` (Windows) / `scripts/autotest-mac.sh` (macOS; seeds `ConquerOps.profile` with the GL custom before launch — the standard autopilot picks the first profile custom when there is one, else the first default class) |
| Shot impacts in game | the default autopilot run greps `player 0 impacts:` (flesh on the rebels, a terrain surface on the misses) and the bridge's `impact fx: N played, 0 missing` line — the latter proves the name resolved and `FXList::doFXPos` ran | `scripts/autotest.ps1` / `scripts/autotest-mac.sh` (default leg) |
| Scope-overlay test | the AS50 in hand, hold ADS past the 0.40 s transition; the native draw logs "base optic hidden at full ADS" and the HUD draws the 2D overlay | `scripts/autotest-mac.sh --scope` |
| W3D ADS-layer test | the W3D fallback (`ZHBO_NATIVE_VIEWMODEL=0`) composes the authored 0x55/0x56 clip over the action clip (`HAnimComboClass` pivot maps) and the slide is skipped; the bridge logs "viewmodel: ADS layer" + "viewmodel: ADS slide skipped" | `scripts/autotest-mac.sh --w3d-ads` (Windows: `scripts/autotest.ps1 -Mode w3d-ads`) |
| Real input test | OS keyboard/mouse path the autopilot does not cover | `scripts/inputtest.ps1` / `autotest-mac.sh --input` |
| Overhead General | his class guns swapped in (`armed slot 0 with ZHBO_GeneralWeapon_`), rounds counted, the command bar's orders given the way the buttons give them — the lethal at a rebel (thrown as first person throws), a reload, the pistol, the knife at a rebel spawned close (`meleed`) — then first person keeps the pistol. `--rts-dry` runs it with no spare rifle, then no spare pistol, rounds: the dry gun hands over | `autotest-mac.sh --rts` / `--rts-dry` (`ZHBO_AUTOPILOT=rts`; macOS only, `autotest.ps1` has no RTS mode) |
| Create-a-class run | the in-match screen is pick-only and shows the profile's ACTIVE class set: the script seeds a v2 profile with one custom (MP7 primary, claymore — deliberately unlike any premade kit) in set 1, picks it from the spawn screen by its row key (the custom leads the match list, `changeclass.lua`) and the sim logs `assumed the AB class`, with no profile write; the once-a-second pose line then names the MP7, proving the *kit* landed; it then leaves first person, takes control again, forces the screen open and Escapes it, and first-person play continues | `ZHBO_AUTOPILOT=classpick` with either autotest script (the macOS script seeds `ConquerOps.profile` first, then checks `assumed the AB class`, `MP7 <n>\|` in a pose line, no `create-a-class: saved` line, a second `entered first-person control`, and `taking control again`) |
| Shell create-a-class | the lobby's class screen (the menu ABI): browse, edit, save, delete, copy, close — in the shell, no match | manual only: the in-game autotest does not cover the shell (see below) |
| Class-screen screenshots | the shell list, the in-match pick list and the Pick-10 editor rendered through `classui::draw_screen` | `cargo run -p zhbo-assets -- screenshot` (uses the locally built `ConquerOps_BO2.big`; PNGs in `target/cac-shots/`) |
| Viewmodel sweep | every player weapon and every attachment it offers drawn through the game's native first-person path (composition, `nativevm::place`, the native BO2 pass), hip and aimed side by side, for a human review of the catalog | `cargo run --release -p zhbo-core --example render_viewmodels --features renderer -- <outdir>` |
| Combo conversion coverage | the converter's legal-combination set equals BO2's own Create-a-Class rows and covers every statstable-backed base weapon; the archive carries a native viewmodel manifest per legal combination (the base set only when `ZHBO_CONVERT_ALL_COMBOS=0`), and only the base set carries the composed first-person W3D meshes and the third-person body's world meshes (the legal combinations fall back to the base mesh; every legal combination has its native `.zbw` world-weapon manifest) | the real-install tests `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets the_enumerator_matches_the_conformance_expected_rows the_legal_combo_set_covers_every_base_weapon -- --ignored`; the build-time measurement: time `env ZHBO_CONVERT_ALL_COMBOS=0 cargo run --release -p zhbo-assets -- build-archive <base.big>` against `env ZHBO_BO2_DIR=… cargo run --release -p zhbo-assets -- build-archive <full.big>` and compare the two `.big` sizes (the full build must stay under 2^32 bytes; both the first-person split and the third-person base set keep it there) |
| Third-person world meshes | every legal combination's `.zbw` carries BO2's third-person parts (base `attachWorldModel` slots, then the resolved attachments, default tag `tag_weapon`); every base weapon's world meshes on the soldier are numbered `W<vm>00..W<vm><n-1>` with `n` its `WorldMeshes` (the bridge shows `k < n`), every vertex rides the hand and every index is valid; `number` closes the gap `finish` leaves after the dropped clan-tag/emblem surfaces | `cargo test -p zhbo-assets world_meshes_are_numbered`; `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets every_world_weapon_shows_all_its_meshes -- --ignored`; the world parts (`the_world_parts_are_the_base_slots_then_the_attachments`, `the_resolved_unique_walks_the_childs_chain`, the conformance fixture `world_parts_default_to_tag_weapon`), a combination's W3D fallback (`a_combination_shows_its_base_weapons_world_meshes`) the body-mode mount (`a_tagless_world_part_rides_the_weapon_in_body_mode`) and the third-person camo (`each_world_part_wears_its_own_camo_asset`, `the_class_camo_follows_the_primary_into_third_person`, the `.zbw` round trip) |
| Camo swap | the parser's synthetic/real camo reads; the `.zcm` round-trip and refusals; the converter's 38 weapon blobs (including the view-model join for `camo_xpr50`) and 7 excluded optic sets; the class camo's transport and the draw-swap; the renderer's second texture and tint | `cargo test -p zhbo-formats`; `cargo test -p zhbo-native`; `cargo test -p zhbo-core`; `ZHBO_BO2_DIR=… cargo test -q --release -p zhbo-assets the_camo_assets_convert_with_their_art -- --ignored`; the sweep's `--camo` (below) |

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

Units are BO2's (ms, inches, degrees; rotations as canonical quaternions); a float column's epsilon
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
   the editor returns to the list.
4. Edit something, walk to **SAVE CLASS**, confirm: `ConquerOps.profile` (in the game's user-data
   directory) is rewritten with the new class, and the list shows it highlighted.
5. On a custom row: **C** copies it (`NAME COPY` appears), **Backspace** deletes it — both rewrite
   the profile file.
6. **Escape** on the list closes back to the skirmish lobby (and the lobby's Escape works again).
7. Start a match: the saved class appears on the in-game pick screen (the set's five custom
   slots, then the default classes), which is **pick-only** (no editor, no delete/copy, untouched
   slots are inert — BO2 creates classes only in the lobby); the
   spawn offer opens in first person while unclassed, the command-bar button forces it, and Escape
   closes it and leaves the General unarmed (knife only) — the offer re-arms on the next
   take-control. `scripts/autotest-mac.sh --classpick` covers that flow.

Unit tests drive the same screen headlessly through `MenuState` (`crates/zhbo-core/src/menu.rs`:
browse → edit → save → file text out; delete; copy; close), and the FFI layer
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
writes nine shots: `cac-browse.png` (the lobby list: the active set's five custom slots, the set
header and the S/O footer), `cac-set-pager.png` (a browsed mid-row set), `cac-pick.png` (the
in-match list: the slots, then the default classes), `cac-editor.png` (a populated Pick-10 draft
with the primary option list open), `cac-editor-empty.png` (BO2's pristine empty class),
`cac-editor-weapons.png` (the category column and stat bars), `cac-editor-attachments.png` (the
attachment strip) and `cac-dialog-set.png`/`cac-dialog-class.png` (the S/O dialogs). Default size
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

**The poses and the notes.** A gun's pair samples its `IDLE` clip at frame 0; its aimed half is
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
pair's zero move (its hip and aimed halves are the same picture). Eye relief and the launchers'
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
left gun stays at the base clip's bind pose. The detection cascades: the weapon id's `_dw` suffix,
the `DWLEFTHAND` inventory, or the base pistol plus the `au_<weapon>_dw` attachment. The
install-gated test `the_dual_wield_composite_loads_both_guns` in `zhbo-assets` composes each of the
five DW pistols and asserts the composite adds the left gun's bones, vertices, surfaces and part
over the base composite. Run it with
`ZHBO_BO2_DIR=… cargo test -q --release -p zhbo-assets the_dual_wield_composite_loads_both_guns -- --ignored --nocapture`.
For a visual check, run the headless sweep with `--weapon fnp45_dw_mp` and inspect the hip pose's
left-palm region (x ≈ 180–360 px, y ≈ 220–340 px in the 640×360 slot): the left gun's geometry
should sit there, not at the torso center or floating near the base gun. The autotest's
`--classpick` leg keeps its MP7 seed — changing the primary would break the `MP7 <n>\|` kit
assertion — so to exercise the DW path in-game, edit the seeded profile's `secondary=` to a DW
pistol (e.g., `secondary=fnp45_dw_mp`) in a local capture run and confirm the left gun renders in
the left palm.

**The camo sweep.** `--camo <id>` (repeatable) draws the weapon with that class camo set and writes
`<weapon>/camo_<id>.png`; the manifest's `camo` column names the option. Compare it with the stock
`base.png` and the BO2 captures (`~/Documents/BO2-reference/cac/sweep/i-personalize/`). C0's
evidence: the atacs solid is 32×32 with alpha 41–47, the pattern 256×256 with alpha 0–110 and
edge-tiling (edge mean difference 4.2/3.2); `CAMO_GAIN` is 2.0 (1.0 is the fallback if the
comparison rejects it). The style camos' colour maps are largely the shared
`zhbo_camo_code_spc` and their differences live in `shaderConsts`, which the swap carries but does
not interpret (D8), so a style pair can look alike until the technique is ported.

## Autopilot and the input path

The autopilot replaces all input inside the Rust core, so `autotest.ps1` never exercises the OS
input path. `ZHBO_AUTOPILOT=manual` sets up the scene without scripted input, and
`scripts/inputtest.ps1` drives real keys and mouse through it.

Scripting real keys: `hold <VK> <ms>` needs a hex VK code (`hold 0x57 2500` for W); on SDL,
`scanmode on` switches `hold`/`press` to scan codes. macOS holds keys by code with
`osascript key code`, or `cliclick` for the mouse.

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
