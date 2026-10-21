# v1.0 — BO2 parity plan (temporary scaffolding)

> Planning document per AGENTS.md "Planning documents": deleted in the commit that finishes the
> work, after anything durable is folded into the permanent docs. Workspace version goes
> `0.1.0 → 1.0.0` in that final commit.

## Goal

Replace every guessed BO2 behavior, layout and look with ground truth from the reference
environment (`~/Documents/BO2-reference/`, see the `bo2-reference` skill), in four headline
areas plus everything else they touch:

1. Create a Class and class selection — art, logic, sounds, motion.
2. The first-person HUD.
3. Weapons in first and third person — models, materials, animations, attachments, sounds,
   effects, ADS alignment. **Rendered natively, no longer through W3D** (decision 2026-10-21).
4. Perks, especially movement.

### Deliberate divergences (keep them)

- **No scorestreaks.** The field computer and the build menu own the streak column of the HUD.
- **Zero Hour factions** replace BO2's on the HUD (emblem, team colours, names).
- **Command view**: first-person capabilities are reachable from the overhead view as **RTS
  orders the General carries out himself** (decision 2026-10-21). The primary/secondary swap
  stays a direct command-bar toggle. One-off actions (lethal, tactical, equipment, melee) each get
  their own targeted flow rather than a generic button.

### Reference constraints (decision 2026-10-21)

**No new captures.** Ground truth is the existing dumps only: Lua (`lua-dump/`), GSC
(`gsc-dump/`), menuDefs, the CaC captures (`cac/`), the asset catalog, and Ghidra. In-game
HUD/viewmodel feel is therefore verified against the *code* that produces it (Lua layout math,
weapon-def numbers, engine functions), not against video. Ghidra queries are read-only and self-serve
(approved 2026-10-21); answers land in `~/Documents/BO2-reference/ghidra/answers/G<n>.md`, listed
in [Reference queries](#reference-queries).

## Rules every track follows

- Values flow through the pipeline: the weapon table schema, the tuning pack, and the converter
  reading the player's own install (zone data, `mp/statstable.csv`, sound aliases, fonts,
  materials). Nothing is hand-copied from the dumps into code. Where only the engine knows a value
  (a dvar default inside `t6mp.exe`), it lands as a cited default in `tuning.rs` + `tuning.ini`.
- Every parity decision gets a citation (dump `file:line`, capture path, or Ghidra function) in
  the commit and MODLOG.md.
- Determinism: animation, sound and effect timing that feeds the sim reads logic time only.
- ABI (`zhbo_core.h`, `zhbo_gfx.h`) is append-only. **Only the integrator bumps ABI versions** —
  tracks append fields/functions and note the bump they need; integration serializes them.
- **Only the integrator runs the game.** Tracks run `cargo test --workspace` + clippy and their
  headless/conformance layers; in-game autotests run serially at integration (one machine, one
  game).

## Tracks

Parallel tracks run in separate worktrees; each owns its files and merges through the integrator.
Ownership is listed so two tracks never edit the same module.

### Track 0 — Foundations (integrator, first, blocking)

- ~~Split `classui.rs` and `hud.rs`~~ — done: `classui/` (layout, categories, edit, input,
  dialog, chrome, screens, tests) and `hud/` (one file per element, sprites, tests).
- The workspace is **not** clippy-clean today (130 pre-existing warnings, mostly `sim/`,
  `weapons/`, `hud/`); each track cleans the warnings in the files it owns.
- A shared **BO2 UI primitives** module the HUD and CaC both use: LUI virtual-screen mapping
  (1280×720 → screen), nine-slice drawing of BO2 materials, BO2 fonts, LUI-style tweens
  (`beginAnimation` duration/easing semantics), and a UI sound cue API (alias → engine audio).
- Converter: fonts (T6 `Font` assets) and UI sound aliases (`cac_*`, `uin_*`) into the BIG.

### Track R — Native BO2 rendering (largest; owns `zhbo-gfx`, `ZhboViewmodel.cpp`, `viewmodels.rs`)

Root cause of today's symptoms: viewmodels and third-person weapons are converted to W3D and drawn
by Zero Hour's fixed-function pipeline. W3D has no tangents, only five blend modes, and the
attachments are rigid sub-objects on `tag_weapon_right`.

- **R1 Data format.** The converter emits a mod-native model/anim blob (mesh, LODs, bones, skin
  weights, surfaces → materials with their `GfxStateBits` and image set, tags) and an anim blob
  (BO2 xanim tracks + notetracks), instead of W3D. Locally converted, never shipped.
- **R2 Pass.** zhbo-gfx gains a BO2 model pass (appended `zhgfx_*` functions): drawn in the scene
  view after the world, viewmodels with their own FOV and depth range (BO2's viewmodel FOV and
  near plane; Ghidra request G4). Draws validate against the ABI and log-or-skip.
