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
| Camera | zhbo-core while `camera_override`, otherwise `W3DView` | Override sets view plane (FOV) and the first-person view's near plane: 0.1 units, returned by the bridge (`Zhbo_GetCameraOverride`); `setWidth()` restores the RTS FOV. The RTS view keeps its own `NearZ = MAP_XY_FACTOR = 10`. |
| Mouse/keyboard | zhbo-core while `camera_override` | `ZhboTranslator` (priority 5, before every stock translator) destroys raw mouse/key messages except Esc. Keys are polled with `GetAsyncKeyState`; the mouse is re-centred every frame. |
| Control bar / RTS UI | Hidden while `hide_rts_ui` | `HideControlBar` / `ShowControlBar`. |

## Units

* World: Zero Hour units. 1 ZH unit = 6 BO2 inches (ZH infantry `GeometryHeight = 12` ≙ a 72-inch BO2 player).
  All BO2 distances/speeds go through `units::inches`.
* The factor is defined once in `zhbo_native::ZH_PER_INCH`; `units::ZH_PER_INCH` and the art's
  `ART_SCALE` (`zhbo_assets::character::SCALE`, the native viewmodel) read it. One scale serves both,
  so a converted body's eye and the sim's first-person eye are the same world point (third person, R7).
  Stock ZH infantry mesh heads are taller than their collision cylinder (14.3–16 units vs
  `GeometryHeight` 12–13); the sim's 12-unit, 72-inch body is the reference.
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

## The General under RTS control: his weapons and orders

Zero Hour's AI moves, aims and fires the overhead General's three weapon slots; `zhbo-core`
(`sim/rts.rs`) owns what each slot is and what its shots do. Every logic frame the bridge reports
the shots and the slot in use (`zhbo_logic_rts_update`) and gets back `ZhboRtsWeapons`:

* **Each slot's Zero Hour weapon, by name** (`templates`, `names::rts_weapon`: one
  `ZHBO_GeneralWeapon_<id>` per BO2 player weapon, written by `zhbo-assets weapons::rts_weapons_ini`
  into `ZHBO_BO2_Weapons.ini`; attachments share their base weapon's). Slots 0 and 1 are the class's
  guns, with their BO2 fire rate, magazine, reload, reach and sound. Slot 2 is the current order's
  item, and its range is that order's reach. The bridge swaps a slot when its name changes
  (`Object::zhboSetSlotWeapon`, engine patch 0016). An empty name keeps the slot as it is: a
  classless General keeps the `ZHBO_General*` stand-ins, which the empty veto holds. A name the
  archive lacks is logged once and skipped.
* **The ammo, and the lock.** The arsenal is first person's: every RTS shot is counted against it,
  an empty slot is vetoed (`setPossibleNextShotFrame`), and `lock` moves him off a dry gun or back
  to his gun after an order.
* **The orders** (`third`, `lethal`, `orders`): the third slot does one thing per order: 0 throws or
  plants the lethal, 1 the tactical, 2 one knife strike. The command bar's Lethal, Tactical and
  Knife buttons are `FIRE_WEAPON TERTIARY` with one shot (`NEED_TARGET_POS`, or
  `NEED_TARGET_ENEMY_OBJECT` for the knife). Before Zero Hour appends its weapon message,
  `Zhbo_OnFireWeaponCommand` (patch 0016) sends `MSG_ZHBO_TAKE_CONTROL` with the action (1 lethal,
  2 tactical, 3 knife; 4 is Reload, a bridge command). In the logic step, which every peer runs
  on the same frame, `applyRtsOrder` does three things in order:
  - sets the order (`zhbo_logic_rts_offhand`, choice = action − 1);
  - arms that item's weapon at once, so its reach applies before Zero Hour aims it;
  - releases his permanent gun lock, because Zero Hour's temporary order lock can't override one.
  When the slot fires, the core does what first person does: the grenade flies on its arc to the
  AI's target point, equipment is planted in front of him facing it (`WeaponAction::Plant`), or the
  knife strikes (`Sim::melee_strike`). Zero Hour measures attack range edge to edge, so the RTS
  strike's reach counts both footprints (`MeleeReach::Footprint`). The Black Hat is never ordered
  overhead (it needs first person). `orders` greys out what can't be ordered; the order buttons'
  availability comes from it, not from the shared slot's weapon state (patch 0016's hook in
  `getCommandAvailability`).
