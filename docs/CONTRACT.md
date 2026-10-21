# Bridge contract: Zero Hour engine ↔ zhbo-core

Conquer Ops runs one process: a source-built Zero Hour (TheSuperHackers GeneralsGameCode at the commit
pinned by `host/GeneralsGameCode`, plus `host/patches/`) with the Rust core `zhbo-core` statically
linked in. Black Ops II never runs; the core rebuilds BO2's first-person rules, and BO2 content is read
from the player's own install.

Route (in the knowledge-base vocabulary): **rebuilt guest mechanic inside a real host**.

## Who owns what

| Thing | Owner | Notes |
|---|---|---|
| The General `Object` (health, armor, death, veterancy, selection) | Zero Hour | It is a normal `INFANTRY HERO` object; enemies target and damage it like any unit. |
| The General's position and facing while first-person | zhbo-core | `AIUpdateInterface::update` and `PhysicsBehavior::update` return early while `zhbo_logic_is_controlled(id)`. |
| The General's position while under RTS control (field computer up) | Zero Hour AI | Control returns through `set_controlled(id, false)`, which issues `aiIdle`. |
| Shots: when, where, what is hit, how much damage | zhbo-core | Damage is applied with `Object::attemptDamage` (`DAMAGE_SMALL_ARMS` / `DAMAGE_MELEE`), so ZH armor, retaliation and kill credit work natively. |
| Muzzle flash, tracer, fire sound | Zero Hour | `WeaponTemplate::fireWeaponTemplate(..., inflictDamage = FALSE)` with the `ZHBO_*_FX` weapons. |
| Camera | zhbo-core while `camera_override`, otherwise `W3DView` | Override sets view plane (FOV) and a 0.5-unit near plane; `setWidth()` restores the RTS FOV. |
| Mouse/keyboard | zhbo-core while `camera_override` | `ZhboTranslator` (priority 5, before every stock translator) destroys raw mouse/key messages except Esc. Keys are polled with `GetAsyncKeyState`; the mouse is re-centred every frame. |
| Control bar / RTS UI | Hidden while `hide_rts_ui` | `HideControlBar` / `ShowControlBar`. |

## Units

* World: Zero Hour units. 1 ZH unit = 6 BO2 inches (ZH infantry `GeometryHeight = 12` ≙ a 72-inch BO2 player).
  All BO2 distances/speeds go through `units::inches`.
* Damage: BO2 damage (100 HP scale) × `BO2_TO_ZH_DAMAGE` (1.5; a Ranger has 180 HP).
* Angles: radians. Yaw 0 = +X, counter-clockwise (same as `Object::getOrientation`). Pitch + is up,
  clamped to ±85°.
* Time: logic runs at 30 Hz (`LOGIC_DT`). The client runs per rendered frame with real `dt`.

## Channels

| Direction | Channel | Rate | When late / missing |
|---|---|---|---|
| Client → logic | `MSG_ZHBO_FPS_INPUT (objectID, held, pressed, move_packed, yaw, pitch)` | ≤ 1 per logic frame | The sim keeps the last input; `pressed` edges are OR-latched until consumed, so a tap is never lost. |
| Client → logic | `MSG_ZHBO_TAKE_CONTROL (objectID)` | on click | Ignored unless the object is a living `ZHBO_General*` owned by the sending player. |
| Logic → engine | `ZhboLogicApi` callbacks | during `zhbo_logic_step` | Synchronous. Callbacks must not re-enter `zhbo_logic_*`/`zhbo_client_*` (state lock). `zhbo_logic_is_controlled` and `zhbo_string` are lock-free. |
| Logic → client | `Sim::events` | drained every client frame | Capped at 512; oldest dropped. |
| Client → engine | `ZhboClientOut` | every client frame | Applied immediately by `Zhbo_ClientFrame`; the camera part is read in `W3DView::update` the same frame. |

Both network messages are ordinary lockstep `GameMessage`s, so they are replicated to all players and
recorded in replays. Everything in `sim` is deterministic given identical inputs (no wall-clock time,
xorshift RNG seeded from the General id and frame, objects sorted by id before use).

## The shell menu ABI (the lobby's create-a-class)

The lobby's Create a Class button opens the class screens *in the shell* (BO2's lobby create-a-class:
edit/manage classes outside a match). The window is the engine's — a full-screen modal window built
in code by the bridge (`host/zhbo/ZhboMenu.cpp`, on the engine's own modal-dialog pattern; host patch
0015 retargets the button to it) — and the screen is the same `classui` screens the in-game `Client`
hosts, driven by a small `MenuState` (`crates/zhbo-core/src/menu.rs`) with no `Sim` and no match.