- **R3 Skinning + animation in Rust.** GPU skinning; xanim playback with BO2's semantics —
  per-part blending, additive anims (ADS, sprint), notetracks emitted as events for sounds/FX
  (`sound`/`fx` notes) so reload sounds land on the frame BO2 plays them.
- **R4 Materials.** Reimplement the BO2 lit technique family in WGSL: colour, normal and
  specular/gloss maps, alpha test vs. blend from the material's state bits, sRGB correctness,
  reflection probes (MODLOG backlog #3). The pixel shaders in the player's own zone are read
  (DXBC disassembly) to learn the math; nothing is copied.
- **R5 Attachments.** Mount `au_<weapon>_<attachment>` models on their real tags; honour
  `hide_tags`; pick the **attachment combination's `WeaponVariantDef`** for animations, ADS
  offsets and sounds (fixes "wrong animation variants").
- **R6 ADS alignment.** Place the view so `tag_sights`/the optic's eye position sits on the
  view axis using the weapon def's ADS offsets and BO2's gun-position math (G4); aim point from
  the same transform the hitscan uses (sim stays authoritative).
- **R7 Third person.** The General's weapon from `world_model` on `tag_weapon_right` (+ world
  attachments), same native path; the General himself moves to the native path too.
- **R8 Effects.** Muzzle flash, shell eject, tracers, impacts from the weapon's `FxEffectDef`s
  (already parsed) on their tags.
- Retire the W3D viewmodel code once R1–R7 pass; the dist archive VERSION bumps.

> **Tracks W and P run as one track, PW** (decision 2026-10-21): both hinge on G1's HasSpecialty
> rule (class perks OR the weapon variant's OR its attachments' specialty bits), so splitting them
> would have two agents editing `weapons/`. PW also writes G4's gun-position math as a pure module
> (`weapons/gunpos.rs`) that Track R consumes.

### Track W — Weapon feel and sound (owns `weapons/`, `viewmodel.rs` state machine, `sfx.rs`)

- Extend the weapon table schema with the WeaponVariantDef fields that drive feel and sound:
  ADS in/out times, sway, bob, kick/recoil patterns, spread (hip/ADS/move), sprint in/out times,
  raise/drop times, fire/reload/ADS/raise sound aliases, and attachment overrides. The converter
  fills them; the schema round-trip test covers them.
- Viewmodel bob/sway/kick math from Ghidra (G4/G5). Crosshair spread drawing from G6.
- Sounds: player vs. world fire sounds, notetrack-driven reload sounds (from R3), surface-aware
  footsteps if the alias data has them.

### Track H — First-person HUD (owns `hud/`)

Port from `lua-dump/decompiled/patch_mp/ui_mp/t6/hud/` (and `patch_mp/ui/t6/hud/compass.lua`):
`ammoarea`, `ammocounter`, `offhandicons`, `weaponlabel`, `scorearea`, `scorebottomleft`,
`scorefeed`, `scorepopup`, `notificationpopups`, `gametimer`, `grenadeeffect`, `hudshaker`,
compass, plus `hud.lua`'s composition. Exact positions, sizes, colours, fonts, fades from the Lua.
Code-drawn parts (crosshair, hit markers, damage indicators, low-health overlay) come from Ghidra
(G6, G7).

- Divergences: the scorestreak column shows the field computer + build menu; the team
  emblem/colours are Zero Hour's faction.
- The scoreboard (`scoreboard.lua`, engine-defined columns) is in scope if the Tab map is kept
  separate from it; **open question** below.

### Track C — Create a Class and class selection (owns `classui/`, `classmenu.rs`, `menu.rs`, `ZhboMenu.cpp`)

Already logic-complete against `cacutility.lua`/`checkclasses.lua`. Remaining:

- **Art:** BO2's own materials for every panel (names from the Lua, resolved through the
  `menu_*` alias map), nine-sliced, replacing today's self-drawn chrome; BO2's fonts.
- **Sound:** the cue map from the Lua (`cac_loadout_edit_sel`, `cac_grid_nav`,
  `cac_grid_equip_item`, `cac_screen_fade`, `cac_cmn_backout`, `cac_restricted`,
  `cac_enter_cac`, `cac_slide_equip_item`, `cac_equipment_add`, `cac_closed`, `uin_*`), each
  site wired to the same event the Lua uses.
- **Motion:** LUI transitions (screen fades, slide-ins, highlight moves) with the Lua's durations.
- **Pixel pass** against `cac/sweep/` at 3024×1890 for every chapter (a–j).
- **In-match class change** per `hud/changeclass.lua` + `hud/class.lua` (class change applies on
  next spawn; the in-match class panel).