* **Reload** (`zhbo_logic_rts_reload`): the gun in hand (or the one he goes back to) tops up from
  its spare rounds; the bridge starts Zero Hour's `Weapon::reloadAmmo` for the time.
* **Events** from orders happen inside `rts_update`, before the logic step, so the ffi logs them
  there (`log_events`), and `ConquerOps.log` shows "threw a lethal" or "meleed" for overhead
  orders too.

The buttons' faces are plain tiles: `zhbo_client_rts_icons` (`ZhboRtsIcons`) names his kit's BO2
icons, which the bridge draws over them with what each has left (the class-button pattern; his
class is the player's, so no conversion-time art can show it). Name slots are
`ZHBO_RTS_WEAPON_NAME_CAP` (48) bytes; a test keeps it equal to `names::RTS_WEAPON_NAME_CAP`.

## The native viewmodel (Track R)

With `ZHBO_NATIVE_VIEWMODEL=1` in a wgpu build, the first-person weapon is drawn by the renderer's
native BO2 pass from the converter's `zhbo-native` blobs (`Art\Native\*.zbm/.zba/.zbv` in
`ConquerOps_BO2.big`) instead of the W3D model. Free functions on the append-only ABI surface
(`zhbo_core.h`, `nativevm/ffi.rs`); presentation only, nothing reaches the sim.