Ownership and rules:

* **Pure local UI.** The menu never touches the sim: no determinism or lockstep requirements, no
  `GameMessage`s. Selecting a row *opens the editor* (in-game it would pick — there is no match);
  the profile file (`ConquerOps.profile`, written by the bridge on the `save_profile` one-shot,
  exactly as in-game) is its only output.
* **Free functions, not table fields.** `zhbo_menu_open` / `zhbo_menu_frame` / `zhbo_menu_close`
  and the `ZhboMenuIn`/`ZhboMenuOut` structs are an append-only surface like the rest of the ABI
  (bump `ZHBO_ABI_VERSION`, size asserts on both sides) but they do not extend `ZhboLogicApi`/
  `ZhboClientApi` — the field counts stay untouched.
* **The frame is the window's draw callback.** The bridge fills `ZhboMenuIn` (keys from the
  platform poll, the pointer/buttons from the engine's cursor, the wheel from the window's
  `GWM_WHEEL` messages, the screen from the display) and calls `zhbo_menu_frame`; inside it the
  core renders its prims through the already-stored `ZhboClientApi` draw callbacks (valid during a
  draw) and plays its UI sounds through the stored sound callback (`TheAudio` works in the shell).
  `ZhboMenuOut` then only needs `closed` (Escape/Back pops the window) and the profile one-shot.
  The core keeps its own prev-key state for edge detection, like `Client` does.
* **Lifetime.** `zhbo_menu_open` creates a fresh `MenuState`, `zhbo_menu_close` drops it; the
  bridge destroys the window on `out.closed` and from `Zhbo_Reset`, so no menu state survives a
  game transition (`zhbo_reset` drops it too). The shell-drawn image lookups share `cbImage`'s
  cache, invalidated by the same `Zhbo_Reset` hook as the in-game path (the MappedImage collection
  resets per game), so they cannot hold stale pointers across a game start.

## The command-bar class set

Selecting a General who has no class swaps the control bar to a one-page, code-built class command
set: Take Control plus the profile's ACTIVE class set — BO2's five premade classes followed by the
active set's up to five customs (BO2's model is ten sets of 5 custom + 5 default; the bar shows
one set, and `Back` is its way out). Once he has a class the normal set returns and its Create a
Class button swaps back for re-picks. The overlay pick screen exists only as the first-person
spawn select — the command bar never opens it.

Ownership and rules:

* **The core owns the data, the bridge owns the windows.** `zhbo-core`'s `classicons` module
  composes the slots, labels and kit-icon names and exposes them through
  `zhbo_class_buttons(ZhboClassButton *out, cap)`; the bridge (`ZhboControlBar.cpp`) builds the
  `CommandSet` at game start, places the present rows' `ZHBO_CHOOSE_CLASS` CommandButtons at the
  returned slots, swaps sets on selection, and draws each kit over its button every frame (the
  ammo-overlay pattern; the buttons' own `ButtonImage` is a plain tile). `zhbo_general_has_class`
  drives the auto-swap. Both are free functions on the append-only ABI surface (same rules as the
  menu ABI: bump + size asserts, no API-table fields).
* **The pick rides the same channel as the overlay, and only the core emits it.** The bridge
  learns *which* button fired by pointer-comparing the command window carrying
  `WIN_STATE_SELECTED` during `processCommandUI` against its registered 15, then hands the 1-based
  button index to the next client frame as `ZhboClientIn.class_button` (one-shot, 0 = none). The
  core maps it to the pick through the same code as the overlay — a 1-based registry index for a
  preset, `ZHBO_CHOOSE_CLASS_CUSTOM` plus the live profile entry's wire words for a custom — and
  emits `ClientOut.choose_class`, naming the General it adopted from the sim record. It cannot be
  queued bridge-side: `zhbo_client_frame` writes the whole `ZhboClientOut` fresh every frame, so a
  `choose_class` stashed there from the UI pump is overwritten before `applyClientOutput` reads it
  (the Phase-6 click-100%-dropped bug).
* **The General id is never ambient.** `Client` adopts the sim record's General every frame, so a
  pick sent from the overhead view before the first take-control names the General under command
  (a pick naming an unknown/zero General is dropped by the sim, which logs it).
* **Unarmed until classed.** A General with no class carries the knife-only `Loadout::unarmed()`:
  no guns, no grenades, no HUD ammo/crosshair; the RTS report marks every slot empty, and the
  bridge's existing no-ammo veto keeps Zero Hour's own weapons from firing until a class lands.

## Lifecycle

1. `GameEngine::init` end → `Zhbo_Init`: ABI check, **requirements check (both games must be installed,
   otherwise a message box and exit)**, host tables handed to Rust, Deploy General buttons injected into
   every `*CommandCenterCommandSet` (not `Fake*`) in the first free of UI slots 3, 2, 11, 12.
2. `GameLogic::startNewGame` / `GameLogic::reset` → `Zhbo_Reset`: client state released (cursor, hidden
   drawable, control bar), FX weapons freed, sim cleared.
3. Take Control → `MSG_ZHBO_TAKE_CONTROL` → `Sim::take_control` → `set_controlled(true)` → client
   enters the 0.9 s camera fly-in, then first person.
4. F (field computer) → `pressed & FIELD_COMPUTER` → 0.7 s laptop raise (General stands still) →
   `set_controlled(false)` → `ReturnedToRts` → client flies out for 1.1 s and centres the RTS camera on
   the General.
5. General dies → `GeneralDied` event → control released, "GENERAL DOWN" banner, camera returns to RTS.
   A new General can be built (`MaxSimultaneousOfType = 1`, linked across the three faction variants).

Not yet covered: save/load keeps no first-person state (a loaded game starts in RTS view).

### Engine gotchas (promoted from MODLOG)

- **Objects must be created inside `GameLogic::update`** (or a logic-step callback): W3DModelDraw
  resolves weapon barrel bones only while the logic updates. An object spawned from the client
  update gets a zero-sized recoil vector and crashes in `W3DModelDraw::handleWeaponFireFX` the
  first time it fires. Objects a test script spawns are made in the logic step too (`rtsAutopilotWave`).
- Zero Hour's `loadFileDirectory` reads `X.ini` *and* every `X\*.ini`: the generated archive adds
  files under new INI subdirectories (e.g. `Data\INI\SoundEffects\ZHBO_BO2.ini`) without
  replacing the stock ones. Objects load in sorted order, so the mod's file is
  `ZZZ_ZHBO_General.ini` (last).
- The General's art is generated into `ConquerOps_BO2.big` with a same-path
  `ZZZ_ZHBO_General.ini`; mod archives load in name order with overwrite, so it replaces
  `ConquerOps.big`'s Burton fallback. **Bump `hud::VERSION` whenever converter output changes**,
  or launchers keep the old archive.
- The first-person General's hidden third-person body still plays its FX weapon's FireFX right
  at the camera. `cbFireFx` skips the local first-person General; this is lockstep-safe because
  the `*_FX` weapons deal no damage and have no scatter, so `fireWeaponTemplate` only plays
  client-side FX and sound for them.
- After the camera override, reset pitch/zoom/pivot (`resetPivotToGround`) or the RTS camera stays
  low; leaving first person flies back to the overhead camera as first person found it, not what
  the view reports then.
- The RTS camera never shows sky, and `new_skybox` is drawn only by the water renderer, so first
  person paints its own gradient before the 3D pass (which clears depth but not colour).
- Zero Hour's `loadFileDirectory` reads `X.ini` *and* every `X\*.ini`: the generated archive adds
  sound events, audio settings, particle systems and FX lists as new files
  (`Data\INI\SoundEffects\ZHBO_BO2.ini`, ...) without replacing the stock ones. Objects load in
  sorted order, so the mod's file is `ZZZ_ZHBO_General.ini` (last).
- The General's art is generated into `ConquerOps_BO2.big` with a same-path `ZZZ_ZHBO_General.ini`;
  mod archives load in name order with overwrite, so it replaces `ConquerOps.big`'s Burton
  fallback. **Bump `hud::VERSION` whenever converter output changes**, or launchers keep the old
  archive.
- The autopilot replaces all input inside the Rust core, so `autotest.ps1` never exercises the OS
  input path. `ZHBO_AUTOPILOT=manual` sets up the scene without scripted input, and
  `scripts/inputtest.ps1` drives real keys and mouse through it.

## Names

Asset names the mod invents are the ABI between the converter, the glue and the core. Their single
source is the naming registry, `crates/zhbo-core/src/names.rs`: typed builders (`world_mesh`,
`viewmodel_skn`, `general`, …), the file paths inside the mod's BIG files, and the `_nsg` texture
suffix. The names the glue uses are generated from it into `host/zhbo/ZhboNames.h` (`cargo xtask
names`; the header is checked in and a zhbo-core test fails when it drifts). Both sides' tests
round-trip the builders (`W0302` → viewmodel 3, mesh 2). When the mod starts inventing a new name,
add a builder there and route every literal — Rust or C++ — through it.

## Tuning

The gameplay tuning pack (`crates/zhbo-core/src/tuning.rs`, filled from
`Data/ConquerOps/tuning.ini`) is the single source for the numbers gameplay code used to scatter
across ~10 files (movement, regeneration, building rates, projectile physics, the field
computer's raise time). The pack is part of the mod like `weapons.ini`: the bridge loads it once
at init (`zhbo_load_tuning`, path from the names registry), every player must have the identical
file, and the sim must never read a tuning value from anywhere else — modules take `&Tuning`
instead of their own constants. When the file is absent the built-in defaults in `tuning.rs` keep
tests and the fallback working; a test asserts those defaults equal the shipped file's values.
Adding a tuning knob: add the field to `Tuning`, give it the current hardcoded value as default,
parse it in `Tuning::from_ini`, add it to `data/ConquerOps/tuning.ini`, and route the old
constant's readers through the struct. The pack moves numbers; it must not change them.

## Versioning

`ZHBO_ABI_VERSION` (`host/zhbo/zhbo_core.h`) must equal `zhbo_abi_version()`; struct sizes are asserted
on both sides (`static_assert` in the header, `const _: ()` in `ffi.rs`). The API tables' field counts
are shared constants (`ZHBO_LOGIC_API_FIELDS`/`ZHBO_CLIENT_API_FIELDS` in the header; `LOGIC_API_FIELDS`/
`CLIENT_API_FIELDS` in `ffi.rs`), and `zhbo_abi_fields()` reports them at init: `Zhbo_Init` refuses to run
when either the version or the counts disagree, with a message naming the mismatch.

### Adding an engine service (a new field in `ZhboLogicApi` or `ZhboClientApi`)

The checklist, in order:

1. **Append-only**: add the new field at the END of the struct in `host/zhbo/zhbo_core.h`
   (inserting mid-table would shift every later pointer; designated initializers catch renames,
   not insertions).
2. Bump `ZHBO_LOGIC_API_FIELDS` or `ZHBO_CLIENT_API_FIELDS` in the same header, and the matching
   constant in `crates/zhbo-core/src/ffi.rs`.
3. Bump `ZHBO_ABI_VERSION` in both places.
4. Implement the callback in the glue (`host/zhbo/*.cpp`) and add it to the designated-initializer
   table in `Zhbo_Init`.
5. Declare the extern in `zhbo_core.h`, wire it into the Rust side (`host.rs`'s `LogicApi`/
   `ClientApi` and the `C*Host` impls in `ffi.rs`), keeping the table order identical.
6. Provide a fake in the tests (zhbo-core's `FakeHost` implements both traits; extend it so sim
   tests cover the new service).

### Save / load: the first-person state (documented follow-up, deliberately not implemented)

Zero Hour saves and restores the RTS game (units, money, buildings) through the engine's own
`SaveGame`/`LoadGame` pipelines; the engine does not know the mod's first-person state exists,
so today that state is lost across a save/load or level transition: the player snaps back to
third-person RTS control with whatever the engine persisted.

What would need serializing before the feature is possible:

- **`PlayerSim`** (crates/zhbo-core/src/sim.rs): position, view angles, health, stance,
  flags (`sprint`, `ads`, `on_ground`), hack/hammer timers, the active `Arsenal` (`WeaponState`:
  current weapon index, per-weapon ammo/mags, reload/offhand timers), and the RNG stream
  position (save the xorshift state so post-load rolls differ from pre-load, not repeat).
- **Projectiles** (projectiles.rs): every live projectile's position, velocity, fuse timers,
  owner, and damage profile — they are pure state, no engine handles.
- **Equipment in the world**: placed claymores/betties/C4/sensors (position, armed state,
  trigger timers) and the insertion marker state.
- **`Client` view state** (client.rs): `ViewMode`, viewmodel selection, `HudTimers`,
  `Autopilot` must NOT be saved (it is a test harness).
- **Serdes transport**: the ZH save hooks for mods are the `GameLogic::CRC`/`Save`/`Load`
  overrides on the player's logic object (see MODLOG: the same hook family that owns
  `GameLogic::update`). Data would go through `Xfer` blocks, size-capped, versioned with a
  `ZhboSaveVersion` constant — same append-only discipline as the ABI. Because the sim is
  deterministic given inputs, the saved state is small: the sim re-derives everything else
  from replayed inputs, so only the snapshot above plus the input-seed needs storing.

Open questions for a feature agent: how ZH migrates `Xfer` blocks across game versions
(engine save compat), whether a save/load must also replay `Client` timers to keep HUD state
coherent, and whether dead players (spectator between respawn) need a serializable form.