- Revisit deliberate omissions: personalize/camo/reticles are possible now that the renderer is
  native (camo = material swap). **Open question** below.

### Track P — Perks and movement (owns `movement.rs`, `weapons/perks.rs`, `tuning.rs`)

Today's perk effects are guesses (`perks.rs`: Lightweight `Speed(1.07)`, Dexterity
`SprintOut(0.5)`, Extreme Conditioning `SprintTime(2.0)`), and Dexterity is the wrong perk:
BO2's Dexterity is `specialty_fastmantle|specialty_fastladderclimb|specialty_sprintrecovery|
specialty_fastmeleerecovery` (`lua-dump/patch_mp/mp/statstable.csv:161`).

- Perks become **sets of specialties**, read by the converter from the player's
  `mp/statstable.csv` (column e). Each specialty is one effect hook in the sim.
- Specialty magnitudes and the base movement model (sprint duration/recovery, strafe/back speed
  scales, crouch/prone, mantle, ladder, jump, fall damage, stance changes) from the engine's PM
  code and perk dvar defaults (G1–G3), into the tuning pack with citations.
- Non-movement perks wire to their specialties too (Fast Hands, Hardline-equivalents become
  no-ops or ZH-adapted where streaks don't exist — listed per perk in the commit).

### Track O — Command view orders (owns `sim/rts.rs`, `ZhboControlBar.cpp`, control-bar INIs)

- Primary/secondary swap stays a direct toggle (as today).
- The General attacks with his current BO2 weapon as a normal RTS attack order (the BO2 weapon,
  its fire rate, magazine and reload — not the Zero Hour weapon).
- Targeted flows: **Throw lethal** (pick point → move into range → throw arc from the sim),
  **Throw tactical**, **Place equipment** (placement ghost using the equipment's placement data),
  **Melee** (pick unit → close in → knife). Each is a ZH command button with its own cursor and
  validity, driving the same sim actions first person uses.
- Reload as an order; the field computer and build stay as they are.

## Reference queries

**Answered 2026-10-21** (G1–G9, `~/Documents/BO2-reference/ghidra/answers/`, index in its
`README.md`; `dvars-t6mp.tsv` lists 330 dvar defaults). Key rule from G1: a player has a
specialty if the class, the weapon variant or one of its attachments grants it.

Read-only Ghidra on `t6mp` (static only); answers in `~/Documents/BO2-reference/ghidra/answers/`:

- **G1** Perk dvar defaults: every `perk_*` dvar registered (name, default, min/max), and which
  `specialty_*` bits each PM/weapon function tests.
- **G2** Player movement (`PM_*`): sprint (duration, recovery, multiplier, sprint-out),
  walk/strafe/back scales, crouch/prone speeds and transition times, jump, mantle timing, ladder,
  fall damage thresholds — the dvar names + defaults and the formulas.
- **G3** Weapon-switch/raise/drop/ADS timing as applied by the engine (where weapon-def times are
  scaled by specialties such as `fastweaponswitch`, `fastads`).
- **G4** Viewmodel placement: `cg_fov`/viewmodel FOV, gun offset dvars (`cg_gun_*`), the
  weapon-position function (ADS lerp, bob, sway, sprint pose), near plane for the viewmodel.
- **G5** Weapon kick/recoil application per shot (view kick vs. gun kick, recovery).
- **G6** Crosshair: spread → crosshair size math, colours, hit-marker drawing (size, duration,
  fade).
- **G7** Damage feedback: directional indicators and the low-health overlay (timings, curves).
- **G8** LUI animation easing and interpolation. **G9** LUI root, safe area, text placement.

## Integration order

1. Track 0 (blocking).
2. In parallel: R, PW, H, C, O. Merge order on conflicts: PW → R (R consumes gunpos and the
   playback rate) → H/C (share Track 0 primitives) → O.
3. Each merge: `cargo test --workspace`, clippy, in-game autotest (`scripts/autotest-mac.sh`,
   plus `--input`); commit with citations; MODLOG entry.
4. Finish: fold durable notes into RENDERER/CONTRACT/ARCHITECTURE/TESTING/MODLOG, delete this
   file, bump to 1.0.0.

## Scope decisions (2026-10-21)

- **Scoreboard:** Track H ports BO2's TAB scoreboard; the full-map radar moves to another key
  (bound through the `Key` enum, documented in CONFIG/README).
- **Personalize:** in scope. Track C adds camo, reticles, reticle colour and knife personalize per
  `cac/sweep/i-personalize`; camo and reticles render through Track R's materials (C depends on R4
  for those screens' previews and the in-game look).
- **Third person:** full BO2 MP body animations for the General (run, sprint, crouch, prone, ADS,
  reload, throw, melee) through the native path (R7), driven by the sim's player state.