* **The bridge asks each frame, the core decides.** `ZhboViewmodel_Update` builds a
  `ZhboNativeIn` from what it already has for the W3D model (clip, time, ADS fraction, eye and
  direction, the gun's view-space offset and angles, the faction letter from the `ZV<vm><F>`
  name) plus Zero Hour's global lights for the time of day (towards-the-light = `-lightPos`) and
  calls `zhbo_native_viewmodel`. `DRAWN` (1): the W3D model stays hidden and the weapon's own
  muzzle-flash/shell-eject FXLists play at `zhbo_native_bone("tag_flash"/"tag_brass")` when the
  sim's shot event reaches the client. The names come from `zhbo_viewmodel_fx(vm, ...)` — the
  weapon table's `ViewFlashFx`/`ViewShellFx`/`ViewLastShotShellFx`, converted from BO2's
  `viewFlashEffect`/`viewShellEjectEffect`/`viewLastShotEjectEffect` by `zhbo-assets`. The generic
  `ZHBO_ViewmodelMuzzle` FXList is the fallback when a weapon's **flash** was not convertible;
  brass has none, because BO2's rule is the def alone — a weapon without `viewShellEjectEffect`
  ejects nothing (131 of 172 MP defs, the AN-94 among them; no eject notetrack and no default), so
  the bridge skips the shell call when `ViewShellFx` is empty. The port is `tag_brass` plus
  `tag_brass1` (the `_lh` model's left port, present only on a dual-wield pair, whose guns the mod
  draws firing each trigger); a missing bone makes the second call a no-op. A shell's X is the
  eject direction, so its converted FXList sets `OrientToObject = Yes` and keeps BO2's axes, unlike
  the world/impact effects' normal-up swap; `vViewShellEjectOffset`/`Rotation` are not applied.
  `LOADING` (2) and `NOT_NATIVE` (0): the W3D path draws this frame. Viewmodel sound notetracks
  are unchanged (the bridge's `playNotes` runs before either path).
  - **Effects draw through the engine's `FXList` path, not the native pass.** The engine already
    owns particle systems, the blend modes from the material state bits, lifetimes and the world
    FX weapon pipeline; the native pass has no particle system and would duplicate all of it.
    Shell model elements are drawn as small sprites from the model's own colour map, because
    Zero Hour's FXList has no model nugget.
  - `zhbo_viewmodel_fx` is a free function on the append-only ABI surface (like
    `zhbo_loadout_viewmodels`); it needs the `ZHBO_ABI_VERSION` bump with its merge.
* **The core cannot open the archive; it asks.** After each call the bridge answers every
  `zhbo_native_wanted(i)` path with `zhbo_native_provide(path, bytes, len)` (`len < 0`: not in the
  archive, after which that viewmodel is given up and logged once) and prints every
  `zhbo_native_take_log` line. A viewmodel draws once its manifest, models and textures are in;
  clips fill in as they arrive (bind pose until then).
* **Where it draws.** Into `target_view` (BgfxBackend's `kBgfxEngineView`, 1) just before
  `before_view` (`kBgfxSceneCompositeView`, 9), with the scene view's own view and projection: BO2
  has no viewmodel FOV (`~/Documents/BO2-reference/ghidra/answers/G4.md`). The pass has its own
  depth buffer, so the world never cuts the gun.
* **The gun-position hook.** The gun's view-space transform enters as `GunPose`
  (`nativevm::GunPose::from_offset_angles`, offset in art units forward/left/up and yaw/pitch/roll
  radians). The core's `viewmodel::Motion` builds it from the weapon def's own gun-position numbers
  (`weapons::gunpos`, G4: stance/movement offsets, sway, bob, the gun kick spring, `fAdsAimPitch`
  and `fAdsBobFactor`), and hands the same offset and angles to the W3D path, so both rigs stay in
  agreement under `ZHBO_NATIVE_COMPARE=1`. The clip playback rate (`ZhboNativeIn.rate`) is the hook
  for G3's state-duration multipliers.
* **Placement and the aim axis (R6).** `nativevm::place` composes the model's root from the eye's
  frame (`ZhboNativeIn.eye`/`dir`: the client writes them from the sim's `Body::eye()` and the view
  angles, the eye→aim axis `sim::fire` traces shots along), the gun's transform and the art's
  scale. **The aimed pose is BO2's own animation data, not a computed alignment**: the
  `weapAnimFiles_t` 0x55 (`ads_up`) / 0x56 (`ads_down`) clips — a second anim channel that keys
  only `tag_torso`, the arms' rig root — are layered over the action clip with
  `zhbo_native::pose::sample_over` and scrubbed by the fraction: frame `ads · num_frames` on up,
  `(1 − ads) · num_frames` on down, where `num_frames` is BO2's last frame index, so `ads = 1`
  lands the authored aimed key. Rising plays up, falling plays down, held keeps the last direction
  (an aimed burst stays on the up clip's last frame); both clips are requested on the first aiming
  frame so a reversal never waits for its clip. The eye-frame slide (`place::aligned`, which moves
  the aim marker — `zhbo_native::AIM_TAG`: BO2's `tag_sights`, an optic's window when one is
  mounted (a sniper scope's own authored `tag_scope` for the `_scope_view` family), or the front
  sight the converter found — onto the axis by `ZhboNativeIn.ads`, BO2's
  `fWeaponPosFrac`, keeping its forward distance) is the **fallback**: it runs only when the
  manifest has no ADS clip (equipment, a partial conversion) or the clip has not loaded yet, and
  it is the W3D path's rule. Applying both would double-correct mid-transition and keep the hip's
  depth forever. The clip owns the whole hip→aimed transform, eye relief and the launchers'
  rotation included; the def's ADS time still owns FOV, the offset fade (`max(0, 1 − 2·ads)`),
  `fAdsAimPitch`, bob, sway and kick. Placement never derives an eye or axis of its own, so the
  shot's ray and the sight picture share one axis. (The camera axis carries the client's cosmetic
  view kick, `hud.view_kick_*`, which the sim's ray does not; the crosshair is drawn on the same
  camera axis, so the sight picture matches the crosshair while the sim stays authoritative for
  where the round goes.)
  - **The engine cross-fades 0x55/0x56 by the fraction (Ghidra G12, answered).** `FUN_0052b750`
    weights slot 0x55 by `fWeaponPosFrac` and 0x56 by its complement — no time-scale into the
    def's ADS times (those live in the fraction integrator, G3), and a direction flip passes no
    blend time (`0xffffffff`). Under the twin-clip property (down frame `(1 − ads) · N` ≈ up
    frame `ads · N` within 0.02″) the cross-fade coincides with the fraction scrub above, so the
    scrub is the cross-fade and reversal needs no blend — the mod's switch-and-step is parity.
  - **Authored residuals are parity, not placement errors.** The tag does not always sit on the
    axis: the KSG's `tag_sights` is 2.198″ low at 13.1″, the AS50's `tag_scope` 1.302″ below BO2's
    line (its clip is authored to a different reference), the launchers' heuristic marker 1.7–2.9″
    off. Never "fix" those onto the axis — BO2's data is the acceptance (test:
    `the_ads_clip_reproduces_bo2s_authored_pose`). `tag_scope`'s converter read is out of this
    point's scope: the clip path does not consume the marker, so it would change only the W3D
    fallback. The attachment mount is BO2's own rule (Ghidra G10): an authored `viewModelTag`
    mounts the part there with its offset in the tag's frame; an empty or missing one mounts at
    the gun's root `j_gun` (the engine's registered default) with the gun-local offset unchanged.
* `zhbo_native_reset` runs from `ZhboViewmodel_Reset` (a game reset tears down the scene); it frees
  the renderer's meshes and textures.
* ABI: the functions and `ZhboNativeIn` (168 bytes) / `ZhboNativeLight` (24) are new; the header and
  Rust carry size asserts. The `ZHBO_ABI_VERSION` bump to 24 covers them and the third person's
  surface.
* **A surface with no indices is a no-op, not corruption.** The composer never emits one (a surface
  whose triangles are all hidden by the hide tags drops with them, as the W3D path drops an
  all-hidden mesh), and if one ever reaches `native_create_mesh` the renderer skips it and logs it.
  A count that is not whole triangles or a range past the index buffer still refuses the mesh, and
  so does a mesh left with no drawable surface: the core then returns `NOT_NATIVE` and the W3D path
  draws, which is what keeps a third-person rig from going invisible.
* **The ADS part list (per part, at full ADS).** A `.zbv` part may name, in `ads_swap`, the
  `ads_only` part BO2 draws instead of it while aiming: an attachment unique's `viewModelADS` (the
  holo's and rangefinder's `_ads`) for its `viewModel`, and `attachViewModel[7]` for slot 0 (the
  M32). `nativevm::visible_parts` switches at `OPTIC_VIEWMODEL_SWITCH` (1.0, BO2's
  `cg_opticAttachmentViewmodelSwitch`): from it up the paired parts swap and every other part keeps
  its model; below it the `ads_only` parts are hidden. Presentation only; the hip and aimed meshes
  are built once at load and the draw swaps handles. The W3D path draws the hip models. The list
  keeps a scope under a 2D overlay: instead, `zhbo_client_frame` records the viewmodel whose
  reticle zoom is up (`nativevm::set_gun_under_overlay`, from `HudLui::gun_under_overlay`) and
  `zhbo_native_viewmodel` poses it but answers `DRAWN` without submitting (so the W3D model stays
  hidden too) — only when the overlay's art is in the archive, else the gun keeps drawing.

