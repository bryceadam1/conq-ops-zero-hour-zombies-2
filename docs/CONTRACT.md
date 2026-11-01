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
| Camera | zhbo-core while `camera_override`, otherwise `W3DView` | Override sets view plane (FOV), the camera's roll (`ZhboClientOut.view_roll`, the view kick's roll axis, passed to `Matrix3D::Look_At_Dir` unchanged — both are a right-handed turn about the forward axis) and the first-person view's near plane: 0.1 units, returned by the bridge (`Zhbo_GetCameraOverride`); `setWidth()` restores the RTS FOV. The RTS view keeps its own `NearZ = MAP_XY_FACTOR = 10`. |
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
  ammo-overlay pattern; the buttons' own `ButtonImage` is a plain tile). Both overlays draw only
  while the selection is the one General and only over a button window that is on screen — it
  and every ancestor unhidden (`zhboCommandWindowShown`): an empty selection hides just the bar's
  `CommandWindow` parent and leaves the `ButtonCommandNN` children unhidden and still carrying the
  last set's buttons, so a test of the child's own bit paints kits over an empty bar.
  `zhbo_general_has_class` drives the auto-swap. Both are free functions on the append-only ABI surface (same rules as the
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
  direction, the gun's view-space offset and angles, and the faction letter `Zhbo_GetViewmodel`
  takes from the General's side — never parsed back out of the `ZV<vm><F>_SKN` name, whose `<vm>`
  is any width: a combination's is four digits, and the core refuses a non-letter faction with a
  log line) plus Zero Hour's global lights for the time of day (towards-the-light = `-lightPos`) and
  calls `zhbo_native_viewmodel`. `DRAWN` (1): the W3D model stays hidden and the weapon's own
  muzzle-flash/shell-eject FXLists play at `zhbo_native_bone("tag_flash"/"tag_brass")` when the
  sim's shot event reaches the client. The names come from `zhbo_viewmodel_fx(vm, ...)` — the
  weapon table's `ViewFlashFx`/`ViewShellFx`/`ViewLastShotShellFx`, converted from BO2's
  `viewFlashEffect`/`viewShellEjectEffect`/`viewLastShotEjectEffect` by `zhbo-assets`. The generic
  `ZHBO_ViewmodelMuzzle` FXList is the fallback when a weapon's **flash** was not convertible;
  brass has none, because BO2's rule is the def alone — a weapon without `viewShellEjectEffect`
  ejects nothing (the launchers, the crossbow, the M32; no eject notetrack and no default), so the
  bridge skips the shell call when `ViewShellFx` is empty. The def's effect is bound as BO2's asset
  pool binds it (`zhbo_formats::weapon::pooled`, every name resolved to the pool's live asset):
  `common_mp` carries `weapon/shellejects/fx_rifle` only as a reference stub and `patch_mp` the
  full asset, so the AN-94, the other ARs and the snipers do eject; the pistols' `fx_pistol` is
  `patch_mp`'s, over `common_mp`'s own copy. Every attachment combination in the table carries its base's converted
  flash/brass/shot (`zhbo_assets::weapons::inherit_converted_assets`, the silencer swap applied). The ports are per hand:
  `viewmodel_fired` (the right gun, or a single gun) flashes at `tag_flash` and ejects at
  `tag_brass`; `viewmodel_fired_left` (a dual-wield pair's left gun, appended to `ZhboClientOut`)
  at the `_lh` model's own `tag_flash1` and `tag_brass1`. A shell's X is the
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
    `zhbo_loadout_viewmodels`).
* **The dual-wield left gun's clip stays in Rust.** A `_dw_mp` weapon fires two guns (one per
  trigger, each with its own magazine), so its left gun plays its own `_LEFT` slot while the hands
  fire, reload or idle apart. The client hands that clip and time to the native pass with
  `nativevm::ffi::set_left_clip` beside `ZhboClientOut.viewmodel`; `ZhboNativeIn` carries only the
  base clip and its size is unchanged. The left clip id is the matching right slot with
  `INSTANCE_LEFT` (`viewmodel::slot::left_as_right`). Each hand's shot has its own fired flag
  (`viewmodel_fired`, `viewmodel_fired_left`; see the effects above).
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
  and `fAdsBobFactor`) — computed in **BO2's own frame and signs** (X forward, Y left, Z up;
  `AnglesToAxis` pitch positive = muzzle down, yaw left, roll right side down; every term the
  decompile's literal sign) and converted once by `gunpos::GunPose::view`/`view_angles` (only the
  pitch flips) — and hands the same offset and angles to the W3D path, so both rigs stay in
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
  rotation included; the def's ADS time still owns FOV (through `weapons::zoom`: the camera
  FOV is `cg_fov` moved toward the zoom level's FOV over the last `fAdsZoomInFrac`/`OutFrac` of the
  ADS lerp, computed client-side from the sim's ADS fraction and zoom level), the offset fade (`max(0, 1 − 2·ads)`),
  `fAdsAimPitch`, bob, sway and kick. **The gun's rotation (`fAdsAimPitch` and the move pose) fades
  by the same `max(0, 1 − 2·ads)`** (`FUN_008f0340`, whose angles both BO2's view weapon and its
  fire direction read), so it dips the gun mid-transition only: at full ADS the drawn gun is not
  turned and the aimed fire angles (`GunPos::fire_angles`) are the drawn pose's own pitch/yaw.
  Placement never derives an eye or axis of its own, so the shot's ray and the sight picture share
  one axis (the install-gated `the_aimed_sight_lies_on_the_shot_for_every_gun` measures it per gun
  and optic). (The view kick is the sim's,
  `PlayerSim::view_kick`: BO2 adds it to every user command's angles, so the sim's aim — the
  ray — is the input's view angles plus the kick the previous logic frame left, and the camera
  axis is the client's mouse aim plus the same kick, interpolated across the logic frame like the
  eye. Only the damage flinch, `hud.flinch_pitch`, is camera-only; the crosshair is drawn on the
  camera axis.)
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
  `PlayerSim.camo`, its secondary camo (`CustomClass.secondary_camo`, BO2's `secondarycamo`) rides
  `PlayerSim.secondary_camo` and its knife camo (`CustomClass.knife`, BO2's `knifecamo`) rides
  `PlayerSim.knife_camo`; `zhbo_client_frame` records one `(viewmodel, camo asset, option)` per held
  weapon (`PlayerSim::held_camos`) — the primary with the class camo, the sidearm with the secondary
  camo, the melee weapon with the knife camo, each through its own def's asset
  (`nativevm::class_camos`, `_class.gsc`'s `calcweaponoptions(class, 0|1|2)`). Each weapon's alt
  chain (`gl_`/`sf_`/`dualoptic_`) joins with the same option, as BO2's give (`FUN_00670460`)
  copies the weapon options onto every alt; the weapon currently held decides a viewmodel its alt
  shares, through its own def's asset (none = stock) — in a
  core-global (`nativevm::set_local_camos`), and `zhbo_native_viewmodel` reads the one whose `vm`
  matches the viewmodel it draws — **no `ZhboNativeIn` field and no ABI bump**. The converter writes
  `Art\Native\<camo_asset>.zcm` (`ZCM\0`, v1): the asset's 40 pattern sets (solid/pattern image,
  UV offset/scale, gain), its 15 style recipes (base→camo material pairs; `replaceFlags` and
  `shaderConsts` carried verbatim, not interpreted), the converted style materials, and each option
  id's resolved `(pattern slot, style slot)`. `nativevm::camo::apply` first replaces style pairs by
  exact material name and clears those surfaces' layer, then paints the chosen pattern set onto the
  surfaces whose semantic-22 layer is `Pattern` (pattern image) or `Solid` (solid image); `Neutral`
  (the reflex `_camo2` group) and `None` draw stock. A weapon's `CamoList` starts as every
  `camo` row; the camo pass keeps the base-tab rows and only the DLC/DLC2 rows whose art this install
  converted (`Camo::has_art`, `camo::offer_what_converted`), so an option with no art (`camo_mtp`/
  `camo_snake`) is not offered. Each row's `Tab`/`Mtx` (the weapon table's `Camo` block) places it on
  the class screen's CAMO / DLC / DLC2 tab (`AddCamos`' rule); `camo_multicam` and `camo_matuka`
  resolve to the devgru (set 0) and bloodshot (set 6) patterns. **Camo is per part, as BO2 resolves it (`FUN_00529600`, G16).** Each `.zbv` manifest part
  carries its `WeaponCamo` asset (`Part::camo`): none on the arms; the def's own `weaponCamo`
  pointer on the gun and its base `attachViewModel` parts (the magazine and its `tag_clip1` copy,
  a sniper's scope, the `_lh` gun its own def's); the attachment's **single** unique's
  `weaponCamo` on its models (an IR rail mount too) — so a reflex wears `camo_reflex`, not the
  gun's. `nativevm::load` reads every distinct asset the manifest names and paints each part's
  surfaces (`SurfaceSpec::part`) from its own blob with the class's one option id. The converter
  writes a blob for every asset a BO2 def or unique points at (47 on the stock install: the guns'
  and the seven optic assets plus `camo_specter`); only `camo_peacekeeper` is unreferenced. A def's
  `CamoMaterial` is its `weaponCamo` (the INSAS's `camo_msmc`, the AS50's `camo_xpr50`) — no
  name-derived alias. The `.zcm` format is `camo::VERSION`; the model blob's material record
  carries the camo layer (see the version list below).
  The material record (model and camo blobs alike, `Material::write`) then appends `specular`, the
  converted spec colour map (`*_spec.tga`, RENDERER.md "The specular colour"); model `VERSION`
  is 4, camo `VERSION` 2 (the parity burn-down's integration, see "Versioning"). A
  style camo replaces the surface's specular colour with the style's (gold's hue lives there).
  - **Third person.** The native third person (`ZHBO_NATIVE_BODY` ≥ 1) wears the class camo too:
    each `.zbw` part carries its `WeaponCamo` (`Part::camo`: the def's on the world model and its
    base slots, the attachment's single unique's on its models) and `nativebody::load` paints each
    part's surfaces (`SurfaceSpec::part`) from its own blob with the General's one option, read from
    the sim by General id (`Sim::third_person_camo`, no ABI change) when the weapon in hand is his
    class primary. The rig's cache identity includes the option.
  - **Divergences.** The W3D fallback (the first-person W3D viewmodel and the W3D third-person
    weapon, `ZHBO_NATIVE_BODY=0`) bakes textures and has no texture override, so it
    draws no camo. In first person each held weapon wears its own class camo (the primary
    `primarycamo`, the sidearm `secondarycamo`, the knife `knifecamo`; `nativevm::class_camos`);
    the third person dresses the General's class primary only.

* **The engine-filled surfaces: the lens reticle, the clan tag, the emblem.** BO2 fills three
  kinds of viewmodel surface at draw time; the converter now keeps them in the native models (the
  W3D path still leaves them out: it has no shader for them) and flags them by the material's
  technique set, never by name. The material record appends `reticle` (the reticle
  technique — `dynamic`, `dynamic_2colors`, `static`, `ui3d` — its constants by the shader's
  names, and the noise map) and `emblem`
  (`sw4_3d_player_icon`); RENDERER.md "The lens reticle" has the technique. The local primary's
  reticle choice rides a core-global like the camo (`nativevm::set_local_reticle`, set by
  `zhbo_client_frame` from `PlayerSim.reticle`: `reticle_N` is `weaponoptions.csv` row `N`,
  its group index — `FUN_0052a010` swaps only for a non-zero row, so `reticle_0` and no choice are
  the stock row 0) — **no `ZhboNativeIn` field and no ABI bump**; the composed viewmodel is keyed
  by it. The converter writes `Art\Native\reticles.zbr` (`ZBR\0`, v1, new): BO2's
  `mp/weaponoptions.csv` `reticle` rows (`t6mp` `FUN_00529360`: column 3 per optic, column 5 the
  reflex's RMR) as one material set per optic column in row order, row 0 the stock material the
  optic's model carries; `nativevm::apply_reticle` swaps each lens surface whose stock material
  starts a set. The reticle fades with `ZhboNativeIn.ads` (BO2's `fWeaponPosFrac`). Nothing in
  the mod knocks a player's optics out, so `optics_off` stays 0: the sim's EMP grenade
  (`Effect::Emp`, `projectiles.rs`) disables only non-infantry objects (vehicles and structures,
  `disable_kind::EMP`) and never the General, whom BO2's EMP would blind. The material record
  then appends `sonar` (the MMS screen, `sw4_3d_sonar_attachment_lens2`: its constants, `Lookup`
  and `Static_Noise_Map`). **The optic displays' inputs are the sim's** (`sim::optics`, per
  `PlayerSim`, deterministic): the MMS's characters (BO2's sonar rule, `FUN_004e7b40`) or the
  target finder's marked enemies (`FUN_00526780`) as `Optics::blips`, and the target finder's
  "enemy spotted" (`FUN_00528ec0`/`FUN_00529d80`) as `Optics::alt`; `zhbo_client_frame` hands the
  local player's to the viewmodel with a presentation clock (`nativevm::update_local_optics`) —
  again **no `ZhboNativeIn` field and no ABI bump**; `NativeFrame.optics`/`reticle_alt` are
  Rust-only. The converter writes `Art\Native\rangefinder.zrd` (`ZRD\0`, v1, new): the target
  finder's display — two baked frames, the atlas, the diamond's rectangle and size
  (`zhbo_assets::rangefinder`, read from the player's `t6mp.exe`). The weapon table gains `Mms` (BO2's `mmsWeapon`, `mmsMaxDist` in inches) on weapons
  and attachments. While the native viewmodel draws a lens
  reticle, the HUD's 2D optic reticle (the W3D fallback's stand-in) stays out, and so do its 2D
  target-finder diamonds
  (`nativevm::lens_reticle_drawn`, per thread). The clan tag draws as BO2 does for a player
  without one: its shader's four glyph indices are all 0 (`FUN_0052a010`), so the converter bakes
  the font atlas's first cell four times across (transparent on the stock install). The emblem
  shows the faction's Zero Hour logo (the parity plan's divergence: ZH factions replace BO2's
  emblem), resolved **at conversion**, not through the bridge: `zhbo_assets::emblems` reads the
  player's Zero Hour install (`zhbo_requirements`, `ZHBO_ZH_DIR`), finds `MappedImage
  <Side>_Logo` in its `Data\INI\MappedImages` INIs and crops that rect (right/bottom exclusive)
  from the page in an `Art\Textures` folder of its archives (`SCShellUserInterface512_006.tga` in
  `EnglishZH.big` on the stock install), writing `zhbo_native::emblem_path(letter)` per faction
  (`Faction::side`). `nativevm::load` puts the viewmodel's faction's logo on every
  `Material::emblem` surface; an archive without it keeps the material's own `emblem_clear`. The
  reticle colour option has no rendering effect found in BO2: `weaponoptions.csv` has no colour
  column and the reticle shader reads no code-set colour. The third-person body draws its world
  model's reticle with `ads = 0`, i.e. dark, as BO2 sets the fade for the local viewmodel only.
  The shared material record appends `reticle` and `emblem` (model `VERSION` 4, camo `VERSION` 2,
  bumped once at the integration, see "Versioning").

## The native third person (Track R7)

The converter also writes the General's third-person art as native blobs, so the renderer can draw
him and the weapon in his hand instead of the W3D conversion. Presentation only, as the viewmodel:
the sim's General object is unchanged, and Zero Hour still owns his health, targeting and death.

* **The Assault Shield.** Its world manifest's base part names its own body bone,
  `tag_weapon_left`, with the script's carry model (`_riotshield.gsc` `refreshshieldattachment`); a
  base part's tag overrides the body's `weapon_tag` (weapon-only mode ignores it). Carried with
  another weapon up, the body also wears the shield's stowed manifest (`ZS<vm>.zbw`: the stow model
  on `tag_stowed_back`). Which General carries a stowed shield comes from a lock-free mirror the core
  publishes after each logic step (`Sim::stowed_weapons` → `nativebody::ffi::publish_stowed`), read
  by `zhbo_native_body` through the frame's `general`: no ABI field. The W3D body still shows the
  shield in the right hand.
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
* **The weapon.** Every legal combination's world weapon is a `ZW<vm>.zbw` manifest (the models
  are shared `.zbm` blobs, converted once): the world model first, then BO2's third-person parts
  (`FUN_008ace30`, `viewmodels::world_parts`) — the variant's `attachWorldModel` slots (the
  magazine, a sniper's scope; slot 0 dropped by `disableBaseWeaponAttachment` and moved by
  `worldModelOffsetBaseAttachment`, slot 6 dropped by `disableBaseWeaponClip`), then each
  attachment's resolved unique (the child/sibling chain, `FUN_008eb690`) with its additional model
  — each on its tag, **`tag_weapon` (the world model's root) when BO2 names none** (the default
  `FUN_008ace30` is passed: `DAT_0240749c`, registered by `FUN_00678440`), at BO2's world offset,
  and the combination's `hideTags`. Only the base set (`converted_base`) also rides the W3D body as
  `W<vm><k>` meshes, numbered `0..WorldMeshes` with no gap after the dropped decal surfaces (the
  bridge shows `k < WorldMeshes`); the W3D body shows a combination's base weapon
  (`Catalog::world_viewmodel`). Offsets stay in BO2 inches; the renderer's scale is applied once, at
  draw. The body manifest never names a weapon: the core composes the two at run time, so a weapon
  switch reloads only the weapon.
* **Archive.** These are more files in `ConquerOps_BO2.big`; `zhbo_assets::hud::VERSION` must be
  bumped whenever converter output changes.
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
  nothing. `ZHBO_NATIVE_BODY=1` (the default) draws the weapon while the W3D body stays; `=2` draws
  the body and the weapon and hides the W3D body (see *The two modes* below); `=0` opts out.
  Each General keeps his own composed rig and GPU upload (keyed by `ZhboNativeBodyIn.general`;
  replaced when his weapon, mode or camo changes, all dropped by `zhbo_native_body_reset`), so
  several on screen do not reload one another's every frame. `zhbo_native_body_enabled`
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
* **The dual-wield left gun's clip stays in Rust.** A `_dw_mp` weapon fires two guns (one per
  trigger, each with its own magazine), so its left gun plays its own `_LEFT` slot while the hands
  fire, reload or idle apart. The client hands that clip and time to the native pass with
  `nativevm::ffi::set_left_clip` beside `ZhboClientOut.viewmodel`; `ZhboNativeIn` carries only the
  base clip and its size is unchanged. The left clip id is the matching right slot with
  `INSTANCE_LEFT` (`viewmodel::slot::left_as_right`). `viewmodel_fired` is the right gun's shot;
  `viewmodel_fired_left` (ABI 27) is the left gun's, flashed and ejected at `tag_flash1`/
  `tag_brass1`.
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
  `fatal`, `impact_type`, `exit`); a shot that hits nothing emits none. A penetrating round
  (below) adds its exit impacts (`exit` set) and its onward entries. `Shot` stays once per shot (the
  gunshot and the viewmodel) and no longer names an impact.
* **The surface is presentation data.** Objects are classified from the `objflags` the bridge
  already reports (`INFANTRY → FLESH`, `VEHICLE|AIRCRAFT → METAL`, `STRUCTURE → CONCRETE`); the
  ground from the new `LogicApi.surface_at(x, y)`, which answers Zero Hour's terrain class
  (`TheTerrainVisual->getTerrainTile`, the same query the logic's build placement uses) and a water
  table. The ground's surface is read in the logic step but **never feeds damage, movement or any other
  sim state**: a peer whose terrain visual disagreed could only differ in its own screen. An
  *object's* surface (its kind flags, sim state) does feed damage: bullet penetration's depth.
* **Bullet penetration** (t6mp `FUN_005d0350`; `zhbo_core::penetration`). After its first hit a
  round goes on through up to `penetrationCount` (5) solids while its damage multiplier (1 at the
  muzzle) stays positive: each solid costs its chord (≥ 1 in; the targets' convex shapes, exact)
  over the smaller of the entry and exit surfaces' depths for the def's `Penetrate` type
  (`info/bullet_penetration_mp`, doubled by `perk_bulletPenetrationMultiplier` with
  `bulletpenetration` — FMJ), and each next object takes `damage_at(distance) × multiplier`. The
  exit and the next entry play their impacts only when over `bullet_penetrationMinFxDist` (30 in)
  apart, at most two per round (the client's `FUN_00538e40` budget). The ground has no underside:
  a round that strikes it stops. All numbers are tuning-pack keys (`[Penetration]`, BO2's own key
  names); deterministic, sim-only inputs.
* **Soldier hit geometry and locations** (t6mp `FUN_007837a0`, `FUN_005f05c0`; `zhbo_core::hitloc`).
  An infantry target is BO2's MP body's hit boxes (`HitBox` lines of the weapon table: each bone's
  `XBoneInfo` box with `radiusSquared ≥ 4` and its `partClassification` location, posed by the
  converter at the first frame of every body state's clip: `HitBox = <state> <loc> <bone> …`),
  standing on the object's position and turned by its yaw. BO2 traces the pose the server animated
  (G_DObjCalcPose, `FUN_00649b50`); a General the sim moves takes the pose of his body state
  (`PlayerSim::body_state` = `nativebody::state_of` over the sim's stance, speed, sprint, ADS and
  sidearm — never the render clock), at his drawn `GENERAL_SCALE`; Zero Hour infantry (no BO2
  animation state) stand at 1.0. The state clips are BO2's `mp/playeranim.script` picks for a
  rifleman (`zhbo_assets::playeranim`), the same clips the third-person body draws. Of the
  boxes a ray crosses the highest-ranked location wins, nearest among equals (`bRifleBullet` ranks
  the head first; other bullets rank the body alike). A soldier's damage is multiplied by the
  def's `location_multipliers[location]` (a combination's resolved by `attachments::apply`:
  first mounted attachment unique differing from the base unique, `FUN_008ed780`); other objects
  take it unscaled. Penetration's chord through a soldier is the boxes' (the onward trace skips the
  soldier the round is inside). A table without `HitBox` lines (the built-in one) keeps the
  16-inch cylinder with a head band. Deterministic, sim-only inputs.
* **The client carries at most `ZHBO_CLIENT_HITS` (8) impacts per frame** in `ZhboClientOut.hits`,
  filled by `consume_events` (so the impact sound is the surface's, not a guess); a frame that
  overflows drops the extras.
* **The bridge plays the name the core resolves.** `zhbo_impact_fx(impact_type, surface, head,
  fatal, …)` (or `zhbo_impact_exit_fx` for a hit with `ZhboClientHit.exit`: the type's exit row,
  BO2's impact flag 4 — rows 1/4/8/10/6, `exit impact`/`exit flesh` lines) looks the cell up in the converted `Data\ConquerOps\impacts.txt` (loaded by
  `zhbo_load_impacts` at init) and the bridge calls `FXList::doFXPos` with a frame whose X is
  `-dir` (the surface's outward normal). A missing/stale archive plays nothing and is counted, never
  a bogus name — the same degrade-don't-panic rule as the renderer's draws.
* **What is out**: other players' impacts (they keep the converted `FireFX`).
* **The pellet's sound is the table's too**: `consume_events` plays `sounds::impact_cue(impact_type,
  surface)` — the `sound` row for the gun's type (`prj_bullet_impact_<size>_<surface>`), silent on a
  `NONE` row (BO2's hash miss), and the fixed flesh/hard cue only for a type with no sound rows (an
  unconverted archive). An exit plays `sounds::exit_cue` — the `exit sound` row of the type's exit
  family (`prj_bullet_impact_<size>_exit_<surface>`; the shotgun's
  `prj_bulletspray_impact_small_exit` has no alias in the install, so it is silent), nothing without
  one.
* **Blasts read the same table by their own impact type.** BO2's explode events (`FUN_004c0ac0`,
  `FUN_004c1590`) call the impact routine `FUN_00533210(weapon, surface)` — the type's
  `FxImpactTable` row and its sound family `<family>_<surface>` — then the def's own
  `projExplosionEffect`/`projExplosionSound`. The sim takes the blast's surface (the object it hit
  directly, else `surface_at`) and puts it in `EventKind::Exploded { at, sound, impact_type,
  surface }` (equipment too); `projectiles::blast` plays the table cell then the def's effect
  through `LogicApi.fx` (presentation, like every FX call), and the client's `consume_events`
  plays `impacts::sound_name(impact_type, surface)` (`sound <type> <surface> <event>` rows of the
  same file; every surface of a family type has one, `NONE` where the install lacks the alias)
  then the def's own sound. A missing *FX* cell falls back to the type's default
  surface (`SURF_TYPE_DEFAULT`); a missing *sound* cell plays nothing, as BO2's hash miss does
  (`FUN_0082df80`); a type with no sound cells at all and no def field falls back to the generic
  blast (an unconverted archive). The
  `impactType` → row/family mapping lives once, in `zhbo_formats::fx::IMPACT_KINDS`. No ABI
  change: the rows ride in the existing file and the existing `fx` callback.
* **Bounces and duds are their own events** (t6mp `FUN_004c1590`, sent by `G_RunMissile`
  `FUN_00608c10`). A bounce (event 0x47, each time a grenade or thrown piece of equipment rebounds)
  plays the def's `bounceSound[surface]` and the `grenade_bounce` row's cell for the surface; a
  projectile that strikes something before flying its `iProjectileActivateDist` of path
  (`FUN_00604280`; the launcher grenades' 300 inches) is a **dud** (event 0x51): no blast, the
  direct hit takes its `iDamage`, and it plays `prj_bullet_impact_large_<surface>`, the
  `projectile_dud` row's cell, then `projDudEffect`/`projDudSound`. The sim tracks the path
  (`Projectile::travelled`), plays the cells through `LogicApi.fx` and emits
  `EventKind::Bounced { at, sound }` (the def's `BounceSounds` cell for the surface, resolved in the
  sim) and `EventKind::Dud { at, sound, surface }`; the client plays them (`sounds::dud_cues`). Both
  rows are read cell by cell with **no default-surface fallback** (`impacts::fx_cell`; the bounce
  row has no flesh cell, so a grenade off a soldier raises nothing), and their constants live once
  in `zhbo_core::impacts` (`GRENADE_BOUNCE`, `PROJECTILE_DUD`, `DUD_HIT_SOUND_TYPE`). No ABI change.
* ABI: `surface_at` is appended to `ZhboLogicApi` (fields 23 → 24), `ZhboClientOut` grows by 324
  bytes (4508 → 4832) and `ZHBO_ABI_VERSION` goes to 26; `zhbo_load_impacts` and `zhbo_impact_fx`
  are new free functions on the append-only surface.
* ABI (penetration): `ZhboClientHit` gains `exit` at its end (40 → 44 bytes; `ZhboClientOut`
  only `hits_len` moves) and `zhbo_impact_exit_fx` is a new free function (ABI 27, see
  "Versioning").

## The view kick's roll

* `ZhboClientOut.view_roll` (appended; ABI 27, see "Versioning") is the sim's view
  kick roll (`PlayerSim::view_kick`, BO2's `-0.5 × yaw` axis), interpolated across the logic frame
  like the eye. The bridge returns it from `Zhbo_GetCameraOverride` and the `W3DView::update` hook
  passes it as `Look_At_Dir`'s roll. The viewmodel is placed in the unrolled eye frame, so the core
  adds the same roll to `viewmodel_angles.z` and the gun stays square to the screen, as BO2 draws
  it in the view's axes.

## The Assault Shield in Zero Hour's damage path

Zero Hour owns the General's health, so a shield must act inside the engine's damage. Host patch
`0017-assault-shield-damage.patch` calls `Zhbo_ShieldDamage(this, damageInfo)` at the top of
`Object::attemptDamage`; the bridge maps the `DamageType` onto `ZHBO_DAMAGE_*` (rounds: small arms,
gattling, sniper, Comanche vulcan; blows: melee; blasts: explosion, armor piercing, the missiles,
land mine, Aurora bomb; everything else `-1`) and asks `zhbo_logic_shield_damage(victim, source,
from xyz, from height, source known, kind, amount)` for the share to apply (it scales `m_amount`).

* **Lock-free.** The hook runs inside Zero Hour's object updates — possibly inside a callback of
  `zhbo_logic_step` (the sim's own `damage`) — so it never takes the state lock: it reads an
  atomic mirror of `Sim::shields` (every bearer's posed box, held or stowed, and each standing
  deployed shield with its object id) and of the four blast protections, written after each logic
  step and on take-control/reset. Deterministic: the mirror is sim state, the engine's call order
  is the same on every peer.
* **Damage the sim dealt passes** (a controlled General's source): the sim already tested it.
* **A held/stowed shield** (`shield::engine_scale`): a round or blow from a source whose line to the
  body's middle crosses the box before the body's axis is stopped (0); a blast keeps
  `1 − protection` with the source standing in for the blast; no source, or another kind, passes.
* **A deployed shield's object** (attackable: `ActiveBody`, a box of the model's bounds) takes no
  engine damage (0): the hook queues `(object, source, kind, amount)` in a fixed lock-free ring
  (64), drained into `Sim::engine_damage_to_deployed` before the next step — the script's
  wear-down scales on Zero Hour damage ÷ `BO2_TO_ZH_DAMAGE`.
* `zhbo_logic_shield_damage` is part of ABI 27 (`zhbo_core.h`, see "Versioning").

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

### The core's lifecycle and per-frame free functions

The functions the sections above do not cover, by when the bridge calls them (the header
`host/zhbo/zhbo_core.h` carries each one's comment):

| When | Function | What it does |
|---|---|---|
| `Zhbo_Init`, first | `zhbo_check_requirements(msg, cap)` | Both games installed? Non-zero with a message otherwise (the bridge shows it and exits). |
| `Zhbo_Init`, data load | `zhbo_install_weapons(table)` | The weapon table (`Data\ConquerOps\weapons.ini`); returns the weapon count, or -1. |
| | `zhbo_load_strings`, `zhbo_load_classes`, `zhbo_load_profile` | The mod's shipped strings (`strings.rs`), the class presets (`classes.rs`), the player's create-a-class profile (`profile.rs`); each module's defaults hold until its call. |
| Lockstep (game messages) | `zhbo_logic_take_control(player, general)` | `MSG_ZHBO_TAKE_CONTROL`: the General enters first person (Lifecycle step 3). |
| | `zhbo_logic_input(player, general, input)` | One frame's `ZhboFpsInput` for a controlled General (the event stream the sim replays). |
| | `zhbo_logic_choose_class(player, general, index, payload, len)` | The pick menu's choice: the shared list's index, a custom class's wire words appended as `[len, word…]`. |
| Profile writes (client) | `zhbo_profile_save(words, len)`, `zhbo_profile_write(words, len)` | Decode a saved custom class / a `save_profile` one-shot into the profile's full text for the bridge to write; null when the words do not decode. |
| Every client frame | `zhbo_client_draw_hud(local_player)` | Draws the first-person HUD through the `ClientApi` draw callbacks. |
| | `zhbo_client_rts_ammo(general, out)` | A General's weapon-slot ammo for the control bar (`ZhboRtsWeapons`). |
| | `zhbo_client_world_poses(out, cap)` | The Generals' third-person weapon/tablet poses (`ZhboWorldPose`); returns how many. |
| | `zhbo_world_mesh_count(vm)` | Third-person meshes of viewmodel `vm`'s weapon on the soldier model (-2: the tablet); cached by the bridge on first use. |
| | `zhbo_native_body_ready(in)` | 1 when the native third-person body frame can be drawn (see "The native third person"). |
| Building | `zhbo_general_build_rate()` | A General's building speed as a share of a dozer's (`building::GENERAL_BUILD_RATE`). |

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
- **What others see of a shot besides the flash** rides the same `fire_fx` callback (no ABI
  change). Per hitscan shot the sim also fires the def's `WorldShellFxWeapon` (its
  `WorldLastShotShellFxWeapon` on the round that empties the magazine, `WeaponAction::Fire.last`)
  — BO2's `worldShellEjectEffect` (`FUN_00531630`) — and, per bullet (each pellet), queues a
  `sim::ShotTracer` (shooter, def, target, end) when the shooter's tracer countdown says so
  (`sim::fire::tracer_due`, BO2's per-entity byte countdown `FUN_007c65c0`: every
  `TracerInterval`-th bullet, kept in `PlayerSim::tracer_count`, so it is deterministic and
  replayable). **The tracer's asset is the viewer's**: right after `Sim::step`,
  `zhbo_logic_step` hands `Sim::tracers` to `client::tracers::play`, which fires the def's
  `TracerFxWeapon` for a shooter on the local viewer's side (his own player's, or one Zero Hour
  calls `ALLY` relative to his General: BO2's `FUN_004bbf10`, same non-zero team as the local
  client) and its `EnemyTracerFxWeapon` (`enemyTracerType`'s walk, `FUN_008eb420`) for anyone
  else. The viewer is the last rendered frame's (`Client::viewer`). Which FX weapon plays differs
  per machine by design; it is as lockstep-safe as `cbFireFx`'s own-view skip (no damage, no
  scatter), and the countdown stays one per shooter because BO2's friendly and enemy tracers
  differ only in material (`the_enemy_tracers_differ_only_in_material`). Both are client-only FX
  weapons (`shotfx.rs`): the brass FXList is placed from the flash bone (`ZF<vm>`, the world
  model's `tag_flash`) to `tag_brass` with the particle nuggets' `Offset`/`RotateX/Y/Z`; the
  tracer is a Zero Hour `Tracer` nugget flying from the flash bone to the bullet's end. The
  overhead General fires Zero Hour's own weapon (no sim shot), so his `FireFX` is one composed
  FXList (`ZHBO_BO2FX_Rts_<id>`: flash, placed brass, and the tracer with `Probability =
  1/drawInterval`). `cbFireFx` skips the shooter's own first-person General (BO2's own view uses the
  view shells). **His own tracers** are BO2's too: the same countdown (`FUN_00539800` sets the draw
  flag for every shooter; `cg_firstPersonTracerChance` is registered with default 0 but never read
  in t6mp, so no chance roll and no client random stream), started at the viewmodel's flash tag
  (`FUN_00535f80`). `client::tracers::ViewTracers::play` queues a tracer whose shooter is the
  viewer's first-person General instead of firing it; the next client frame makes them that frame's
  (`begin_frame`, dropped outside first person), and the bridge's viewmodel pass pulls them with
  `zhbo_viewmodel_tracer(i, fx_weapon, cap, end, left)` (ABI 27; `left` 1 for a dual-wield pair's
  left gun, drawn from its `tag_flash1`) and plays the FX weapon's `FireFX` from `tag_flash`
  (native pose or W3D model) to the end with `FXList::doFXPos`. BO2 caps an own tracer's width at
  4 (`FUN_007c6420`, the local entity's `+0x7c` flag); every player gun's tracer is at most 4, so
  the cap is not modelled.
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

While the overlay is up the HUD also drops what BO2 drops, instantly (alpha 0, no fade): the damage
indicators (their gate is the reticle zoom itself, `FUN_004adda0`), and the minimap and compass
strip, which read `CoD.BIT_IS_SCOPED` (`weapons::scope::scoped`: the reticle zoom, unless the def's
`guided_missile_type` is TV-guided, 6). The optic's 2D reticle art does not draw under the overlay,
and the crosshair's side pieces take `1 − zoom / max(overlay_alpha_scale, 1)` before the
`cg_crosshairAlphaMin` floor. The ammo area, score panel, scorestreak column, hit marker and score
popup stay.

## Tuning

The gameplay tuning pack (`crates/zhbo-core/src/tuning.rs`, filled from
`Data/ConquerOps/tuning.ini`) is the single source for the numbers gameplay code used to scatter
across ~10 files (movement, regeneration, building rates, projectile physics, the field
computer's raise time) and for BO2's engine numbers: movement, the `perk_*` multipliers, damage
flinch, the view and bob dvars (v1.0 Track PW), the hold-breath `player_breath_*` dvars, `bulletrange` (how far a non-spread round is traced), the optic displays'
`cg_sonarAttachment*` and `cg_rangeFinder*` dvars (`[Optics]`, `sim::optics`), and the MP scripts' equipment, concussion and EMP numbers (`[Equipment]`, `[Concussion]`: the `scr_weaponobject_*`, `betty*`, `scr_proximityGrenade*` defaults and literals from `gsc-dump/mp/`, each key citing its `.gsc` line; `equipment::EquipTuning`, `projectiles::Flight`). Script values (not engine dvars) are keyed by their GSC `level.*`/`scr_*` name. BO2's numbers stay in BO2's units (inches,
inches/s, seconds) under the engine's dvar names as ini keys (`g_speed`, `perk_weapSwitchMultiplier`),
each field's doc citing the Ghidra answer it came from (`~/Documents/BO2-reference/ghidra/
answers/G1-G7.md`); code converts at the point of use. The pack is part of the mod like
`weapons.ini`: the bridge loads it once at init (`zhbo_load_tuning`, path from the names
registry), every player must have the identical file, and the sim must never read a tuning value
from anywhere else — modules take `&Tuning` (`MoveTuning::from_tuning`, `Arsenal::tuning`, `EquipTuning::from_tuning`)
instead of their own constants. When the file is absent the built-in defaults keep tests and the
fallback working; a test asserts the shipped file sets every key to its default. Adding a knob:
one line in `tuning.rs`'s field table (field, ini key, default, cited doc) and the same key in
`data/ConquerOps/tuning.ini`.

**Weapons are BO2's patched definitions.** The weapon table converts every `WeaponVariantDef`,
`WeaponAttachment`, model, clip and effect as BO2's asset pool resolves them over the multiplayer
weapon zones (`zhbo_formats::weapon::MP_POOL`, highest priority first: `patch_mp`,
`common_patch_mp`, `common_mp`). `DB_LoadXAssets` (`FUN_0059a550`) loads `common_mp` (zone flag
0x80) with its patch zone `common_patch_mp` (`"%s_patch%s"`, flag 0x100); `DB_LinkXAssetEntry`
(`FUN_00597000`) keeps the asset whose zone flag ranks higher in `FUN_00596e60` (`patch_mp`'s 2 →
0x41, 0x100 → 0x37, 0x80 → 0x36), and a `,name` reference stub never overrides. So the 79 weapons
and 49 attachments `common_patch_mp` redefines carry BO2's post-launch balance (the M8A1's 32-round
magazine, the SMGs' hip kick), and the Peacekeeper, defined only there, is a class item — listed
**locked** when its art did not convert (its gun images are reference stubs only the Revolution DLC
zone fills): the weapon table's `CacLocked`/`CacLockHint` (the hint is the localized
`MENU_<statstable column 15>_REQUIRED_HINT`), set by `zhbo_assets::weapons::lock_items_without_art`
from whether the view model's colour maps resolve; the picker draws BO2's lock
(`menu_mp_lobby_locked_big`, 64 square, the picture and bars hidden, `c-primary/007`), refuses the
pick, and `classrepair` drops a stored one. The
converter picks them in one place (`zhbo_assets::paks::mp_weapons`; models and clips through
`viewmodels::find_model`/`find_anim` over the same pool), and the conformance expected side reads
the same pool (`zhbo_conformance::bo2::install::Install::pool`). The stringtables
(`mp/statstable.csv`, `mp/attachmenttable.csv`, ...) are all `patch_mp`'s. The class menus name an
item by its statstable column 3 (`GetItemName`, the table's `CacName`, `WeaponDef::menu_name`); the
HUD by the variant's `displayName` (`DisplayName`) — the patched R870 is "R870 MCS" in the menus
and "Remington 870 MCS" on the HUD.

**Perks are specialty sets.** What a perk does is the `specialty_*` bits it grants (the
converted weapon table's `Perk <reference>` blocks, BO2's `specialty` rows of the player's
`mp/statstable.csv`, which also carry the localized name and description the perk grid shows; the
built-in table carries the same sixteen rows, and a table predating the text takes the stand-ins'
text). `weapons/perks.rs` only ties the perk ids and tiers to those rows (`Perk::row`). Gameplay
tests specialties, never perk names:
`Arsenal::has_specialty` is BO2's rule (the class's perks, or the weapon in hand's own bits and
its attachments'; G1).

**Wildcards are the statstable's bonus cards.** The converted weapon table's `BonusCard <reference>`
blocks are BO2's `bonuscard` rows (name, description, image, column-13 loadout slot, cost, sort
key, index, unlock level); the catalog lists them in the grid's order (sort key, then index) and
falls back to the built-in stand-ins when a table predates them. The editor's rules key on the
card's statstable reference (`classmenu::Wildcard::reference`), never on a list position; a
class's cards are not stored in the profile but re-derived from what it holds (Tactician from a
tactical-slot item in the lethal slot, Danger Close from the class spec's `lethal_count=2`, the
loadout's `primarygrenadecount` that the arsenal starts the lethal with). The spec's
`tactical_count` (1 or 2, BO2's `specialgrenadecount`; a second tactical is a point of its own) is
what the arsenal starts the tactical with, and the `lethal_count` (BO2's `primarygrenadecount`)
the lethal's: one per filled box of the slot (`cacgrenadesbutton.lua:130-230`), so one lethal is
one whatever the item's start count (a C4 too), two with Danger Close's second box filled (a
point of its own, like the tactical's), Tactician's chosen 1 or 2;
both are capped by the item's `MaxReserve` (`classmenu::items::offhand_cap`). The editor always
writes both counts; repair makes a held item's 0 (a class saved before the counts) one. A spec
without a count (the shipped presets) gives the item's start count.

**The class editor's UI strings come from the install.** The weapon table's `UiText <key>` blocks
carry the english text of the localize keys the core draws (`weapons::UI_TEXT_KEYS`: the offhand
titles, `MPUI_PRIMARY_GRENADE_CAPS`, `MPUI_SPECIAL_GRENADE_CAPS` and Tactician's
`MPUI_SECOND_SPECIAL_GRENADE_CAPS`), read by the converter from the same english zones as the
display names; `Catalog::ui_text` looks them up and a table without them takes the built-in
stand-ins (BO2's strings, checked against the install by an ignored test).

## Versioning

`ZHBO_ABI_VERSION` (`host/zhbo/zhbo_core.h`) must equal `zhbo_abi_version()`; struct sizes are asserted
on both sides (`static_assert` in the header, `const _: ()` in `ffi.rs`). The API tables' field counts
are shared constants (`ZHBO_LOGIC_API_FIELDS`/`ZHBO_CLIENT_API_FIELDS` in the header; `LOGIC_API_FIELDS`/
`CLIENT_API_FIELDS` in `ffi.rs`), and `zhbo_abi_fields()` reports them at init: `Zhbo_Init` refuses to run
when either the version or the counts disagree, with a message naming the mismatch.

**ABI 27** (the parity burn-down's integration, one bump for every append since 26):
`ZhboClientOut` gains `view_roll` (view kick) and `viewmodel_fired_left` (dual wield) at its end and
`ZhboClientHit` gains `exit` (penetration): `ZhboClientOut` 4832 → 4872 bytes, `ZhboClientHit`
40 → 44; `Zhbo_GetCameraOverride` returns the roll; `ZhboClientApi.sound_gain` is appended after
`ghost` (`ZHBO_CLIENT_API_FIELDS` 13 → 14); new free functions `zhbo_viewmodel_tracer` (tracers,
with the left-gun flag), `zhbo_impact_exit_fx` (penetration), `zhbo_logic_shield_damage` (Assault
Shield, host patch 0017) and `zhbo_load_sound_layers` (sound mixer). Blob versions at the same
point: archive `hud::VERSION` 78, `.zbv` `viewmodel::VERSION` 3 (part `camo` and `ads_swap`, in that
order after `ads_only`), `.zbw` `thirdperson::WEAPON_VERSION` 3 (part `camo`), `.zbm`
`model::VERSION` 4 (the camo layer, `specular`, `reticle`, `emblem`), `.zcm` `camo::VERSION` 2.
Polish round 1 moved only the archive: `hud::VERSION` 79 (the weapon table's `OverlaySize` key and
the atlas/backing changes); the ABI and the blob formats are unchanged.

### Third-person shots: BO2's chain, layer by layer (`soundlayers.rs`)

* **Converted.** For every converted gun's third-person chain (`ZHBO_BO2_FireNpc_<key>`) the
  converter also ships each audible layer as its own AudioEvent `<chain>_L<k>` (k = the chain index;
  `MinRange = MaxRange`, so Zero Hour does not attenuate it inside its reach) and a row in
  `names::SOUND_LAYERS_PATH` (`Data\ConquerOps\sound_layers.txt`): `layer <FX weapon> <chain>
  <event> <distMin> <distMax> <min curve: 8 x,y> <curve: 8 x,y>`, distances in Zero Hour units. The
  gun's FX weapon (`ZHBO_FX_<key>`) now carries `FireSound = NONE`; the chain's mixed event stays.
* **Loaded.** The bridge reads the file at init and passes it to `zhbo_load_sound_layers` (returns
  the layer count, -1 for null). Without it the core plays nothing for others' shots (the FX
  weapon is silent), so the file ships with the archive that sets `NONE`.
* **Played.** The client's `consume_events` turns each other player's `Shot` into
  `Cues::third_person_shot(def.fx_weapon, origin, listener)`: with a first-person listener, each
  layer whose BO2 curve (`soundlayers::distance_gain`, t6mp `FUN_0083d060`) is non-zero at the
  listener's distance, through `ClientApi::sound_gain(event, at, gain)`; with no first-person body
  (overhead), the chain's mixed event through `sound`. Presentation only (client side, no sim input).
* **ABI.** `ZhboClientApi.sound_gain` is appended after `ghost` (`ZHBO_CLIENT_API_FIELDS` 13 → 14 on
  both sides); the glue's `cbSoundGain` sets the AudioEventRTS volume to the event's INI volume ×
  gain (`setVolume` overrides). ABI 27.
  A host without the field plays the layer at its own volume (`ClientHost::sound_gain`'s default).

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
6. Provide a fake in the tests (zhbo-core's `FakeHost` in `sim/mod.rs`'s tests implements
   `LogicHost`, the HUD tests' `Recorder` (`hud/tests.rs`) implements `ClientHost`; extend the one
   the service belongs to so the tests cover it).

### Save / load: the first-person state (documented follow-up, deliberately not implemented)

Zero Hour saves and restores the RTS game (units, money, buildings) through the engine's own
`SaveGame`/`LoadGame` pipelines; the engine does not know the mod's first-person state exists,
so today that state is lost across a save/load or level transition: the player snaps back to
third-person RTS control with whatever the engine persisted.

What would need serializing before the feature is possible:

- **`PlayerSim`** (`crates/zhbo-core/src/sim/mod.rs`): position, view angles, health, stance,
  flags (`sprint`, `ads`, `on_ground`), hack/hammer timers, the active `Arsenal` (`WeaponState`:
  current weapon index, per-weapon ammo/mags, reload/offhand timers), and the RNG stream
  position (save the xorshift state so post-load rolls differ from pre-load, not repeat).
- **Projectiles** (projectiles.rs): every live projectile's position, velocity, fuse timers,
  owner, and damage profile — they are pure state, no engine handles.
- **Equipment in the world**: placed claymores/betties/C4/sensors (position, armed state,
  trigger timers) and the insertion marker state.
- **`Client` view state** (`client/mod.rs`): `ViewMode`, viewmodel selection, `HudTimers`,
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
