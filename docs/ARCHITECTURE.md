# Architecture

The crate graph and the dependency rules. Read [CONTRACT.md](CONTRACT.md) for the engine ↔
Rust ownership split and [CONFIG.md](CONFIG.md) for every `ZHBO_*` variable.

## Crate graph

`A → B` means A depends on B (from the crates' `Cargo.toml`; leaves have no in-repo dependency):

```
zhbo-requirements, zhbo-formats, zhbo-native, zhbo-gfx      (leaves)
zhbo-conformance → zhbo-formats
zhbo-core        → zhbo-requirements, zhbo-native, (feature "renderer") zhbo-gfx
zhbo-assets      → zhbo-core, zhbo-formats, zhbo-native, zhbo-conformance, zhbo-requirements
zhbo-launcher    → zhbo-requirements, zhbo-assets
zhbo-voice       → zhbo-core (no default features: the command grammar only)
xtask            → zhbo-core, zhbo-formats, zhbo-assets (the dist keeps a current BO2 archive: `hud::archive_current`)

host/zhbo (C++) ←─ ABI ─→ zhbo-core
```

## Crates

### `crates/zhbo-requirements`
Finds and validates both game installs: Steam libraries, SteamCMD folders,
Wine/CrossOver/Proton prefixes. Depends on nothing. Used by the launcher and by zhbo-core
(install paths for the weapon table).

### `crates/zhbo-formats`
Parsers for game data: Zero Hour `.big` (from a file or, `big::members`, from memory), Black Ops II
`.sabs`/`.sabl` sound banks, `.ipak` image packs, `IWi` images, LZO1X, RefPack (`refpack`: Zero Hour's
`EAR\0` files, its maps among them), the BO2 fastfile decryption,
and the compiled UI Lua (`havok_lua`: Havok Script chunks and their constant stores). `t6zone::load_file`/`install_path`/`language_path` and `localize::install_strings` are the one way to open a zone of the player's install. Depends on nothing. All
readers are hardened against corrupt input (refactor plan Phase 0.3) and used by the
converter (`zhbo-assets`), the conformance oracle (`zhbo-conformance`) and `xtask`. The runtime
(`zhbo-core`) never parses BO2 files: it reads only what the converter wrote.

### `crates/zhbo-gfx`
The renderer: the engine's bgfx API subset implemented on wgpu, plus the build-time shader
translator. Depends on nothing. Split into `runtime` (`mod.rs` device and pools,
`resources.rs` resource calls, `encode.rs` per-draw calls, `frame.rs` frame phases, `native.rs` the
BO2 model pass), `state.rs` (pipeline/bind-group state), `shader.rs`, and `ffi.rs`
(the C ABI the engine's backend calls). Optional dependency of `zhbo-core` behind the
`renderer` feature.

### `crates/zhbo-native`
The mod's own blobs for BO2 art drawn natively (Track R), and the pose math that plays them:
`model` (`.zbm`: skeleton, LODs, four-influence skinned surfaces, materials with BO2's blend and
texture paths), `camo` (`.zcm`: a weapon's camo pattern sets, style material recipes and resolved
option slots), `anim` (`.zba`: keyframe tracks and notetracks), `viewmodel` (`.zbv`: the DObj
recipe — parts on tags with each part's camo asset, hidden tags, marker tags, slot → clip), `pose` (composition, sampling,
crossfade, additive layers, notetracks, the skinning palette), `tga` (reading the converter's
textures back) and `math`. It also holds `ZH_PER_INCH`, the one BO2-inches → Zero Hour-units
factor the converter's art (`ART_SCALE`) and the sim's `units` both read. Depends on nothing; the
converter writes it, the core reads it.

### `crates/zhbo-core`
The Rust gameplay core: BO2-style movement, weapons, hitscan, field computer, camera, HUD,
and the deterministic lockstep simulation. This is the mod's brain; everything reachable
from `zhbo_logic_*` must stay deterministic. Depends on `zhbo-requirements`, `zhbo-native` and
optionally (feature `renderer`) `zhbo-gfx`. After Phase 2.3:
- `weapons/`: `def.rs` (WeaponDef, enums), `table.rs` (the weapon table's schema,
  parse and format), `catalog.rs` (the converted table or built-in stand-ins, with BO2's perk rows and bonus cards),
  `perks.rs` (the perk ids and tiers, each tied to its statstable row: name, description, specialty set), `specialty.rs` (BO2's 62
  specialty bits), `feel.rs` (gun kick, sway, viewmodel offsets, reticle data), `handling.rs`
  (the fire cycle's extra times, the zoom levels/fractions/DOF, BO2's per-weapon traits),
  `recoil.rs` (per-shot kick, the gun kick spring, flinch, the spread cone's stance blend),
  `aim_spread.rs` (BO2's aim spread scale, one per player: its per-frame update, shot and event adds, the
  shot's spread in the cone and the pellet's place, `Bullet_Fire`'s linear radius), `view_kick.rs`
  (BO2's view kick integrator: the camera recoil that moves the sim's aim), `zoom.rs` (BO2's
  ADS zoom: the view FOV over the ADS lerp, the zoom levels, the hybrid's alt scope), `scope.rs`
  (the reticle zoom: when a 2D-overlay scope is looked through), `idle_sway.rs` (BO2's idle sway
  and hold breath), `gunpos.rs` (the client-side gun position: offsets, bob, sway), `loadout.rs`, `rng.rs` (xorshift),
  `pickup.rs` (a weapon off a player, `Carried`, with the options it wears, `WeaponOptions`; which
  weapon a death drops, the ammo a walk-over takes, the use key's swap — BO2's item rules),
  `weapon_state.rs` (one weapon's WeaponState: ammo, fire-cycle timers, the Reload in
  progress, the dual-wield hands), `arsenal.rs` (Offhand, Arsenal with HasSpecialty and the
  perk-timed weapon states, WeaponAction).
- `sim/`: `mod.rs` (Sim, PlayerSim, Event, the top-level `step` orchestration),
  `hack.rs` (Black Hat), `build.rs` (hammer/site), `plant.rs` (BO2's plant check for planted
  equipment), `deploy.rs` (the deployed Assault Shield), `drops.rs` (dropped weapons: the death drop,
  the fall, the 60 s expiry, the `g_maxDroppedWeapons` cleanup, the walk-over's ammo, the use list and
  the swap, and the step's kill witness that tells a suicide or team kill, which drops nothing, and
  hands each kill's killer, means and place to the Scavenger rule), `scavenger.rs` (BO2's Scavenger
  pack: the drop where the victim fell, its fall, the walk-over that refills a Scavenger General,
  the item cap it shares with the dropped weapons), `rts.rs` (the overhead General:
  `rts_update`, the third slot's orders `rts_offhand`/`Third`, `rts_reload`, `RtsWeapons`
  with each slot's Zero Hour weapon, `RtsIcons`), `fire.rs` (`apply_weapon_action`, `fire_round` (one bullet, to its end and through penetration),
  `melee_strike`, `gather_targets`, `throw_velocity`), `optics.rs` (the optic displays' rules:
  the MMS's characters, the target finder's spotting and marks), `guard.rs` (the victim's class
  perks against another General's blasts — Flak Jacket, Tactical Mask — applied where the damage
  reaches the host, as BO2's damage callback does), `shock.rs` (which shellshock a first-person
  General gets from a blast: the GSC's `shellshock()` sources, and the Shock Charge's zap — its
  `MOD_GAS` ticks and hidden HUD, `Taser`), `score.rs` (BO2's assists: the
  step's damage recorded through a pass-through host, each General's attackers this life, the
  assists a seat's kill credits), `lockstep.rs` (the state's
  fingerprint, `Sim::fingerprint`/`state_hash`: per-seat and world sub-hashes, the multiplayer
  gate's and Zero Hour's logic CRC's share), `antilag.rs` (BO2's antilag: the first-person
  Generals of the last frames, and the frame a shot is resolved against), `predict.rs` (the
  client's prediction's half in the sim: `Sim::scratch`, `Sim::predict_step` and the `Sandbox`
  host that reads the engine and writes nothing), and test code: `fakehost.rs` (`FakeHost`, the flat
  fake engine world) and `harness.rs` (the lockstep match: peers, the replicated command stream
  with the run-ahead, each machine's render cadence; `roster_classes`, every item the editor offers
  as legal classes), compiled for the crate's own tests and, behind zhbo-core's `harness` feature,
  for zhbo-assets' real-install gate (a dev-dependency only; the shipped static library never has
  it).
- `statehash.rs`: the fingerprint's streaming hash (`StateHash`, `state_hash!`): every sim type
  names each field (destructured without `..`, so a new field does not compile until it is hashed
  or skipped with a reason), floats by bits, definitions by id, no addresses or map order.
- `shellshock.rs`: BO2's shellshocks (`ShockDef`, the weapon table's `Shock` blocks converted from
  the install's `shock/*.shock` files) and their effect while one runs — the run-speed scale (the
  sim), the look control (the client's mouse look), and the presentation: the screen layers (the
  renderer's shock layer through `nativevm::set_local_shock`, the HUD's whiteout as its fallback),
  the view kick (the client's camera only) and the ringing loop (`Presented`, through
  `ClientHost::sound_loop`).
- `lui/`: BO2's LUI layout model shared by the HUD and the class screens: the anchor/edge
  resolution against a 720-unit root (`mod.rs`), BO2's converted UI fonts (`font.rs`), the
  BO2 materials the ports draw by name (`materials.rs`) and LUI's animation states (`anim.rs`).
- `hud/` and `classui/`: the first-person HUD and the create-a-class screens, one file per element
  or concern. `classmenu/` is the Pick-10 editor's state machine behind them (draft, legality,
  offers, wildcards, navigation); `classes.rs` the shared class list, `classicons.rs` the command
  bar's class buttons, `classrepair.rs` BO2's loadout repair for classes read back from the
  profile (`profile.rs`), `classwire.rs` a custom class's pick payload (catalog tokens),
  `menu.rs` the shell menu.
- `command/`: first-person commands (docs/COMMANDS.md), presentation-side: `ast.rs` (the command
  AST), `vocab.rs` (the match's word lists and the verb table with each verb's argument slots,
  `Vocabulary::problems`), `parse.rs` and `print.rs` (canonical text to the AST and back; the
  parser keeps every complete parse, so ambiguity is an error, and records what it expected where
  it stopped), `world.rs` (the ABI records `CmdObject`/`CmdButton`/`CmdOrder` and the `World`
  snapshot that names the types and the buttons' things), `actions.rs` (how the language covers
  every engine command type and order message: the one table of which buttons a verb presses, and
  the coverage gates), `cluster.rs` (the groups `in the <region> group` names), `sample.rs` (canonical
  commands over invented vocabularies for the voice training set; the `voice_corpus` example
  builds it in `tools/voice/`), `normal.rs`
  (the synonym and alias tables of `data/ConquerOps/commands.ini` and the normaliser),
  `resolve.rs` (a command to object ids and orders), `session.rs` (the command
  line's typing and echo, and `run`, which the FFI layer calls after the client frame). The HUD
  draws the line in `hud/command.rs`.
- `client/`: the per-render-frame client (mouse look, key bindings, one `FpsInput` per logic
  frame, the camera and its RTS ↔ first-person transitions, tracers, the autopilot harness,
  `events.rs`: the sim's events as the local player hears and sees them, and `predict.rs`: the
  local player's prediction over his inputs in flight, which first person is drawn from, and
  `spectate.rs`: the spectating client of an observer, a defeated player or a replay viewer, which
  follows first-person Generals at the sim's aim, sends nothing and never predicts). Nothing in it
  reaches the sim except through the input messages it emits.
- `building.rs` (the General as a builder), `impacts.rs` (the surface a shot hit and its BO2
  impact effect), `terrain.rs` (the ground's surface per height-map cell, parsed from the map
  file the bridge hands it: the logic-side answer under the sim's ground query),
  `soundlayers.rs` (third-person shots' per-layer distance curves), `torso.rs` (the third-person
  upper body's action table: which `mp/playeranim.script` torso clip a weapon's reload, knife,
  throw, shot or weapon change plays, and the clip's sound notetracks; `client/remote.rs` times it
  per General, plays its sounds and the third-person def sounds and voices, and `nativebody` layers
  it).
- `math.rs` (the world-space vector math, and `math::det`: the sim's transcendental functions through the pure-Rust `libm` crate, the one external dependency of the sim, so every lockstep peer computes the same bits — `the_sim_calls_no_platform_libm` keeps sim-reachable code off the platform libm), `config.rs` (env vars parsed once at init), `content.rs` (the multiplayer content fingerprint:
  the weapon table's `Role::Sim` fields, the class-pick vocabulary, tuning, classes, settings and
  build, in parts the lobbies compare and name on a mismatch), `names.rs` (the naming registry),
  `tuning.rs` (the gameplay tuning pack, BO2 units), `strings.rs` (the localised strings),
  `equipment.rs` (BO2 equipment behavior, driven by the weapon table's `Equipment` block),
  `shield.rs` (the Assault Shield on its bearer: the posed collision-surface box turned with the
  General, at his drawn scale (`units::DRAWN_ZH_PER_INCH`),
  the ray block and the blast protection; `sim/deploy.rs` plants, wears down and returns it),
  `smoke.rs` (BO2's smoke screen: its 12 s, the sight blocker's growth and BO2's sight test, which
  Zero Hour's targeting and pathfinder ask through host patch 0024),
  `penetration.rs` (BO2's bullet penetration walk over `hitscan`'s shapes, its depth table from
  the tuning pack), `hitloc.rs` (BO2's hit locations, their trace ranking and the soldier's hit
  boxes per body state from the weapon table).
- `nativevm/`: the native first-person viewmodel (Track R, `ZHBO_NATIVE_VIEWMODEL`): files asked
  of the bridge, composition and posing (pure, tested without a GPU), the renderer upload/submit
  (`gpu.rs`, feature `renderer`), the class camo swap (`camo.rs`: the `.zcm` recipe applied to the
  composed mesh, plus the core-global transport), the hold metric (`hold.rs`: the drawn gun's
  parts in the eye frame and whether one encloses the eye — tests and the sweep, no frame path) and
  its C ABI (`ffi.rs`).
- `nativebody/`: the native third person (Track R7): the General's body and the weapon in his
  hand through the native pass (`ZHBO_NATIVE_BODY`), with the same pure / `gpu.rs` / `ffi.rs` split.

### `crates/zhbo-assets`
The BO2 → mod converter: reads your BO2 install (through `zhbo-formats`), converts HUD art,
sound, weapon viewmodels and the weapon table, and writes `ConquerOps_BO2.big`. Also a CLI
to list/export BO2 sounds, images and fonts. `fonts.rs` converts BO2's UI fonts. Depends on `zhbo-formats`, `zhbo-requirements`, and
`zhbo-core`. After Phase 2.5 the module is split into `textures.rs` (the texture pipeline),
`ini.rs` (the INI rewriters), `viewmodels.rs` (the viewmodel conversion), `props.rs` (thrown,
launched and placed weapons as `ZHBO_Proj_<id>` objects at the General's drawn scale, the planted
shield in its deploy pose, and each droppable weapon's world model lying on its side as
`ZHBO_Drop_<base>`), `shield.rs` (the shield's collision-surface boxes), `generals.rs`
(the Zero Hour names, factions and the `files()` orchestrator), `camo.rs` (the camo art and the
`.zcm` recipes for every `WeaponCamo` a def or attachment unique points at) and `paks.rs` (pack-file access, the shrinker
and `load_zone`, over the shared `zhbo_formats::t6zone::load_file`). `thirdperson.rs` groups the weapons held in hand into animation sets by what BO2's
`mp/playeranim.script` plays for their `weapClass` and `playerAnimType` (`playeranim.rs`, its
first-match picker, which also reads the `EVENTS` torso clips), resolves each set's clip per movement
state and poses the soldier's hit boxes in each; it also groups every weapon (offhands too) into
torso sets by the `EVENTS` clip each action plays and writes the torso table (`torso_table`).

### `crates/zhbo-conformance`
BO2 parity conformance (see [TESTING.md](TESTING.md) "BO2 conformance"): the shared CSV schema
(`schema`, the `weapons` and `cac` column sets), one CSV writer/reader (`csv`), the differ that joins
the two sides and clusters every mismatch (`diff`), and the expected side (`bo2/`): BO2's own rules
— transcribed from the engine (Ghidra answers G14–G17 in the reference folder) and the UI Lua — over
the player's install, reading the raw weapon structs through the generated layouts (`bo2/raw.rs`),
never `zhbo_formats::weapon::read` or the converter. Depends on `zhbo-formats` only, so the expected
side cannot reach the converter's or the core's rules. The actual side lives in
`zhbo-assets::conformance` (the launcher's archive built in memory, the runtime catalog, the class
editor's offer).

### `crates/zhbo-launcher`
`ConquerOps.exe` / `conquerops`: requirement check, BO2 art conversion on first launch,
starting the game, and the voice models' background download (`voice`, `--fetch-voice`). Depends
on `zhbo-requirements` and `zhbo-assets`.

### `crates/zhbo-voice`
Voice commands (docs/COMMANDS.md section 11), on candle:
- `hear`: Whisper prompted with the match's names, over `whisper`, our port of the network with
  the decoder's self-attention cached and the encoder's window cut to the audio;
- `snap`: respells what Whisper heard into the match's words;
- `Translator` and `constrain::Mask`: the fine-tuned T5 under the grammar's mask (the grammar's
  word-level completions as token tries), reporting the probability the mask removed;
- `t5net`: our port of T5 (flan-t5) with every position's logits and a copyable decoder cache,
  and `options`: decoding by whole options or a token beam under the mask (experiments; the
  pipeline uses them when `ZHBO_VOICE_OPTIONS` is set);
- `pipeline` runs the three; `audio` brings sound to 16 kHz mono;
- the `conquerops-voice` binary: the game's voice process (microphone through `cpal`, a line
  protocol over pipes), staged beside the game by `cargo xtask build`;
- examples: `voice_eval` (the translator on the held-out set), `voice_listen` (spoken commands
  end to end, macOS `say`), `voice_match` (spoken commands over a match's kind of English names),
  `voice_heard` (training rows as Whisper hears them), `voice_trace` (one translation step by
  step), `voice_options` (option and beam decoding against the mask), `voice_audio` (Whisper
  encoder states for spoken commands, for `tools/voice/train/train_bridge.py`), `voice_judge`
  (scores a model's predictions).

The game side is `zhbo-core`'s `command::voice` (the link to the process, presentation only).
The launcher's `voice` module downloads the models listed in `data/ConquerOps/voice.ini`.

## The host bridge (C++)

`host/zhbo/*.cpp` is the engine-side glue. After Phase 2.1 it is split by responsibility:

| File | Responsibility |
|---|---|
| `ZhboBridge.cpp` | Globals, API-table init, `Zhbo_Init/Reset/…` entry points |
| `ZhboLogicApi.cpp` | Logic callbacks (`cbLog`…`cbFx`) |
| `ZhboRtsWeapons.cpp` | RTS shot tracking, the class weapons swapped into his slots, his orders (`applyRtsOrder`), the kit icon and ammo overlay |
| `ZhboHud.cpp` | Text cache, fill/line/text/image/clip/radar/cash |
| `ZhboInput.cpp` | `ZhboTranslator`, mouse capture, hidden object, RTS UI hide |
| `ZhboAutopilot.cpp` | The in-game test harness |
| `ZhboScreenLook.cpp` | Sky/BO2-look toggles |
| `ZhboControlBar.cpp` | `ZhboControlBarAccess`, idle-worker tracking |
| `ZhboBuilding.cpp` | First-person build/repair/ghost |
| `ZhboCommand.cpp` | First-person commands: the clickable-object and command-button lists, template names, orders sent as the overhead view sends them (the control bar's presses, the bridge's own `CommandTranslator` for targeted buttons and right-clicks, construction placement) |
| `ZhboDetail.cpp` | The render rate at each game start (`Zhbo_PaceNewGame`, host patch 0026); logs the graphics detail GameLOD applied, the dynamic LOD's changes and the frame rate (presentation only) |
| `ZhboMenu.cpp` | The shell create-a-class window (modal, code-built) and `Zhbo_MenuFrame` |

It talks to `zhbo-core` through the `ZhboLogicApi`/`ZhboClientApi` tables only — no direct
calls into Rust internals. Name literals shared across the boundary are generated into
`ZhboNames.h` (Phase 1.1).

## Rendering gotchas (from MODLOG, still relevant)

- wgpu loses the whole frame if one draw fails validation, so draws are range-checked before
  encoding and pipelines are built in an error scope (crates/zhbo-gfx).
- Unaligned dynamic index updates and failed R32F render targets made buildings vanish in RTS
  and the whole world vanish in first person. Both were silent in the game; only
  `zhbo-gfx.log` showed them.
- In the bgfx backend, 2D draws go to the UI view (last), so a sky painted with `drawFillRect`
  before the 3D pass hides the world. The wgpu build uses the scene view's clear colour.
- bgfx shaders through naga's GLSL frontend need: `mtxFromCols/Rows` macros, `#define`
  constants as uniform array sizes, and helper functions taking `sampler2D` split into
  `texture2D` + `sampler` (Vulkan GLSL has no combined-sampler parameters). Vertex and
  fragment uniform blocks must use different bindings (their layouts differ).
- Sorted (translucent) draws replay from the render state captured at insert; bgfx's
  `Set_Index_Buffer_Index_Offset` did not update it, so every translucent mesh in a shared
  vertex buffer read another mesh's vertices.
- The fork's SDL builds use `StdBIGFileSystem`, not `Win32BIGFileSystem`, so the `ZH_Generals`
  fallback in patch 0001 only matters for the DX8 build.

## Dependency rules

1. `zhbo-formats`, `zhbo-requirements`, `zhbo-gfx`, `zhbo-native` depend on nothing in this repo.
2. `zhbo-core` may depend on `zhbo-requirements`, `zhbo-native` and, behind the `renderer`
   feature, `zhbo-gfx`. It must never depend on `zhbo-assets` (the converter depends on the core,
   not the other way round).
3. `zhbo-assets` may depend on `zhbo-core`, `zhbo-formats`, `zhbo-native`, `zhbo-conformance` and
   `zhbo-requirements`.
   `zhbo-conformance` depends on `zhbo-formats` only — never on the core or the converter (the
   expected side must not share the mod's rules).
4. `zhbo-launcher` may depend on `zhbo-requirements` and `zhbo-assets`.
   `zhbo-voice` may depend on `zhbo-core` only, without its default features; the core never
   depends on it: the game reaches it as a separate process (`command::voice`), so the models and
   their runtime stay out of the static library.
5. Engine code (`host/GeneralsGameCode`) is never modified; changes go through
   `host/patches/*.patch`. The bridge (`host/zhbo/*.cpp`) is the only engine-side mod code
   and it owns the ABI in `host/zhbo/zhbo_core.h`.
6. Nothing from either game is shipped; converted data stays local (see
   [CONTRACT.md](CONTRACT.md) "Asset rule").