* **The class camo (`camo_<weapon>.zcm`, PLAN D1–D8).** A class's camo (`CustomClass.camo`) rides
  `PlayerSim.camo`; `zhbo_client_frame` records the local primary's `(viewmodel, camo asset,
  option)` in a core-global (`nativevm::set_local_camo`) and `zhbo_native_viewmodel` reads it when
  the `vm` matches — **no `ZhboNativeIn` field and no ABI bump**. The converter writes
  `Art\Native\<camo_asset>.zcm` (`ZCM\0`, v1): the asset's 40 pattern sets (solid/pattern image,
  UV offset/scale, gain), its 15 style recipes (base→camo material pairs; `replaceFlags` and
  `shaderConsts` carried verbatim, not interpreted), the converted style materials, and each option
  id's resolved `(pattern slot, style slot)`. `nativevm::camo::apply` first replaces style pairs by
  exact material name and clears those surfaces' layer, then paints the chosen pattern set onto the
  surfaces whose semantic-22 layer is `Pattern` (pattern image) or `Solid` (solid image); `Neutral`
  (the reflex `_camo2` group) and `None` draw stock. Options with no art (`camo_mtp`/`camo_snake`,
  the MTX-only rows) are recorded with no slots and draw stock; `camo_multicam` and `camo_matuka`
  resolve to the devgru (set 0) and bloodshot (set 6) patterns. The
  seven optic camo assets (`camo_reflex/holo/ir/acog/hybrid/mms/targetfinder`) are attachment
  recipes and are excluded from per-weapon blobs; `camo_b23r`/`peacekeeper`/`specter` do not resolve
  on the stock install (their weapons/models are outside the parsed `common_mp` table). Weapon
  matching is the weapon id, then a data-driven join through the weapon's BO2 view-model name
  (`camo_xpr50` → `as50_mp`, `camo_ballistic_knife`, `camo_riot_shield`). The `.zcm` version is new
  (`camo::VERSION = 1`); the model blob's material record gains the camo layer, and its version
  bump (2→3) and the archive bump (64→65) are integrator-owned.
  - **Divergences.** The camo applies to the first-person native viewmodel only: the W3D fallback
    bakes textures and has no texture override, and the third-person native body pins `camo` to
    `INVALID` (no third-person camo). Only the primary's viewmodel inherits the class camo, as the
    class stores one camo.

## The native third person (Track R7)

The converter also writes the General's third-person art as native blobs, so the renderer can draw
him and the weapon in his hand instead of the W3D conversion. Presentation only, as the viewmodel:
the sim's General object is unchanged, and Zero Hour still owns his health, targeting and death.

* **The body.** `zhbo-assets` writes each faction's MP body (`c_usa_mp_seal6_assault_fb`,
  `c_chn_mp_pla_assault_fb`, `c_mul_mp_pmc_assault_fb`) as one `.zbm` (skeleton, four-bone skin,
  surfaces and materials, in BO2 inches) and a `ZB<faction>.zbc` manifest naming it, the body bone
  a weapon hangs from (`tag_weapon_right`), and one `.zba` clip per movement state
  (`zhbo_native::thirdperson::state`: stand idle/walk/run/sprint/ADS, crouch idle/walk/ADS, prone
  idle/crawl, the pistol's idle and ADS). The clips are BO2's `pb_*` MP body animations. The
  converter tries the names the W3D conversion already proves (`pb_stand_alert`,
  `pb_combatrun_forward_loop`, ...) and then scans `common_mp` for a plausible one, because the
  exact spelling of the stances the W3D path never needed (walk, crouch, prone) can differ between
  installs; a state the body has no clip for falls back to the nearest stance and gait at run time
  (`state::fallbacks`).
* **The weapon.** Each converted weapon's `world_model` and its attachments' world models are
  written as `.zbm` blobs and a `ZW<vm>.zbw` manifest: the world model first, each attachment on
  its `worldModelTag` (or the model's origin when BO2 names no tag) at BO2's world offset, and the
  combination's `hideTags`. Offsets stay in BO2 inches; the renderer's scale is applied once, at
  draw. The body manifest never names a weapon: the core composes the two at run time, so a weapon
  switch reloads only the weapon.
* **Archive.** These are more files in `ConquerOps_BO2.big`; `zhbo_assets::hud::VERSION` must be
  bumped whenever converter output changes (left to the integrator on this branch).
* **The bridge asks each frame, the core decides (M2, the weapon).** `applyWorldPoses`
  (`ZhboRtsWeapons.cpp`) calls `zhbo_client_native_poses` for the overhead Generals' poses (the sim's
  `Sim::native_poses`: the held weapon's converted index, stance, body clip, clip time and rate, the
  pistol stance; first-person Generals are left to the viewmodel), fills one `ZhboNativeBodyIn` per
  General from the drawable's `getTransformMatrix` (feet and yaw), the drawable's
  `getCurrentWorldspaceClientBonePositions("tag_weapon_right")` (the engine's own world transform the
  W3D gun hung from), Zero Hour's global lights and the RTS camera position, and calls
  `zhbo_native_body`. `DRAWN` (1): the bridge hides the W3D `W<vm><k>` sub-objects. Anything else
  (0 not native, 2 still loading) leaves the W3D weapon showing — and the `s_worldLooks` cache
  remembers the native flag, so a failed native draw brings the W3D weapon back instead of leaving
  nothing. `ZHBO_NATIVE_BODY=1` draws the weapon while the W3D body stays; `=2` draws the body and
  the weapon and hides the W3D body (see *The two modes* below). `zhbo_native_body_enabled`
  is the mode the bridge tests before doing the third-person work at all.
* **The two modes.** In *weapon* mode (M2) the gun is placed by the engine's world-space
  `tag_weapon_right` transform (`ZhboNativeBodyIn.tag`, read from the drawable), scaled once at the
  root. In *body* mode (M3) the gun rides the body's own posed `tag_weapon_right`, and the whole
  rig is placed at the General object's world transform (`pos`/`yaw` from the sim's `NativePose`);
  the bridge's tag is ignored. The body's clip is picked from the movement state with the
  run-time fallback chain (`state::fallbacks`); the full BO2 transition graph and blend times are
  deliberately skipped. The bridge hides the whole W3D body in body mode when the native pass
  returns `DRAWN`, through the draw modules (`Drawable::getDrawModules()` →
  `ObjectDrawInterface::setHidden`), not `Drawable::setDrawableHidden`. The first-person path's
  `setDrawableHidden` sets `Drawable::m_hidden`; `Drawable::updateHiddenStatus` then deselects the
  General (`CanSelectDrawable`/`SelectionInfo` also refuse effectively hidden drawables). The
  draw-module call hides only the W3D render object and its effects (`W3DModelDraw::setHidden`) and
  leaves `m_hidden` false, so the General stays selected and commandable. The bridge re-asserts the
  hide every frame and restores the body to the drawable's own effective-hidden state as soon as
  the native pass stops returning `DRAWN`, so a refused draw never leaves an invisible,
  uncommandable General.
* **The core cannot open the archive; it asks.** After each `zhbo_native_body` call the bridge
  answers every `zhbo_native_body_wanted(i)` path with `zhbo_native_body_provide(path, bytes, len)`
  (`len < 0`: not in the archive, after which that rig is given up and logged once) and prints every
  `zhbo_native_body_take_log` line — the same channels as the viewmodel's. A rig draws once its
  manifest and models are in; its textures fill in as they arrive.
* **Where it draws.** Through the same native pass as the viewmodel: `target_view` 1 just before
  `before_view` 9, with the scene's own view and projection (`view_proj = None`, so the world camera
  frames it). The draw's frame carries the RTS camera's eye, Zero Hour's global lights and the scene
  ambient.
* **Placement.** BO2 inches are scaled once at the root: `root = transform × ART_SCALE`, so a skinned
  vertex is `root × pose × inv_bind × vertex`. The weapon-only mode's transform is the engine's own
  `tag_weapon_right` world matrix, so the native gun sits exactly where the W3D gun did. `state`,
  `time` and `rate` are carried for milestone 3's body clip; the weapon-only pose is rigid.
* ABI: the third person's functions (`zhbo_client_native_poses`, `zhbo_native_body_enabled`,
  `zhbo_native_body`, the file request/answer loop, `zhbo_native_body_bone`, `zhbo_native_body_reset`)
  and `ZhboNativePose` (28 bytes) / `ZhboNativeBodyIn` (204 bytes) are new; the header and Rust carry
  size asserts. The `ZHBO_ABI_VERSION` bump to 24 covers them and the viewmodel's surface.

## Shot impacts (R8d)

BO2 has no per-weapon impact effect: the weapon carries a bucket (`impactType`) and the effect is a
cell of the global `FxImpactTable` (32 surfaces, plus four body/head × nonfatal/fatal blood cells).
The mod reproduces that split: the **sim classifies**, the **client carries**, the **bridge plays**.

* **The sim emits one `EventKind::Impact` per pellet that hit** (`at`, `dir`, `surface`, `head`,
  `fatal`, `impact_type`); a shot that hits nothing emits none. `Shot` stays once per shot (the
  gunshot and the viewmodel) and no longer names an impact.
* **The surface is presentation data.** Objects are classified from the `objflags` the bridge
  already reports (`INFANTRY → FLESH`, `VEHICLE|AIRCRAFT → METAL`, `STRUCTURE → CONCRETE`); the
  ground from the new `LogicApi.surface_at(x, y)`, which answers Zero Hour's terrain class
  (`TheTerrainVisual->getTerrainTile`, the same query the logic's build placement uses) and a water
  table. It is read in the logic step but **never feeds damage, movement or any other sim state**:
  a peer whose terrain visual disagreed could only differ in its own screen.
* **The client carries at most `ZHBO_CLIENT_HITS` (8) impacts per frame** in `ZhboClientOut.hits`,
  filled by `consume_events` (so the impact sound is the surface's, not a guess); a frame that
  overflows drops the extras.
* **The bridge plays the name the core resolves.** `zhbo_impact_fx(impact_type, surface, head,
  fatal, …)` looks the cell up in the converted `Data\ConquerOps\impacts.txt` (loaded by
  `zhbo_load_impacts` at init) and the bridge calls `FXList::doFXPos` with a frame whose X is
  `-dir` (the surface's outward normal). A missing/stale archive plays nothing and is counted, never
  a bogus name — the same degrade-don't-panic rule as the renderer's draws.
* **What is out**: first-person tracers (BO2's `cg_firstPersonTracerChance` defaults to 0), other
  players' impacts (they keep the converted `FireFX`), exit/through-and-through impacts, and
  per-surface impact *sounds* (flesh vs hard only).
* ABI: `surface_at` is appended to `ZhboLogicApi` (fields 23 → 24), `ZhboClientOut` grows by 324
  bytes (4508 → 4832) and `ZHBO_ABI_VERSION` goes to 26; `zhbo_load_impacts` and `zhbo_impact_fx`
  are new free functions on the append-only surface.

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

## BO2 UI: fonts, materials and menu sounds

The screens ported from BO2's LUI Lua (`crates/zhbo-core/src/lui/`) draw with three converted
resources, all from the player's own install and all named by the registry:

- **Fonts.** `zhbo_assets::fonts` writes BO2's UI font atlas (`zhbo_bo2_fonts.tga`), one
  MappedImage per glyph (`names::font_glyph`, `ZHBO_Glyph_bigFont_65`) and the metrics file
  `names::FONTS_PATH`. The bridge passes that file to `zhbo_load_fonts` at init, next to the
  strings, tuning and classes. Without the converted archive the text is empty, nothing loads, and
  `lui::font::text` draws with the engine font (`Prim::Text`), so every screen keeps working.
- **Materials.** `lui::materials::MATERIALS` lists the BO2 materials the ports draw, by their BO2
  names. The converter turns each into the MappedImage `lui::materials::image(name)`
  (`ZHBO_Mat_<material>`), so a port keeps BO2's names end to end.
- **Menu sounds.** `sounds::UiCue` lists every alias BO2's multiplayer UI Lua plays. The converter
  makes each an AudioEvent `ZHBO_UI_<alias>` from the MP banks. Aliases that only the
  single-player banks have stay missing, matching BO2's silent MP menus; the host logs each missing
  event once.

The HUD (`crates/zhbo-core/src/hud/`) draws only through these and the existing callbacks. One
more is wanted: `ClientHost::image_quad` draws a MappedImage on four arbitrary corners (BO2 rotates
its damage-direction indicators and crosshair pieces). It is not on the C ABI yet, so the FFI host
answers false and those prims draw their outline fallbacks. Adding it is an append to
`ZhboClientApi` with a version bump. The damage indicators also need the engine to report where
damage came from (`HudLui::damage_from`); until then no indicator shows.

The scope overlay draws in the HUD's 2D pass while BO2's reticle zoom is up
(`weapons::scope::reticle_zoom`, `FUN_004b4760`): the weapon has an overlay material or reticle, and
`(ads − (1 − frac)) / frac > 0.01`, where `frac` is the def's `ads_zoom_in_frac` while aiming in and
`ads_zoom_out_frac` otherwise (the direction flips to "in" at `ads == 0` and "out" at `ads == 1`,
`HudLui::ads_zoom_in`). Its alpha is that zoom over `max(overlay_alpha_scale, 1)`. Over the same
window the viewmodel is not drawn at all (BO2's `drawGun = 0`), tested with the direction before the
frame's update. The overlay is a full-screen `Prim::Image`; its art carries the black surround and
the clear lens centre.

## Tuning

The gameplay tuning pack (`crates/zhbo-core/src/tuning.rs`, filled from
`Data/ConquerOps/tuning.ini`) is the single source for the numbers gameplay code used to scatter
across ~10 files (movement, regeneration, building rates, projectile physics, the field
computer's raise time) and for BO2's engine numbers: movement, the `perk_*` multipliers, damage
flinch, the view and bob dvars (v1.0 Track PW). BO2's numbers stay in BO2's units (inches,
inches/s, seconds) under the engine's dvar names as ini keys (`g_speed`, `perk_weapSwitchMultiplier`),
each field's doc citing the Ghidra answer it came from (`~/Documents/BO2-reference/ghidra/
answers/G1-G7.md`); code converts at the point of use. The pack is part of the mod like
`weapons.ini`: the bridge loads it once at init (`zhbo_load_tuning`, path from the names
registry), every player must have the identical file, and the sim must never read a tuning value
from anywhere else — modules take `&Tuning` (`MoveTuning::from_tuning`, `Arsenal::tuning`)
instead of their own constants. When the file is absent the built-in defaults keep tests and the
fallback working; a test asserts the shipped file sets every key to its default. Adding a knob:
one line in `tuning.rs`'s field table (field, ini key, default, cited doc) and the same key in
`data/ConquerOps/tuning.ini`.

**Perks are specialty sets.** What a perk does is the `specialty_*` bits it grants (the
converted weapon table's `Perk` blocks, read from the player's `mp/statstable.csv`; built-in
copies in `weapons/perks.rs`). Gameplay tests specialties, never perk names:
`Arsenal::has_specialty` is BO2's rule (the class's perks, or the weapon in hand's own bits and
its attachments'; G1).

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
