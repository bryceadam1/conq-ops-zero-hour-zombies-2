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
| A stun (concussion, flashbang, Shock Charge, EMP) on a General | zhbo-core: a shellshock on the first-person General (`crate::shellshock`: slower run and look; presented as the renderer's blur/grab/white layer, the camera's view kick and the ringing loop — see "The shellshock's presentation"); Zero Hour: `disable(PARALYZED)` on everything else (its units, a General under RTS control) | The first-person General never receives `disable(PARALYZED)`: Zero Hour's paralysis froze the player outright. `sim::guard` intercepts it and the sim plays BO2's shock instead. |
| Shots: when, where, what is hit, how much damage | zhbo-core | Damage is applied with `Object::attemptDamage` by the Zero Hour `DamageType` the tuning pack maps each hit's BO2 means of death to (see "Units", Damage), so ZH armor, retaliation and kill credit work natively. |
| Muzzle flash, tracer, fire sound | Zero Hour | `WeaponTemplate::fireWeaponTemplate(..., inflictDamage = FALSE)` with the `ZHBO_*_FX` weapons. |
| Camera | zhbo-core while `camera_override`, otherwise `W3DView` | Override sets view plane (FOV), the camera's roll (`ZhboClientOut.view_roll`, the view kick's roll axis, passed to `Matrix3D::Look_At_Dir` unchanged — both are a right-handed turn about the forward axis) and the first-person view's near plane: 0.1 units, returned by the bridge (`Zhbo_GetCameraOverride`); `setWidth()` restores the RTS FOV. The RTS view keeps its own `NearZ = MAP_XY_FACTOR = 10`. |
| Mouse/keyboard | zhbo-core while `camera_override` | `ZhboTranslator` (priority 5, before every stock translator) destroys raw mouse/key messages except Esc. Keys are polled with `GetAsyncKeyState`; the mouse is re-centred every frame. |
| Control bar / RTS UI | Hidden while `hide_rts_ui` | `HideControlBar` / `ShowControlBar`. |
| A dropped weapon (BO2's `dropweaponfordeath` item): which drops, where it lies, its rounds, attachments and camo, the walk-over's ammo, the use key's swap, its expiry | zhbo-core (`Sim::drops`, `sim/drops.rs`, `weapons::pickup`; the tuning pack's `[Drops]`) | Its Zero Hour object (`ZHBO_Drop_<base>`, `WeaponDef::drop_object`: the base weapon's world model at the General's scale, inert and unclickable) is spawned, moved and destroyed by the sim through `LogicHost::spawn`/`set_pose`/`destroy`; it is drawn only, nothing reads it back. Zero Hour's own infantry drop nothing (their weapons are Zero Hour's, not BO2's). |
| A smoke screen (BO2's smoke grenade): where, how long, how wide its sight blocker is | zhbo-core (`Sim::smokes`, `crate::smoke`, the tuning pack's `[Smoke]`) | Zero Hour's units and pathfinder ask the core's lock-free mirror (host patch 0024, "Smoke screens in Zero Hour's targeting and pathing"); the cloud is the converted BO2 effect, an `AREA_EFFECT` FXList played at the blast. |
| Graphics detail (the static level, the particle cap, shadows, the dynamic LOD) | Zero Hour's `GameLODManager`, per machine | Presentation only and never a sim input: each machine picks its own level (host patch 0025 recommends High for hardware the 2002 presets cannot classify, CONFIG.md "Graphics detail"), and the dynamic level follows its own frame rate. Its two logic-side fields (`DebrisSkipMask`, `SlowDeathScale`) are pinned to no-ops by the data tests (`zhbo_assets::detail`). Not in the content CRC. |

## Units

* World: Zero Hour units. 1 ZH unit = 6 BO2 inches (ZH infantry `GeometryHeight = 12` ≙ a 72-inch BO2 player).
  All BO2 distances/speeds go through `units::inches`.
* The factor is defined once in `zhbo_native::ZH_PER_INCH`; `units::ZH_PER_INCH` and the art's
  `ART_SCALE` (`zhbo_assets::character::SCALE`, the native viewmodel) read it. One scale serves both,
  so a converted body's eye and the sim's first-person eye are the same world point (third person, R7).
  Stock ZH infantry mesh heads are taller than their collision cylinder (14.3–16 units vs
  `GeometryHeight` 12–13); the sim's 12-unit, 72-inch body is the reference.
* The General is drawn `GENERAL_SCALE` (1.15) over that (his INI `Scale`, as tall as Zero Hour's
  infantry), and so is everything BO2 sizes against its player: the weapon on him, and every prop
  the converter writes (`ZHBO_BO2_Projectiles.ini`'s objects carry `Scale = GENERAL_SCALE`: grenades
  and rounds in flight, placed equipment, the stuck bolt, the planted shield). In the sim, a length
  on a drawn model — the body's hit boxes, the shield's held, stowed and deployed boxes and where
  they stand — is `units::drawn_inches` (`DRAWN_ZH_PER_INCH` = `ZH_PER_INCH × GENERAL_SCALE`), so
  what blocks is where it is drawn (the eye is the drawn body's too); gameplay distances (speeds,
  ranges, radii) stay `inches`.
* Damage: BO2 damage (100 HP scale) × `BO2_TO_ZH_DAMAGE` (1.5; a Ranger has 180 HP), for every
  target. Every hit carries **BO2's means of death** (`zhbo_core::damage::MeansOfDeath`, t6mp's
  `meansOfDeath_t` in its order, `FUN_00678440`), chosen where BO2's engine chooses it: a round
  `MOD_RIFLE_BULLET` (`MOD_PISTOL_BULLET` from a pistol, `MOD_HEAD_SHOT` on a soldier's head or
  helmet, `isheadshot`); a blow `MOD_MELEE`; a missile's strike (`FUN_00608c10`) `MOD_IMPACT` when
  it is a dud or its def does not go off on impact (a frag or Semtex striking, the crossbow's bolt,
  a ballistic knife, a combat axe), else its missile handler's direct means (`MOD_GRENADE` for a
  `weapType` grenade, `FUN_00607730`; `MOD_PROJECTILE` for anything launched, `FUN_0060d330`; the
  handler table at `0x00c8c0e4`); radius damage the handler's splash means (`MOD_GRENADE_SPLASH`,
  `MOD_PROJECTILE_SPLASH`, `FUN_00604240`). `LogicHost::damage` takes the blow
  (`damage::Blow`: the means of death, the weapon that dealt it and the way it travelled, BO2's
  `G_Damage` `mod`/`weapon`/`vDir`: a round along its trace, a blow the way the striker faces, a
  missile along its flight, radius damage from the blast to the victim; the weapon and direction are
  only the victim's death animation's, `crate::death`); the FFI
  host sends `LogicApi.damage` the **Zero Hour damage type** the tuning pack's `[MeansOfDeath]`
  table names for it (`MOD_IMPACT = EXPLOSION`; the kind is an ABI code, `ZHBO_DAMAGE_*`, one per
  Zero Hour `DamageType` by name, `damage::ZH_DAMAGE_TYPES`; the bridge's `s_zhboDamageTypes`
  maps it), and Zero Hour's armour scales it by that type's row. The shipped table: bullets
  `SMALL_ARMS` (BO2's vehicles cut bullets — the AI tank ×0.3, the turrets'
  `bulletdamagereduction` — as tank armour's 25 % does; soldiers take them whole), grenades,
  rockets, blasts and **impacts** `EXPLOSION` (every vehicle, aircraft and structure armour takes it
  at 70–130 %: an impact has no rule in `callback_vehicledamage` and falls to every killstreak's
  full-damage branch, where `SMALL_ARMS` cut a dud or a thrown axe on a tank to a quarter), blows
  `MELEE` (BO2's turrets ignore them, `_turret_killstreak.gsc:907`; Zero Hour's vehicles and
  structures take 0 %), the rest by their namesakes (`CRUSH`, `FALLING`, `FLAME`, `WATER`, `POISON`,
  `UNRESISTABLE` for the script's kills). The table is sim data: it is in the content fingerprint's
  tuning part. BO2's two kinds of victim map onto Zero Hour's objects:
  a **soldier** (a General, Zero Hour infantry) is BO2's player and takes BO2's numbers as they are
  (his perks by the means of death: Flak Jacket only `isexplosivedamage`'s — the splash means and
  `MOD_GRENADE`, never a rocket's direct hit nor an impact —; the blast shellshock
  `_shellshock.gsc:19`'s causes); **anything else** (a vehicle, an aircraft, a structure) is BO2's
  vehicle/killstreak gear and takes `_globallogic_vehicle.gsc`'s rules by the means of death
  (`damage::vehicle_scale`, every scalar a tuning-pack key by weapon name): radius damage ×
  `getvehicleprojectilesplashscalar` (`callback_vehicleradiusdamage`; 1 for every player weapon),
  then `MOD_PROJECTILE`/`MOD_GRENADE` × `getvehicleprojectilescalar` (SMAW and FHJ-18 0.2),
  `MOD_GRENADE_SPLASH` × `getvehicleunderneathsplashscalar` (**C4's blast ×30** on any vehicle it
  reaches: the radius damage reaches `callback_vehicledamage` through `finishvehicleradiusdamage`,
  t6mp `FUN_0066a3e0` → `G_Damage` → `FUN_005f0810`), everything × `vehicledamagescalar` (1). A
  sticky def (`stickiness` all/all_orient) stuck to a vehicle strikes it for its
  `iExplosionInnerDamage` by its direct means and spares it the radius (`FUN_0060a480`): C4 stuck
  to a tank deals its 200 by `MOD_GRENADE`, not ×30. Killstreak-specific rules (the AI tank's
  hatchet-as-bullet, the helicopters' rocket one-shots) belong to BO2 entities with their own
  damage pools that Zero Hour's objects do not have: Zero Hour's armour rows stand in for them.
  A SMAW into a Crusader group (480 HP): the struck tank
  300, each neighbour 105–225 by the gap between the hulls, so two rockets wreck the group; into
  infantry: 1500 on the one struck, the blast kills a Ranger within three-eighths of the radius (BO2's falloff: 225 at the solid).
* A projectile in flight strikes an object by its **solid** too (`projectiles::struck_solid`): a
  soldier's thin cylinder, anything else its Zero Hour box, cylinder or sphere
  (`hitscan::solid`), grown by half the projectile's radius, so a rocket passing a tank's side
  strikes it where its side is, not within a circle of its length. A knife reaches a vehicle when
  its footprint (the same solid) is within `player_meleeRange`.
* Radius damage measures each object to its **solid** (`projectiles::blast_distance`: its Zero Hour
  geometry box/cylinder/sphere, BO2's soldier-sized cylinder for infantry, `hitscan::solid`), as
  BO2's measures each entity to its bounds (t6mp `FUN_005f1fd0`, from `FUN_005f2130`), and falls
  off from the inner damage at the solid to the outer at the radius. A planted Assault Shield is
  measured the same way, to its deployed box where it is drawn (`shield::Bearer::distance`, at
  `DRAWN_ZH_PER_INCH`), not to its tag. A concussion's stun time runs
  over the distance to the soldier's origin, as `_weapons.gsc:1366` measures it.
* The object query (`LogicHost::query`, the bridge's `cbQuery`) returns every object whose
  **bounding sphere** comes within the radius (Zero Hour's `FROM_BOUNDINGSPHERE_3D`, the measure its
  own radius damage uses; `ObjInfo::in_query` is the same test for the test hosts): a superset, and
  each caller measures what it needs (a blast the solid, `ObjInfo::origin_within` where BO2 measures
  an origin: the Trophy System's range, the Black Hat's, the sonar and target finder). It was
  centre-to-centre, which never listed a structure or an Overlord whose hull was in a blast but whose
  centre was not.
* Angles: radians. Yaw 0 = +X, counter-clockwise (same as `Object::getOrientation`). Pitch + is up,
  clamped to ±85°.
* Time: logic runs at 30 Hz (`LOGIC_DT`). The client runs per rendered frame with real `dt`. The
  first-person eye and view kick are drawn between two logic frames at the render clock (the time
  since the frame's step, carried across steps so a render rate that does not divide 30 Hz stays
  even): the predicted record's last two frames while the client predicts (`client::predict`), the
  sim record's last two otherwise. The render rate is independent of the logic's (host patch 0026,
  CONFIG.md "The render rate"): by default the display's refresh rate, with the logic held at the
  game's speed by the engine's logic time scale; Zero Hour's drawables are drawn between their last
  two logic transforms (`Drawable::getRenderTransformMatrix`) and W3D animation advances every render
  update. None of it reaches the sim: the core sends one input per logic frame whatever the render rate.

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

## Multiplayer (lockstep)

Every peer of a Zero Hour network game (and a replay's playback) runs the same logic frames over
the same command lists, so the core's sim must reach the same state on every machine from the
replicated messages alone. The rules, and what enforces each:

* **Only the messages feed the sim.** `MSG_ZHBO_TAKE_CONTROL`, `MSG_ZHBO_FPS_INPUT` and
  `MSG_ZHBO_CHOOSE_CLASS` (a custom class as its catalog-token wire words) are the sim's inputs;
  the logic callbacks read only logic-side engine state. The local client (its camera, look, HUD,
  sounds, shellshock presentation, viewer) never writes the sim's deterministic state: the look
  and the shellshock's look scaling are applied before the input is *sent*, so the angles in the
  message are what every peer uses.
* **The input carries the frame its sender saw.** `MSG_ZHBO_FPS_INPUT` is `(objectID, held,
  pressed, move, yaw, pitch, seen_frame)`: the seventh argument is the core's sim frame on the
  sender's screen as the client made the input (`ZhboClientOut.input_seen_frame`), handed to
  `zhbo_logic_fps_input` (a six-argument message, from an older replay, goes to `zhbo_logic_input`:
  seen frame 0). The record keeps the latest (`PlayerSim::seen_frame`, hashed with it). Several
  inputs on one frame (the run-ahead shrank): the last one's keys, move, look and seen frame, every
  one's presses; a frame with none (it grew): the last one holds.
* **Lag-compensated hitscan (BO2's antilag).** Every step ends by keeping each first-person
  General's position, yaw and body state and the Assault Shields (`sim/antilag.rs`, the last
  `AntilagMaxRewind` frames: the tuning pack's 400 ms, t6mp `FUN_0066b6e0`'s cap, 12 frames). A
  bullet (`fire_round`, through penetration and the shield check) is traced against the frame its
  input was seen at, no older than the cap; seen now or not reported: as things stand. Only the
  sim's bodies are rewound, as BO2 rewinds only players: Zero Hour's units and an overhead General
  stand where they are. Melee, projectiles and equipment are not rewound. The history is a pure
  function of the stepped state, so `state_hash` leaves it out.
* **Assists are sim state (`sim/score.rs`).** BO2's assist rule (`_globallogic_player.gsc`
  `trackattackerdamage` / `playerkilled_awardassists`, `_globallogic_score.gsc` `processassist`):
  every `LogicHost::damage` call a step (or `rts_update`) makes is recorded by a pass-through host
  (`score::Tally`), in call order; an enemy seat's damage to a seat's General joins his attacker
  list (BO2 health, decayed by his regeneration, `decayplayerdamages`), and a seat's killing blow
  credits every other attacker on the killer's side and not the victim's (Zero Hour's alliance:
  a free-for-all credits none) an `Assist { level }` event (`assist_25/50/75`) and one assist on
  `Score::assists`. The step's blows are scored after every seat has stepped, and a dead General's
  record ends in his own seat's step, so the victim's seat is found among each seat's last
  Generals and his attacker list ends only after the scoring (`Sim::step` forgets the Generals
  `step_frame` reports dead): a kill scores whichever of the two seats steps first. The lists, the counts and each seat's last Generals are hashed (the world part).
  The scoreboard's ASSISTS is that count; the client adds the points (`client::ASSIST_POINTS`,
  `mp/scoreinfo.csv`'s tdm column) to the SCORE column. A concussion, flashbang, EMP grenade or
  Shock Charge that reached a first-person General (the sim's shock hits name the weapon and its
  owner) is that attacker's last hit (`score::Tactical`, BO2's 1-damage tactical blast), and his
  assist is then `assist_concussion`/`_flash`/`_emp`/`_proximity` (50); a later plain hit clears it.
  The recording host takes damage-rules' `MeansOfDeath`.
* **The local player's prediction is presentation.** The client (`client/predict.rs`) predicts its
  own first-person General over the inputs in flight by running the sim's own step for him
  (`Sim::predict_step`) on a copy of the sim (`Sim::scratch`) through `sim::predict::Sandbox`,
  which forwards the engine's reads (`object`, `query`, terrain, build options, work sites) and
  drops every write (no pose, control, damage, heal, spawn, destroy, FX, capture, place, work, log;
  `damage` answers no kill). It runs in `zhbo_client_frame` (the bridge's render-time call into the
  core, where the logic's reads are valid and unchanging) and its copy is thrown away; its events
  are presented once (the weapon's: shot, recoil, impacts, hit marker, reload, swap, alt, melee,
  offhand, launch, hammer, the plant hint) and the sim's own of those kinds are matched against
  them when the record takes the input in (a predicted kill is confirmed by the sim's marker). The
  own gun's tracers come from the prediction (`ViewTracers::predicted`). The lockstep gate runs
  with predicting clients.
* **`Sim::events` and `Sim::tracers` are presentation, never sim input.** Each client drains the
  events at its own render cadence, a headless replay check never does (the buffer then sits at its
  512 cap and evicts from the front), and the tracers are drawn per viewer. A sim decision taken
  from either diverges between peers; `hasdonecombat` comes from the emit itself
  (`Sim::combat_seen`), not from an index window into the events.
* **Bit-identical floats on every platform.** `+ - * /` and `sqrt` are IEEE-exact, and neither
  Rust nor the engine (`-ffp-contract=off`, MSVC `/fp:precise`) fuses a multiply and an add. The
  transcendental functions are the gap: `f32::sin` and its kin call the platform C library, whose
  last-place results differ across macOS, Windows (whose UCRT picks code paths by CPU feature) and
  Linux. The engine routes its own through TheSuperHackers' GameMath (`USE_DETERMINISTIC_MATH`);
  the core routes the sim's through `math::det` (musl's libm, ported to Rust). The
  `the_sim_calls_no_platform_libm` test fails on any `f32` transcendental call in a sim-reachable
  file (everything except the client, HUD, class screens, LUI, renderer passes, conformance oracle,
  sounds and the C ABI module; the class rules in `classmenu/` are scanned, since the sim repairs
  received classes with them).
* **A received class is never trusted.** The pick's words are whatever the sender's machine put
  there (a modded client, a hand-edited profile, noise). `ChosenClass::decode` runs the editor's
  own repair pass on every machine before the class is worn (`classrepair::repair_received`: each
  slot's items and their statstable slot, attachments and their compatibility, the grenade counts,
  the perk tiers, Pick 10 and the bonus-card cap, the spare-round cap), as BO2's `giveloadout`
  reads each slot through the engine's checked loadout getters; words that do not decode drop the
  pick. It leaves out the two rules that read the receiving machine's own install: the DLC locks
  (`CacLocked`; BO2 checks DLC per owner, `Engine.HasDLCForItem` on the local controller) and the
  camo lists (`CamoList`, built from the art this machine converted). Those are the picker's
  (`classrepair::repair`, the profile and the editor), so peers with different DLC mend a class
  identically (`hostile_class_payloads_are_mended_alike_on_every_machine`,
  `a_received_class_ignores_the_receivers_dlc`). The fields the received pass does read (the items'
  `CacSlot`, `Allowed` attachments and costs, the perk rows, the class caps) are sim inputs:
  `Role::Sim` in the weapon table, so in the content fingerprint, and the content scan
  (`the_sim_reads_no_presentation_field`) covers `classrepair.rs` and the editor rules it applies
  (`classmenu/items.rs`, `legal.rs`, `draft.rs`, `wildcard.rs`); the picker-only reads there are
  marked `// content: presentation (...)`.
* **A seat commands only its own General.** Ownership is checked at take-control and on every
  step: a network game hands a leaving player's assets to a living ally (`onSelfDestruct` with
  `transferAssets`), and the leaver's record then lets go of the General (back to RTS control, AI
  on) instead of driving him with the last input it received.
* **The same sim content on every machine, checked in the lobby.** Each player converts the
  weapon table (and keeps the tuning pack and the classes) on his own machine. What reaches the sim
  must match; what does not (the install's language, its DLC packs, the art a pack adds) must not
  keep players apart. The core's content fingerprint (`content.rs`, `zhbo_content_crc`) hashes the
  parsed data in parts: Zero Hour's own INI CRC (the engine's INIs and the converted archive's
  Object/Weapon/FXList files), the weapon table's **sim fields** (`Role::Sim` in the table schema,
  `weapons/table.rs`; the presentation fields — localized names and descriptions, sound and FX
  aliases, viewmodel indices and anims, the Create-a-Class lock, a weapon's camo list, the stat
  bars — are `Role::Presentation` and left out), the class-pick vocabulary (the weapon ids in
  catalog order and the camo/reticle/colour/perk ids `classwire` indexes), the tuning pack's values,
  the class list, `ZHBO_LOADOUT`/`ZHBO_AUTOPILOT`, and the mod build (ABI version and field counts,
  core version, the table schema). `Zhbo_Init` makes the total `m_iniCRC` (`ConquerOps.log`:
  `multiplayer: content CRC … (engine INI …, weapon table …, …)`). Online staging rooms refuse a
  mismatched INI CRC and the join-failed box shows both totals; host patch 0018 makes Zero Hour's
  LAN join check it (the exe CRC stays unchecked there, as in stock Zero Hour), and host patch 0021
  carries the parts in the LAN join request and its refusal (`GameToJoin`/`GameNotJoined`
  `zhboContent`, inside the union's existing size), so the refused player's box and the host's
  chat name the parts that differ (`zhbo_content_mismatch`). Each game setup menu (LAN, online)
  prints the short fingerprint and the install's DLC packs in its chat (`zhbo_content_label`).
  Replays record the total in their header. `the_sim_reads_no_presentation_field` scans the
  sim-reachable sources for the members a presentation field sets; a line in a mixed file that
  reads one for presentation says so (`// content: presentation (<why>)`).
* **DLC is the owner's, as in BO2.** BO2 asks the local controller whether it owns an item's DLC
  (`Engine.HasDLCForItem`, `changeclass.lua:214`, `cacweaponbutton.lua:540`) and gates only map
  packs by the whole party (`partylobby.lua:870-934`). The Peacekeeper's definition ships in
  `common_patch_mp` (every final-patch install has it), so the mod's sim content is the same with or
  without Revolution: the picker's own install decides the lock (`CacLocked`, presentation), and a
  peer without the pack runs the identical sim for it. Measured: archives converted with and
  without the `dlc0` zones, with a Revolution `dlc1_load_mp.ff` present, and without the english
  zones all fingerprint the same (MODLOG 2026-11-01).
* **No logic reads a UI-only override.** The General's Build menu and class set are command-set
  overrides the bridge puts on the object on the machine whose player opened them. Zero Hour's
  logic reads an object's command set in its construction check (`BuildAssistant::isPossibleToMakeUnit`,
  "run this logic on all their machines"), so host patch 0019 checks a General against his side's
  build set (`Zhbo_BuildCommandSet`) on every machine instead of his current set.
* **A machine that only watches sends nothing.** An observer seat, a defeated player who stays
  to watch and a replay's playback (`ZhboClientIn.spectating`: `!ThePlayerList->getLocalPlayer()
  ->isPlayerActive()` or `TheRecorder->isPlaybackMode()`) run a spectating client
  (`client/spectate.rs`): no `MSG_ZHBO_FPS_INPUT`, no class pick, no `select_object` group
  message, no mouse capture, no class screen. `ZhboClientIn.local_player` stays the engine's
  observed-or-local seat; the core chooses whom to follow from it (the engine's observed seat is
  adopted when it changes) and from the spectator's own buttons (BO2's dead-spectate bar: attack
  the previous first-person General, aim the next; the middle button or the field-computer key
  the overhead view and back), so following works without Zero Hour's player-observer option.
  Following, the camera is the General's eye at the sim's own aim — the input angles every peer
  stepped on plus the sim's view kick, interpolated across the logic frame like the eye — and
  the viewmodel, sounds and impacts are his; his death banner, hit markers, score popup, hints and
  work bars are not shown. The followed General dying or leaving first person cuts to the next
  one, or flies out overhead when none is left. The HUD is his (`zhbo_client_draw_hud` draws the
  followed seat), reduced to what BO2's spectator sees, with BO2's dead-spectate panel naming him
  (`ZhboClientApi.player_name`). `Client::viewed` is the seat a frame presented; the native
  viewmodel's class art reads it. In a replay the autopilot's test scene is built for the first
  human seat (the recording's local player in a skirmish) and its take-controls are not re-sent.
* **The fingerprint.** `Sim::state_hash` (`sim/lockstep.rs`) hashes the deterministic state (the
  player records with each input's seen frame, chosen classes, projectiles, equipment that is
  down, smokes, deployed shields, dropped weapons and the kills not yet read by a death, the antilag history of the last frames' first-person Generals
  and shields, the frame) and leaves out the presentation above, the step's scratch copies (the
  frame's shield bearers and guards, the shock hits) and the installed data the content CRC covers
  (tuning, the antilag rewind cap, penetration, hit bodies). The history is in the world's part
  but not in the idle test (every step records a frame, an empty one with nobody in first
  person). It streams the values field by field (`statehash.rs`: every type names
  every field, floats by their bits, a weapon or shock definition by its id; no addresses, no map
  order), about 11 µs for sixteen busy seats in a release build. `Sim::fingerprint` also gives
  each seat's part (its record and chosen class) and the world's. The multiplayer gate
  (`sim/lockstep_tests.rs`, TESTING.md) runs one match on four machines — two players' machines
  at different render cadences, each with its own client and viewer, one that never renders, and
  a spectating observer that follows the Generals, switches and goes overhead and must send
  nothing — and requires the same hash, and the same engine calls, on every frame; the full-roster gate
  (zhbo-assets, `the_install_roster_plays_in_lockstep`) runs the same harness (`sim/harness.rs`)
  over the player's own install's every weapon, alt mode, attachment and offhand on three players'
  machines, an observer's that never renders and a spectating observer's.
* **The fingerprint in Zero Hour's CRC.** Host patch 0020 calls `Zhbo_XferLogicCRC` in
  `GameLogic::getCRC` right after `MARKER:TheAI` (the logic frame's own CRC at the end of
  `GameLogic::update`, after `Zhbo_LogicStep`, so every machine and a replay hash the same
  post-step state). The bridge asks `zhbo_logic_crc` and, when the sim holds state, xfers
  `MARKER:ConquerOps` and the 64-bit hash (two `UnsignedInt`s) into the CRC. An idle sim — no seat
  holds a record or a class since the last `Zhbo_Reset` (no General deployed: the overhead
  General's RTS report is a seat record too; no class picked) and nothing in the world — adds
  nothing, so such a game
  CRCs exactly as stock Zero Hour does. A mod-only divergence (ammo, a class, a shellshock, a
  cooldown) is therefore Zero Hour's desync on the next CRC frame (`NetCRCInterval`, 100 frames in
  release builds): in a network game `Network::setSawCRCMismatch` opens `Menus/CRCMismatch.wnd`,
  starts the end-game timer and marks the replay as desynced; in replay playback the recorder
  shows "GUI:CRCMismatch" with the frame and pauses (`-headless -replay` prints `CRC Mismatch in
  Frame N` and exits non-zero). To localize one, each machine's `ConquerOps.log` has a line per CRC
  frame while the sim is active, `crc frame F: <hash> world <hash> seats <seat>:<hash> …`, and
  `DEBUG_CRC` builds log the same parts in the engine's CRC log (`CRCGEN_LOG`, dumped on a
  mismatch): diff the two machines' lines and the first differing seat (or the world) is where the
  state went apart.
  In game, `ZHBO_STATE_HASH_LOG=1` logs the fingerprint after every step (`state hash: frame N
  <hash> world <hash> seats <seat>:<hash> …`, the CRC frame line's parts): the two-instance test
  (`scripts/mptest-mac.sh`) and a two-machine session compare the machines' lines, and the bridge
  logs Zero Hour's own mismatch (`TheNetwork->sawCRCMismatch`, otherwise only on screen in a
  release build).
* **Third person is per viewer.** Each machine draws every General's native third-person weapon
  (`zhbo_client_native_poses`) except the one its own client looks through in first person (his
  body is hidden; the viewmodel draws his arms). The other players' Generals are in first person
  on their own machines, and here they show their class weapon, attachments and camo.

## The shell menu ABI (the lobby's create-a-class)

The lobbies' Create a Class button opens the class screens *in the shell* (BO2's lobby
create-a-class: edit/manage classes outside a match). Every game setup menu has it — Skirmish, LAN
and online (GameSpy) — built in code by the bridge (`Zhbo_EnsureCreateClassButton`, host patch 0018;
no `.wnd` is shipped) in the empty top-right corner of the menu's panel, fitted above its title
line. The window is the engine's — a full-screen modal window built
in code by the bridge (`host/zhbo/ZhboMenu.cpp`, on the engine's own modal-dialog pattern; host patch
0015 retargets the button to it) — and the screen is the same `classui` screens the in-game `Client`
hosts, driven by a small `MenuState` (`crates/zhbo-core/src/menu.rs`) with no `Sim` and no match.

Ownership and rules:

* **Pure local UI.** The menu never touches the sim: no determinism or lockstep requirements, no
  `GameMessage`s. Selecting a row *opens the editor* (in-game it would pick — there is no match);
  the profile file (`ConquerOps.profile`, written by the bridge on the `save_profile` one-shot,
  exactly as in-game) is its only output. The one-shot carries the file's text when it fits the
  1,024-word payload; a larger profile (ten sets of personalised classes pass 4 KiB) sends
  `save_profile_payload_len = -1` (`profile::PROFILE_LIVE`), and `zhbo_profile_write` then serves
  the core's own, already-applied profile — a write is never truncated or dropped.
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
* **No first-person play without a class** (BO2: `_globallogic_ui.gsc`'s `menuclass` is the only
  path from the class menu to `spawnclient`; `_menus.gsc`'s `back` just closes the menu and leaves
  the player unspawned). A General taken into first person with no class is *awaiting a class*
  (`PlayerSim::awaiting_class`): the client keeps the spawn's class screen up and draws no
  viewmodel, and the sim ignores all of his first-person input except `buttons::FIELD_COMPUTER`,
  which hands him straight back to RTS control (`ReturnedToRts`, no field-computer tablet). Escape
  on that screen is how the client sends it (`Client::class_pick_declined`: the screen shuts, the
  view holds still and only the press goes out until the sim answers). Back overhead (from here, or after the field computer) he is
  selected again (`ClientOut.select_object`: the core sets it on the frame the leaving camera lands, for a General still alive, and the bridge selects him as a click would and logs `selected General <n> back overhead`), so the command bar's class set is up as his way
  back in; the next Take Control puts the screen up again. Take Control on a General with no class,
  and a re-entry after a declined spawn, both go through this. A class picked overhead first
  spawns him straight into play. Escape on a classed General's forced re-open keeps playing.
* **The class is the player's, the kit is the General's** (`sim/spawn.rs`). BO2 keeps
  `pers["class"]`, the class the player chose, which survives his deaths, apart from `curclass`,
  what this life was given. Here `Sim::chosen` (one per player seat, `ChosenClass`: id, name, kit,
  the custom's per-slot options) is the first and `PlayerSim::class` (the record, which dies with
  the General) the second. Every record is made by `Sim::new_record` wearing the chosen class, in
  `take_control`, `rts_update` and `choose_class` alike, so after a death the next General comes
  in armed, overhead and in first person, with no class screen. `zhbo_general_has_class` and the
  command bar's class set still read the record, which now carries the class from its first frame.
* **Mid-life picks follow `menuclass`.** A pick always becomes the chosen class (the same class
  again does nothing). A General with no class wears it at once. A classed General wears it at once
  only inside the grace period (the tuning pack's `ClassChangeGracePeriod`, seconds of sim frames
  from the match start) and before his first combat (`PlayerSim::combat`, BO2's `hasdonecombat`:
  a shot, a throw or plant, a launch, or damage taken, in first person or overhead, reset at each
  spawn). Otherwise the kit stays, `ClassAtNextSpawn` shows BO2's `MP_CHANGE_CLASS_NEXT_SPAWN` hint
  (a `UI_TEXT_KEYS` string), and his next General comes in with it: the seat's `Sim::class_waits`
  (hashed with the seat) marks the class as waiting, and the record made for the next General
  (`Sim::fresh_record`, in `take_control` and `rts_update`) emits `ClassChosen` as it wears it, so
  both machines log "assumed the ... class" for him. Picking back the class he wears leaves nothing
  waiting.
* **The spawn is the first time in first person with a class** (`Sim::spawn_in`, run by
  `take_control` and at the top of each `step`): a Take Control on a classed General, or the pick
  that ends his wait behind the screen. That is when the player's Tactical Insertion is used (he is
  moved onto it) and his combat starts over. A take-control he declines without a class leaves the
  insertion down. Later take-controls of the same General are the same life.

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
  eject direction, so its converted FXList sets `OrientToObject = Yes` and keeps BO2's axes, as
  do the muzzle flashes (`ViewFlashFx`, the silencer pair, the world flash: played on the flash
  bone's matrix, X down the barrel) and a bullet's impact cells (played on `zhboFrameAt`'s surface
  frame, X along the normal): `fx::Converted::add_framed`, FXList `<name>_Framed`, offsets and
  velocities in BO2's axes. Everything played with no matrix (a blast through the `fx` callback, a
  bounce, a dud, the General's impacts) is the world copy, BO2's X (the normal) swapped to up;
  `vViewShellEjectOffset`/`Rotation` are not applied.
  `LOADING` (2) and `NOT_NATIVE` (0): the W3D path draws this frame. Viewmodel sound notetracks
  are unchanged (the bridge's `playNotes` runs before either path).
  - **Effects draw through the engine's `FXList` path, not the native pass.** The engine already
    owns particle systems, the blend modes from the material state bits, lifetimes and the world
    FX weapon pipeline; the native pass has no particle system and would duplicate all of it.
    Shell model elements are drawn as small sprites from the model's own colour map, because
    Zero Hour's FXList has no model nugget. A converted system's `Priority` follows its role
    (`fx::Role`, as Zero Hour prioritises its own effects of that kind): blasts, impacts and
    muzzle flashes `CRITICAL`, casings and bounce dust `WEAPON_EXPLOSION`, the smoke screen
    `AREA_EFFECT`, a projectile's trail `WEAPON_TRAIL`. The lowest priority is culled by the
    dynamic LOD below 25 fps and can evict nothing at the particle cap.
  - **Particle fields host patch 0028 adds** (`ParticleSystemInfo`, parsed from the converted
    `ParticleSystem` blocks; presentation only, never xfered — a loaded system takes them from its
    template — and never read by the sim):
    * flipbooks: `FrameGrid` (cells per side of a square grid, a power of two up to 16; 1 = a plain
      texture), `FrameCount`, `FrameStartType` (`FIXED`/`RANDOM`/`INDEXED`) and `FrameStart`,
      `FrameRate` (cells a second), `FrameOverLife` (every cell once over the particle's life) and
      `FrameLoops` (0 forever; n holds the last cell after n passes). The converter lays BO2's
      atlas out again on that grid, row by row from the top left (`fx::flipbook_grid`, texture
      `zhbo_fx_<image>_f<frames>.tga`); `Particle::getFrame` picks the cell from the updates the
      particle has had (`m_lifetime − m_lifetimeLeft`, so it advances at the logic rate) and a
      random start from a hash of its personality (no random stream is drawn), and
      `W3DParticleSystemManager` hands the cells to W3D's point group (`Set_Arrays`' frame array,
      `Set_Frame_Row_Column_Count_Log2`), which writes the UVs on the CPU for both the wgpu and
      the DX8 backend. A batch never mixes grids.
    * distance ranges from the camera (world units): `SpawnRange = min max` (no particle is emitted
      while the system is outside it; BO2's `spawnRange`), `FarFade = start end` (fully drawn nearer
      than start, gone past end; BO2's `fadeInRange`) and `NearFade = start end` (gone nearer than
      start, fully drawn past end; BO2's `fadeOutRange`). The eye is the scene camera of the last
      drawn frame (`ParticleSystemManager::setCameraPosition`, set by the renderer); an additive
      particle darkens, the others fade.
  - **Other nuggets the converter writes:** a blast's decal element (`Role::Blast`, upright) is a
    `TerrainScorch` of BO2's radius (Zero Hour's own scorch marks; a hit's decal is not, as Zero
    Hour marks only terrain); an omni light is a `LightPulse` whose `DecreaseTime` is BO2's life in
    **milliseconds** (Zero Hour parses the times as durations). A projectile's `projTrailEffect` is
    converted framed in `Role::Trail` and its systems are attached to the projectile object's model
    (`ParticleSysBone = None <system>` in `ZHBO_BO2_Projectiles.ini`: no bone of that name, so the
    object's origin), so they ride it and leave their particles behind. BO2's trail (ribbon)
    elements are left out there: their art is a ribbon strip, which as sprites tiles the view with
    streaks and as a Zero Hour `STREAK` crosses the shooter's eye in screen-wide polygons.
  - `zhbo_viewmodel_fx` is a free function on the append-only ABI surface (like
    `zhbo_loadout_viewmodels`).
* **The torso layer (body mode).** Over the movement clip the upper body plays the General's
  action: BO2's `mp/playeranim.script` `EVENTS` torso clip for his reload, knife (`knife_melee`,
  `knife_melee_charge`; the shield's `meleeattack`), throw and pin (the offhand's `fireweapon`,
  `prime_grenade`), shot and weapon change (`raiseweapon`, `dropweapon`, `firstraiseweapon`), per
  stance, gait and aim. The converter groups the weapons into torso sets (`WeaponDef::torso_anims`,
  `TorsoAnims`), puts every torso clip in the faction body manifest at id `torso::BASE` (0x1000) +
  its index, and writes the table into `sound_layers.txt` (see "Other players' actions" below). The
  client times each General's action (`client::remote`) and hands it to the native pass through a
  lock-free per-General mirror (`nativebody::ffi::publish_torso`, read by `torso_of`), as the
  stowed weapon is: no ABI. The pass loads a torso clip the first time anyone plays it (shared by
  every rig), samples it over the bones it tracks (a `pt_*` clip tracks the lower spine and up) and
  fades it in and out over `nativebody::TORSO_BLEND`. The weapon-only mode stays rigid; the W3D
  body keeps its per-set fire and reload.
* **The offhand in the hand and the corpse (body mode).** The class's lethal and tactical world
  manifests (`ZW<vm>.zbw`, every offhand is converted) are composed on the body's `tag_inhand`
  (`nativebody::OFFHAND_TAG`, published per General like the stowed weapon:
  `nativebody::ffi::publish_offhands`) and drawn only while that one is primed or thrown, the gun
  then hidden (t6mp `FUN_0052a470`). A General who dies stays drawn: `client::remote` keeps a
  corpse (`Corpse`, for `CORPSE_TIME`) whose pose `zhbo_client_native_poses` appends, playing the
  script's `DEATH` clip in the context BO2's engine sets (`crate::death`: his stance, `movestatus`
  by speed, `direction` by his movement keys, the killing blow's `dmgType` by its means and weapon
  — `player_die`'s rule, so a shotgun's kill is `normal_shotgun` and the shield's bash `meleebash` —
  and `dmgDirection` by the blow's direction against his view, `finishplayerdamage`'s sectors;
  `EventKind::KilledBy { dmg, dir }`, `torso::death_context`) whole-body, faded in and held at its end, without the gun (BO2's corpse goes ragdoll part-way;
  Zero Hour has no ragdoll, so the clip's last pose lies until the body is removed). Overhead, the
  rounds Zero Hour spends (`Arsenal::rts_shot`) start the gun's shot torso, client-side.
* **The dual-wield left gun's clip stays in Rust.** A `_dw_mp` weapon fires two guns (one per
  trigger, each with its own magazine), so its left gun plays its own `_LEFT` slot while the hands
  fire, reload or idle apart. The client hands that clip and time to the native pass with
  `nativevm::ffi::set_left_clip` beside `ZhboClientOut.viewmodel`; `ZhboNativeIn` carries only the
  base clip and its size is unchanged. The left clip id is the matching right slot with
  `INSTANCE_LEFT` (`viewmodel::slot::left_as_right`). Each hand's shot has its own fired flag
  (`viewmodel_fired`, `viewmodel_fired_left`; see the effects above).
* **Dropped weapons.** Each frame, after the Generals, the bridge asks `zhbo_client_dropped_weapons`
  and draws each drop through `zhbo_native_body` in `ZHBO_BODY_LYING` mode, keyed by its Zero Hour
  object: the combination's world weapon (`ZW<vm>`: its attachments), the camo it was dropped with
  (`Sim::third_person_camo` finds the drop by its object), at the General's drawn scale, a quarter
  turn about its length onto its side, its lowest posed vertex on the ground at `pos`. While it
  draws, the bridge hides that object's W3D prop (`ZHBO_Drop_<base>`, the base weapon's world model)
  through its draw modules, and shows it again the frame a draw is refused or still loading;
  with `ZHBO_NATIVE_BODY=0` (and on DX8) the prop is what shows. Its optic's lens reticle stays dark,
  as on every BO2 world model (the reticle fade is the local viewmodel's only).
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
  (an aimed burst stays on the up clip's last frame); both clips are requested as soon as the
  viewmodel composes, so a reversal never waits for its clip. **The layer is on at every fraction,
  the hip included** (`ads = 0` samples the up clip's first key, which is the down clip's last):
  BO2 weights 0x56 by `1 − fWeaponPosFrac` and holds it at full weight on a switch (G12,
  `FUN_0052dcb0`), so the hip rig root is the layer's hip key, not the hands' bind `tag_torso`.
  For almost every gun the two agree (`[-4.63, 0, -1.38]`), but the XPR-50 (`as50_mp`) authors its
  hip root 13″ forward in the layer alone (its action clips are relative to it; LSAT 0.8″, the
  minigun 0.3″), so a hip without the layer put the eye inside its receiver. The install-gated
  `every_viewmodel_is_held_in_front_of_the_eye` (`nativevm::hold`) checks every weapon and
  attachment at the hip. The eye-frame slide (`place::aligned`, which moves
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
  are built once at load (with every texture either names, `Loaded::texture_paths`) and the draw
  swaps handles. The W3D path draws the hip models. The list
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
    class primary. The rig's cache identity includes the option. Both native draws hand the
    renderer the same surface (`nativevm::native_surface`/`surface_files`), camo layer included.
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
  (`sw4_3d_player_icon`); RENDERER.md "The lens reticle" has the technique. The local player's
  reticle choices ride a core-global like the camo (`nativevm::set_local_reticle`, set by
  `zhbo_client_frame` from `PlayerSim::held_reticles` through `nativevm::class_reticles`: the
  primary's `PlayerSim.reticle` and the sidearm's `PlayerSim.secondary_reticle` — BO2's
  `primaryreticle`/`secondaryreticle`, `CustomClass.reticle`/`secondary_reticle` — one
  `(viewmodel, row)` per slot weapon and each of its alt modes; `reticle_N` is `weaponoptions.csv` row `N`,
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
  base part's tag overrides the body's `weapon_tag`. The weapon-only mode is placed by the engine's
  `tag_weapon_right`, so it refuses such a weapon (logged once: "held on tag_weapon_left, not the
  right hand ...") and the W3D weapon draws it: the converter skins the shield's `W<vm><k>` meshes
  on the W3D body's `tag_weapon_left` pivot, the bone the manifest names. Carried with
  another weapon up, the body also wears the shield's stowed manifest (`ZS<vm>.zbw`: the stow model
  on `tag_stowed_back`). Which General carries a stowed shield comes from a lock-free mirror the core
  publishes after each logic step (`Sim::stowed_weapons` → `nativebody::ffi::publish_stowed`), read
  by `zhbo_native_body` through the frame's `general`: no ABI field. The W3D body has no stowed
  shield (body mode only).
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
  `Sim::native_poses`: the held weapon's converted index, stance, body clip (the held weapon's
  animation set and movement state, below), clip time and rate, whether the second gun is up;
  first-person Generals are left to the viewmodel), fills one `ZhboNativeBodyIn` per
  General from the drawable's `getTransformMatrix` (feet and yaw), the drawable's
  `getCurrentWorldspaceClientBonePositions("tag_weapon_right")` (the engine's own world transform the
  W3D gun hung from), Zero Hour's global lights and the RTS camera position, and calls
  `zhbo_native_body`. `DRAWN` (1): the bridge hides the W3D `W<vm><k>` sub-objects. Anything else
  (0 not native, 2 still loading) leaves the W3D weapon showing — and the `s_worldLooks` cache
  remembers the native flag, so a failed native draw brings the W3D weapon back instead of leaving
  nothing. `ZHBO_NATIVE_BODY=2` (the default) draws the body and the weapon and hides the W3D body
  (see *The two modes* below); `=1` draws the weapon while the W3D body stays; `=0` opts out. The
  W3D body still draws whenever the native pass does not answer `DRAWN` (no native pass on DX8, a
  rig still loading or refused), while the tablet is up (`native_poses` reports no weapon then) and
  once the General is dead (the sim drops his record, so his W3D death animation plays).
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
  deliberately skipped.
* **Animation sets: the body animates by the weapon in hand.** BO2's `mp/playeranim.script` picks a
  player's body clip by his weapon's `weapClass` and `playerAnimType` (the line of
  `mp/playeranimtypes.txt` the def stores) as much as by stance and gait: a pistol walks
  `pb_combatwalk_forward_loop_pistol`, a launcher `pb_walk_forward_RPG_ads`, the combat knife
  `pb_knife_walk_forward`, the Assault Shield `pb_riotshield_walk_forward`. The converter groups
  every weapon held in hand (BO2's primary, alt-mode and melee inventories) into **animation sets**
  — the weapons the script animates alike in every movement state; set 0 is the rifleman's, which
  also takes every weapon the script gives no clip of its own (the crossbow, shotguns, snipers) —
  and stores each def's set in `WeaponDef::body_anims` (`BodyAnims` in the weapon table; a
  combination and an alt mode keep their own def's). A clip id is `set × 16 + state`
  (`state::id`, at most `state::MAX_SETS` = 9 sets); the body manifest lists set 0's every state
  and another set's only where it differs, and `state::fallbacks` tries each nearer state in the
  set, then the rifleman's same state. `NativePose.state`/`ZhboNativePose.state` and the hit boxes'
  `HitBox` state carry the full id. **The W3D body** (the default third person) plays set `k`'s idle,
  run, aim, fire and reload (`ZHBO_PLR_<IDL|RUN|ADS|FIR|RLD><k>`; the fire and reload are the set's
  plain-`default` weapon's `EVENTS` torso clips over its aim and idle) under the model condition
  `names::ANIM_SET_CONDITIONS[k − 1]` (`RIDER1..RIDER8`, which Zero Hour raises only on vehicles
  with riders): `zhbo_client_world_poses` reports the set (`ZhboWorldPose.anim_set`, which replaced
  the slot-based `sidearm` at the same offset) and `applyWorldPoses` raises that condition and
  drops the others (none while the tablet is up or the General is dead). Either gun slot's
  `FIRING`/`RELOADING`/`BETWEEN_FIRING_SHOTS` plays the set in hand. Offhands are not grouped (the
  W3D throw plays over the held weapon's stance). The bridge hides the whole W3D body in body mode when the native pass
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
* **The surface is sim state, read from logic data only.** Objects are classified from the
  `objflags` the bridge already reports (`INFANTRY → FLESH`, `VEHICLE|AIRCRAFT → METAL`,
  `STRUCTURE → CONCRETE`). The ground is water where the logic's water table covers it
  (`LogicApi.surface_at(x, y)`, `TerrainLogic::isUnderwater`), else the map's tile class there,
  from the core's grid (`zhbo_core::terrain`): at the loading screen (and on the first ground
  query of a map it has not loaded) the bridge reads the map file as the logic does
  (`CachedFileInputStream`, decompressed) and hands it, with Zero Hour's terrain types and each
  one's class classified, to `zhbo_terrain_load`; the grid answers `WorldHeightMap::getTerrainNameAt`'s
  cell (the cell under the point plus the border, its tile over four, the first texture class
  holding it, that name's terrain type). It never reads `TheTerrainVisual`, the client's copy of
  the height map: the surface feeds the sim — a grenade's bounce scales by it, the plant check
  refuses deep water by it, an object's surface sets bullet penetration's depth, and the impact and
  blast cells ride the events — so a peer with other graphics settings, or a `-headless` one,
  must get the same answer. At load the bridge cross-checks the grid against the terrain visual's
  lookup on every fourth cell where one exists (`ConquerOps.log`: `terrain: the grid agrees with
  the terrain visual on N of M sampled cells`); the check decides nothing.
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
  converter at the first frame of every body clip — every animation set's movement states, the
  clip id as above: `HitBox = <state> <loc> <bone> …`),
  standing on the object's position and turned by its yaw. BO2 traces the pose the server animated
  (G_DObjCalcPose, `FUN_00649b50`); a General the sim moves takes the pose of his body state
  (`PlayerSim::body_state` = `nativebody::state_of` over the sim's stance, speed, sprint, ADS and
  the held weapon's animation set — never the render clock), at his drawn `GENERAL_SCALE`; Zero Hour infantry (no BO2
  animation state) stand at 1.0. The state clips are BO2's `mp/playeranim.script` picks for the
  held weapon (`zhbo_assets::playeranim`), the same clips the third-person body draws. Of the
  boxes a ray crosses the highest-ranked location wins, nearest among equals (`bRifleBullet` ranks
  the head first; other bullets rank the body alike). A soldier's damage is multiplied by the
  def's `location_multipliers[location]` (a combination's resolved by `attachments::apply`:
  first mounted attachment unique differing from the base unique, `FUN_008ed780`); other objects
  take it unscaled. Penetration's chord through a soldier is the boxes' (the onward trace skips the
  soldier the round is inside). A table without `HitBox` lines (the built-in one) keeps the
  16-inch cylinder with a head band. Deterministic, sim-only inputs.
* **The client carries at most `ZHBO_CLIENT_HITS` (8) impacts per frame** in `ZhboClientOut.hits`,
  filled by `consume_events` (so the impact sound is the surface's, not a guess): the presented
  seat's own first, then other players' rounds into the slots left; a frame that overflows drops
  the extras (others' first).
* **The bridge plays the name the core resolves.** `zhbo_impact_fx(impact_type, surface, head,
  fatal, …)` (or `zhbo_impact_exit_fx` for a hit with `ZhboClientHit.exit`: the type's exit row,
  BO2's impact flag 4 — rows 1/4/8/10/6, `exit impact`/`exit flesh` lines) looks the cell up in the converted `Data\ConquerOps\impacts.txt` (loaded by
  `zhbo_load_impacts` at init) and the bridge calls `FXList::doFXPos` with a frame whose X is
  `-dir` (the surface's outward normal). A missing/stale archive plays nothing and is counted, never
  a bogus name — the same degrade-don't-panic rule as the renderer's draws.
* **Every player's impacts play on every machine.** BO2 plays an impact for each client whoever
  fired (the event is the world's), so the client presents another player's `Impact` and
  `ShieldHit` exactly as its owner's: the same cell and the same sound at the point
  (`client::events::world_event`). The same goes for every other event that happens at a place —
  `Exploded`, `Bounced`, `Dud`, `Intercepted`, `ShieldDeployed`, `ShieldDestroyed`. Another
  player's `Shot` and `Launched` play his gun's third-person chain at him, his `Thrown`/`Planted`
  the offhand's third-person `fireSound`; the rest of his events are his own first person.
  `present_remote`'s match is exhaustive, so a new event kind has to say who sees it.
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
* **A missile's strike** (t6mp `FUN_00608c10`). Every strike of an object deals the def's `iDamage`
  (`damage[0]`) first, by its means of death (`MOD_IMPACT` for a dud or a def that does not go off on
  impact, else `MOD_PROJECTILE`/`MOD_GRENADE`; on anything but a soldier BO2's vehicle rule for it,
  `getvehicleprojectilescalar` only for the direct means, see Damage above): a rocket's or launcher grenade's direct hit takes only that, because BO2's
  radius damage skips the struck entity; a Semtex or C4 sticking, or a frag or flashbang bouncing
  off a player, strikes for it before it sticks or bounces (`Projectile::struck`, a hit marker). A
  `bolt` explosion (`projExplosion` 9, the crossbow) becomes its `szSpawnedGrenadeWeaponName`
  (the table's `SpawnedGrenade`, `explosive_bolt_mp`) where it struck, stuck to what it hit, and
  that goes off on its own fuse with its own blast (`Outcome::Spawned`; the sim resolves the
  grenade through the catalog). A strike is placed where the move met the object or the ground,
  not at the end of the logic step, and nothing is skipped near the muzzle: only the thrower is
  never struck, as BO2's missile skips its owner. `Sticky` is BO2's `stickiness` all/all_orient
  (not for impact weapons, whose strike is their damage). Sim only; no ABI change.
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

## The shellshock's presentation

Presentation only: nothing here feeds the sim (the shock's state is the sim's, read by the client).

* **Screen layers.** `ffi::zhbo_client_frame` hands the local first-person player's shock screen
  (`shellshock::screen`: white, shot, the blurred shock's `blur_alpha` for this frame's `dt`, the
  shock's serial) to `nativevm::set_local_shock`. The native viewmodel's draw (`zhbo_native_viewmodel`,
  whose `target_view`/`before_view` are the scene and its composite) queues it as the renderer's
  shock layer (`Renderer::shock_submit`, `zhbo_gfx::runtime::ShockDraw`) before anything else it
  does: a flashed shock's first frame (a new serial) is a `Grab`, later frames `Flashed { white,
  shot }`; a blurred shock is `Blurred { alpha }`. It runs after the native draws, before the
  composite, so the HUD stays on top (RENDERER.md "The shellshock layer"). When the draw queued a
  layer (`nativevm::shock_layer_drawn`), the HUD does not draw its whiteout; otherwise (no renderer,
  the DX8 build) the HUD's white rectangle is the fallback. Rust-internal: no zhgfx C ABI change.
* **View kick.** `Client` adds the shock's kick (`shellshock::kick`, yaw/pitch radians) to
  `ZhboClientOut.dir` only; `ClientOut.input` carries the unkicked aim.
* **Ringing.** `shellshock::Presented` keeps the shock's `soundLoop` voice (fade in, hold until
  `soundModEndDelay` past the end, fade out) and queues `(event, gain)` in `Cues::loops`, which the
  FFI layer sends through `ClientApi::sound_loop(event, gain)`: the glue starts the looping 2D
  event (`Control = loop`, `LoopCount = 0`, converted as `ZHBO_BO2_Shock_<alias>`), adjusts its
  volume (`adjustVolumeOfPlayingAudio`, the INI volume × gain) and removes it at gain 0; leaving
  first person stops it. `ZhboClientApi.sound_loop` is appended after `sound_gain`
  (`ZHBO_CLIENT_API_FIELDS` 14 → 15 on both sides). A host without the field plays no loop.

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
* **A deployed shield's object** (attackable: `ActiveBody`, a box of the planted model's collision
  surfaces at the General's drawn scale, the sim's deployed box) takes no
  engine damage (0): the hook queues `(object, source, kind, amount)` in a fixed lock-free ring
  (64), drained into `Sim::engine_damage_to_deployed` before the next step — the script's
  wear-down scales on Zero Hour damage ÷ `BO2_TO_ZH_DAMAGE`.
* `zhbo_logic_shield_damage` is part of ABI 27 (`zhbo_core.h`, see "Versioning").

## Smoke screens in Zero Hour's targeting and pathing

BO2's smoke grenade raises a visibility blocker that its engine's own aiming (turrets, lock-ons)
cannot see through (`crate::smoke` cites the script and t6mp functions); the mod's Zero Hour units
are those, so host patch `0024-smoke-screen.patch` puts the sim's screens into three engine places:

* **Targeting.** `WeaponSet::getAbleToAttackSpecificObject` (after its sanity checks) returns
  `ATTACKRESULT_NOT_POSSIBLE` when `Zhbo_SmokeHides(source, victim)`: the line between their
  centres (feet + half the geometry height) passes within a screen's current radius of its centre
  and they are at least 80 inches apart (`zhbo_logic_smoke_blocks`, BO2's `FUN_006e2f90`). That one
  test is every acquisition filter's (`PartitionFilterPossibleToAttack`, the guard and turret
  scans) and the attack states' continuation condition (`cannotPossiblyAttackObject`, the turret's
  `ATTACK_CONTINUED_TARGET`), so a unit neither picks nor keeps a target the screen hides, forced
  or not; the UI's attack cursor (`ActionManager`) asks the same. A target inside the screen is
  hidden from outside, and sees nothing out (beyond 80 inches).
* **Pathing.** `Pathfinder::examineNeighboringCells` and `findGroundPath` add
  `Zhbo_SmokePathCost(cell)` to a cell's cost while a screen covers its centre (seen from above):
  100, the engine's own penalty for tunnelling through an invalid cell. A cost, not a wall: a path
  goes round when it can, a unit inside walks out, a goal inside is still reached. Three engine
  shortcuts would undo the way round, so each stops at a screen's edge: the A* step's straight run
  at the goal (`examineCellsCallback`, half a step's cost per cell, which never sees the neighbour
  costs), and the straight-line checks a path's smoothing and the walk along it use
  (`linePassableCallback`, `groundPathPassableCallback`: a line that starts outside a screen may
  not enter one; one that starts inside may go anywhere).
* **Repathing.** `AIInternalMoveToState::update` recomputes a move's path once when
  `Zhbo_SmokeCrossesPath(path, m_pathTimestamp)`: a segment of it passes under a screen that went
  off after the path was computed (`zhbo_logic_smoke_crosses`).

The answers come from an atomic mirror of `Sim::smoke_blockers()` (centre, radius now, the frame it
went off) and the tuning pack's always-seen distance, written after each logic step and on
take-control/reset (`ffi::publish_smokes`), never under the state lock — the hooks run inside Zero
Hour's object updates and pathfinder, possibly inside a `zhbo_logic_step` callback. The screens are
sim state in the fingerprint and the engine's call order is the same on every peer, so every
machine's units decide alike and Zero Hour's own CRC stays consistent; nothing new is stored on the
engine side. `ZhboLogicApi.smoke` stays the per-frame notice (the bridge logs it; the smoke autotest
reads it). The three free functions are ABI 31 (see "Versioning").

## Lifecycle

1. `GameEngine::init` end → `Zhbo_Init`: ABI check, **requirements check (both games must be installed,
   otherwise a message box and exit)**, host tables handed to Rust, Deploy General buttons injected into
   every `*CommandCenterCommandSet` (not `Fake*`) in the first free of UI slots 3, 2, 11, 12.
2. `GameLogic::startNewGame` / `GameLogic::reset` → `Zhbo_Reset`: client state released (cursor, hidden
   drawable, control bar), FX weapons freed, sim cleared.
3. Take Control → `MSG_ZHBO_TAKE_CONTROL` → `Sim::take_control` → `set_controlled(true)` → client
   enters the 0.9 s camera fly-in, then first person.
4. F (field computer) → `pressed & FIELD_COMPUTER` (held back, the press carried frame to frame,
   while `Arsenal::holds_switch_back` — an offhand, a melee or an alt toggle under way, as BO2
   holds a weapon change; opening cuts a reload short) → 0.7 s laptop raise (General stands still) →
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
| | `zhbo_logic_choose_class(player, general, index, payload, len)` | The pick menu's choice: the shared list's index, a custom class's wire words appended as `[len, word…]` (at most 64). The words are the class as catalog tokens (`classwire`: item and attachment numbers, perk/camo/reticle/colour indices, the name's bytes; literals only for values outside the catalog), so every legal class fits — 19 words at most over the real install; the profile file's text form outgrew the 64 words (up to 387 bytes) and such picks were refused. Decoding indexes the installed catalog, which every client holds identically; the old text payload still decodes. |
| Profile writes (client) | `zhbo_profile_save(words, len)`, `zhbo_profile_write(words, len)` | Decode a saved custom class / a `save_profile` one-shot into the profile's full text for the bridge to write; null when the words do not decode. `zhbo_profile_write` with `len = -1` (`PROFILE_LIVE`) returns the core's live profile (the text did not fit the payload). |
| Every client frame | `zhbo_client_draw_hud(local_player)` | Draws the first-person HUD through the `ClientApi` draw callbacks. |
| | `zhbo_client_rts_ammo(general, out)` | A General's weapon-slot ammo for the control bar (`ZhboRtsWeapons`). |
| | `zhbo_client_world_poses(out, cap)` | The Generals' third-person weapon/tablet poses and the held weapon's animation set (`ZhboWorldPose`); returns how many. |
| | `zhbo_world_mesh_count(vm)` | Third-person meshes of third-person index `vm`'s weapon on the soldier model (`WeaponDef::world_index`: the viewmodel's, or an alt mode's own past the converted viewmodels); -2: the tablet; -3: one past the highest index carrying meshes, which the bridge scans to (at least 160, so an older core answering 0 keeps the old range). Cached by the bridge on first use. The -3 meaning is ABI 32's (no signature change). |
| | `zhbo_native_body_ready(in)` | 1 when the native third-person body frame can be drawn (see "The native third person"). |
| Building | `zhbo_general_build_rate()` | A General's building speed as a share of a dozer's (`building::GENERAL_BUILD_RATE`). |
| Engine sight and path hooks (patch 0024) | `zhbo_logic_smoke_blocks(a, b)`, `zhbo_logic_smoke_covers(x, y)`, `zhbo_logic_smoke_crosses(a, b, since)` | Lock-free questions to the mirror of the standing smoke screens (see "Smoke screens in Zero Hour's targeting and pathing"). |
| `GameLogic::getCRC` (patch 0020) | `zhbo_logic_crc(out, log)` | The sim's fingerprint and its per-seat/world parts (`ZhboLogicCrc`, 152 bytes); returns 1 when the sim holds state (the bridge xfers the hash after `MARKER:TheAI`), 0 when idle; `log` writes the parts to `ConquerOps.log` (see "Multiplayer (lockstep)"). |

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
`cg_sonarAttachment*` and `cg_rangeFinder*` dvars (`[Optics]`, `sim::optics`), the dropped weapons (`[Drops]`: `g_maxDroppedWeapons`, `player_useRadius`, the use list's facing scale, `_weapons.gsc`'s 60 s `deletepickupafterawhile`, the item's owner second and `Drop_Item`'s toss, its touch box and the player box's height; `sim::drops`), the smoke screen's time and sight blocker (`[Smoke]`: `_smokegrenade.gsc`'s `spawntimedfx` time and `blocksight` radii, t6mp's always-seen distance; `smoke::Params`), the melee reach and the knife's lunge (`player_meleeRange`, `player_meleeCharge*`, `[Melee]`; the lunge distance and times are the weapon def's `meleeChargeRange`/`iMeleeChargeTime`/`iMeleeChargeDelay`), and the MP scripts' equipment, concussion and EMP numbers (`[Equipment]`, `[Concussion]`: the `scr_weaponobject_*`, `betty*`, `scr_proximityGrenade*`, `*hackertooltimems` defaults and literals from `gsc-dump/mp/`, each key citing its `.gsc` line; `equipment::EquipTuning`, `projectiles::Flight`). Script values (not engine dvars) are keyed by their GSC `level.*`/`scr_*` name. BO2's numbers stay in BO2's units (inches,
inches/s, seconds) under the engine's dvar names as ini keys (`g_speed`, `perk_weapSwitchMultiplier`),
each field's doc citing the Ghidra answer it came from (`~/Documents/BO2-reference/ghidra/
answers/G1-G7.md`); code converts at the point of use. The pack is part of the mod like
`weapons.ini`: the bridge loads it once at init (`zhbo_load_tuning`, path from the names
registry), every player must have the identical file, and the sim must never read a tuning value
from anywhere else — modules take `&Tuning` (`MoveTuning::from_tuning`, `Arsenal::tuning`, `EquipTuning::from_tuning`)
instead of their own constants. When the file is absent the built-in defaults keep tests and the
fallback working; a test asserts the shipped file sets every key to its default. Adding a knob:
one line in `tuning.rs`'s field table (field, ini key, default, cited doc) and the same key in
`data/ConquerOps/tuning.ini`. The one section that is not numbers is `[MeansOfDeath]`
(`MOD_* = <Zero Hour DamageType>`, `damage::MeansOfDeathTable`; its defaults in `damage.rs`, held
equal to the file and hashed into the content fingerprint like the numbers).

**Weapons are BO2's patched definitions.** The weapon table converts every `WeaponVariantDef`,
`WeaponAttachment`, model, clip and effect as BO2's asset pool resolves them over the multiplayer
weapon zones (`zhbo_formats::weapon::MP_POOL`, highest priority first: `patch_mp`,
`common_patch_mp`, `common_mp`). `DB_LoadXAssets` (`FUN_0059a550`) loads `common_mp` (zone flag
0x80) with its patch zone `common_patch_mp` (`"%s_patch%s"`, flag 0x100); `DB_LinkXAssetEntry`
(`FUN_00597000`) keeps the asset whose zone flag ranks higher in `FUN_00596e60` (`patch_mp`'s 2 →
0x41, 0x100 → 0x37, 0x80 → 0x36), and a `,name` reference stub never overrides. So the 79 weapons
and 49 attachments `common_patch_mp` redefines carry BO2's post-launch balance (the M8A1's 32-round
magazine, the SMGs' hip kick), and the Peacekeeper, defined only there, is a class item — listed
**locked** when its art does not convert or its DLC (Revolution) is not installed: its
`common_patch_mp` materials sample `,name` image stubs that resolve by name to the images
`patch_mp` streams (`Paks::resolve`), and a patch zone's model may carry only `,name` references
to its materials, which bind to the pool's definition (`viewmodels::resolve_materials` — the SMAW's
`common_patch_mp` world model drew white overhead until they did). The weapon table's `CacLocked`/`CacLockHint` (the hint is the localized
`MENU_<statstable column 15>_REQUIRED_HINT`), set by `zhbo_assets::weapons::lock_items_without_art`
from whether the view model's colour maps resolve and the item's DLC is installed; the picker draws BO2's lock
(`menu_mp_lobby_locked_big`, 64 square, the picture and bars hidden, `c-primary/007`), refuses the
pick, and `classrepair` drops a stored one. The
converter opens the pool and picks them in one place (`zhbo_assets::paks::MpPool`, which loads the
`MP_POOL` zones through the shared `t6zone` loader; models and clips through
`viewmodels::find_model`/`find_anim` over the same pool), and the conformance expected side loads
the same `MP_POOL` zones through the same loader (`zhbo_conformance::bo2::install::Install::pool`). The stringtables
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

**The class editor's and HUD's UI strings come from the install.** The weapon table's `UiText <key>` blocks
carry the text of the localize keys the core draws (`weapons::UI_TEXT_KEYS`: the offhand
titles `MPUI_PRIMARY_GRENADE_CAPS`, `MPUI_SPECIAL_GRENADE_CAPS` and Tactician's
`MPUI_SECOND_SPECIAL_GRENADE_CAPS`, the slot and category headings, the dialog entries, the stat
labels, the prompts, the scoreboard columns; drawn through `Catalog::label`/`label_upper`, BO2's
`Engine.Localize` and `UIExpression.ToUpper`; the mod's own HUD words are `HUD:ZHBO_*` labels in
`strings_en.ini`, read by `strings::text`), read by the converter from the same language zones as the
display names (the install's own language, `t6zone::language`: the folder `localization.txt` names,
`zone/english/en_*.ff`, `zone/french/fr_*.ff`, ..., its prefix read off its files); `Catalog::ui_text` looks them up and a table without them takes the built-in
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
Polish round 2 again moved only the archive: `hud::VERSION` 80 (the weapon table's `FireSoundNpc`,
`CacName`, `CacLocked` and `CacLockHint` keys, the weapons resolved over the `MP_POOL` zones with
the Peacekeeper, the store reticles and the localized labels); the ABI and the blob formats are
unchanged.
Polish round 3 again moved only the archive: `hud::VERSION` 81 (the weapon table's `MeleeCharge` and
`Bounce` keys, the attachment table's `Image` key, the four-number `Flags` without FMJ, the
statstable-derived roster, the order-independent texture conversion, the Assault premade base kit); the ABI and the
blob formats are unchanged.

**ABI 28** (playtest round 2's integration, one bump for the round): `ZhboClientApi.sound_loop` is
appended after `sound_gain` (`ZHBO_CLIENT_API_FIELDS` 14 → 15, the shellshock's ringing; see "The
shellshock's presentation"); no struct moves otherwise, only meanings. `ZhboWorldPose`'s
fourth field is `anim_set` (the held weapon's body animation set, 0 the rifleman's, else the W3D
condition `names::ANIM_SET_CONDITIONS[anim_set − 1]`), replacing the slot flag `sidearm` at the same
offset (16 bytes, now asserted on both sides); `zhbo_logic_choose_class`'s custom payload is the
`classwire` token form (the 64-word size is unchanged; the text form still decodes); the menu's
`save_profile` one-shot may carry length `-1` (`profile::PROFILE_LIVE`: write the core's live
profile). Archive `hud::VERSION` 82 at the same point (the weapon table's `SpawnedGrenade`,
`BodyAnims` and `ExplodeOnGround` keys and its `Shock` blocks (BO2's shellshock files, with their
`Kick`/`Sound` keys; the ringing loop's event, WAV and INI), `explosive_bolt_mp`
converted beside the crossbow, the
per-optic reticle names and art, the patch-zone reference materials, the body's per-set W3D clips
and INI); the native blob formats are unchanged.

**ABI 29** (no-classless-spawn's integration): `ZhboClientOut.select_object` (`uint32_t`, one-shot
object to select overhead: the local General as he comes back alive from first person) is appended
after `viewmodel_fired_left`: `ZhboClientOut` 4872 → 4876 bytes, asserted on both sides; nothing else
moves. Archive `hud::VERSION` 83 at the same point (the weapon table's `UiText
MP_CHANGE_CLASS_NEXT_SPAWN`, the mid-life class change's hint); the native blob formats are unchanged.

**ABI 30** (the multiplayer round's integration, one bump for every append since 29):
`ZhboClientOut.input_seen_frame` (`uint32_t`, the sim frame on screen as the input was made) is
appended after `select_object`: `ZhboClientOut` 4876 → 4880 bytes; `ZhboClientIn.spectating`
(`int32_t`, after `class_button`) fills the struct's tail padding, so `ZhboClientIn` stays 136 bytes
and the field's offset 132 is asserted on both sides; `ZhboClientApi.player_name(player, out, cap)`
is appended after `sound_loop` (`ZHBO_CLIENT_API_FIELDS`/`CLIENT_API_FIELDS` 15 → 16). New free
functions: `zhbo_logic_fps_input(player, general, input, seen_frame)` after `zhbo_logic_input` (which
stays, for a `MSG_ZHBO_FPS_INPUT` without its new seventh argument, the seen frame);
`zhbo_logic_crc(ZhboLogicCrc *out, int32_t log)` with the struct `ZhboLogicCrc` (`uint64_t hash,
world, seats[16]; uint32_t frame, active`: 152 bytes) for host patch 0020 (the logic CRC hook);
`zhbo_terrain_load(bytes, len, names, surfaces, count)` and `zhbo_terrain_surface_at(x, y)` (the
map's ground surfaces, `zhbo_core::terrain`); `zhbo_content_crc`, `zhbo_content_label`,
`zhbo_content_mismatch` and `zhbo_content_versus` with the struct `ZhboContentCrc`
(`ZHBO_CONTENT_PARTS` 7 parts + the total, 32 bytes) for host patches 0018/0021 (the lobby content
check); and `zhbo_hammer_place_near(radius)` (the hammer's nearest placement, `building::place_near`,
which the autopilot's `--build` layout used to copy by hand). Meanings: `ZhboLogicApi.surface_at`
keeps its slot and type but answers the water table only (`ZHBO_SURF_WATER`, else
`ZHBO_SURF_DEFAULT`), the core reading the ground from the grid (a non-default answer is still taken
as it is); the LAN `GameToJoin`/`GameNotJoined` messages gain `zhboContent` (host patch 0021; builds
without it cannot join builds with it — the INI CRC differs anyway). Every struct size above is
asserted on both sides. Archive `hud::VERSION` 84 at the same point (the weapon table's top-level
`InstalledDlc` line and `CacSlot` now a sim field of the content fingerprint; the spectator HUD's two
converted materials `hud_shoutcasting_change_tab` and `hud_shoutcasting_viewing_box_dead` and two
`UiText` keys `MPUI_SPECTATING` and `MPUI_FIRST_PERSON_VIEW`); the native blob formats are unchanged.

**ABI 31** (playtest round 3's integration, one bump for the round): three free functions are
appended after `zhbo_hammer_place_near`, `zhbo_logic_smoke_blocks(ax, ay, az, bx, by, bz)`,
`zhbo_logic_smoke_covers(x, y)` and `zhbo_logic_smoke_crosses(ax, ay, bx, by, since)` (each
`int32_t`, lock-free, from the sim's mirrored screens; host patch 0024, see "Smoke screens in Zero
Hour's targeting and pathing"); no struct or API table moves. One meaning changes:
`ZhboLogicApi.query` returns every object whose bounding sphere comes within the radius (Zero Hour's
`FROM_BOUNDINGSPHERE_3D`, see "Units"), not centre-to-centre. Archive `hud::VERSION` 85 at the same
point: the converted effects (runners, stubs, counts and motion by BO2's rules, each system at its
`fx::Role`'s priority, the `_Framed` copies of flashes and bullet impacts, the smoke screen at
`AREA_EFFECT` standing the tuning pack's 12 s), the props drawn at `GENERAL_SCALE` (the projectile
objects' `Scale`, the planted shield in its deploy pose), the weapon table's `Shield` field now
three boxes (39 numbers; a table converted before it does not parse, so the archive must be
rebuilt) and two sound events, `ZHBO_BO2_StepNpc` and `ZHBO_BO2_SprintStepNpc` (other players'
footsteps). The native blob formats are unchanged.

**ABI 32** (round 4's integration, one bump for the round): `ZhboLogicApi.damage`'s `kind` widens
from three codes to one per Zero Hour `DamageType`: `ZHBO_DAMAGE_SMALL_ARMS`/`MELEE`/`EXPLOSION`
keep 0–2 and `ZHBO_DAMAGE_CRUSH` … `ZHBO_DAMAGE_STATUS` are appended as 3–37 (`ZHBO_DAMAGE_COUNT`
38; Zero Hour's order after the first three, `damage::ZH_DAMAGE_TYPES`, a core test holding the
header and the bridge's `s_zhboDamageTypes` map to it, a `static_assert` its size; an older bridge
would take every new code as small arms, so core and bridge move together;
`zhbo_logic_shield_damage` still reports only the first three). The dropped weapon's native draw is
appended: the struct `ZhboDroppedWeapon` (24 bytes: `object`, `vm`, `pos[3]`, `yaw`; asserted on
both sides), the free function `zhbo_client_dropped_weapons(out, cap)` (the sim's drops with a Zero
Hour object, `Sim::dropped_poses`; it also releases the rigs of drops gone since) and the
`ZhboNativeBodyIn.mode` value `ZHBO_BODY_LYING` (3: `general` is the drop's object, `pos`/`yaw`
where it lies, no tag; the bridge passes the prop's render transform). One meaning is added:
`zhbo_world_mesh_count(-3)` answers one past the highest third-person index carrying meshes (an alt
mode's own world weapon, `WeaponDef::world_index`, past the viewmodels), which the bridge scans to.
No other struct or API table moves.

Archive `hud::VERSION` 86 at the same point: the weapon table's new fields (`NpcSounds`,
`TorsoAnims`; `Drop`, `DropObject`, `HudIconRatio` and the four pickup sounds; `LowAmmoWarning`;
`FireStartSoundNpc`, `FireLastSoundNpc`, `ScreenFx`, `HudIcon`, `WorldWeapon`: 169 fields), the
`UiText` keys
`PLATFORM_PICKUPNEWWEAPON`, `PLATFORM_SWAPWEAPONS`, `PLATFORM_RELOAD`, `PLATFORM_LOW_AMMO_NO_RELOAD`
and `PLATFORM_HOLD_BREATH` (read from the install's own language, `t6zone::language`), the torso
and death clips in each faction body manifest (ids from `torso::BASE`; the `.zbc` format is
unchanged), the torso rows in `sound_layers.txt`, the new AudioEvents (`ZHBO_BO2_SndNpc_*`,
`ZHBO_NTN_*`, the voices, `ZHBO_BO2_LandNpc`, the 117 battle-chatter events, the NPC fire start/last
events, the pickup sounds), the General INI without the Zero Hour death voice, the dropped weapons'
`ZHBO_BO2_Drops.ini` and `ZDR###` models, the alt modes' own world weapons (`ZW<index>.zbw` past the
viewmodels), the HUD icons by material name, the converted effects on host patch 0028's particle
fields (`FrameGrid` …, `SpawnRange`, `FarFade`, `NearFade`; an engine without 0028 refuses them),
the flipbook grid textures, the projectile trails (`ParticleSysBone`), `TerrainScorch` and the
millisecond `LightPulse`, the four equipment-breaking effects, and the smoke grenade's screen as its
`ScreenFx`. The native blob formats are unchanged (`BODY_VERSION` 1, `WEAPON_VERSION` 3: the torso
and death clips are new ids in the body's clip list, an alt mode's world weapon and an offhand's are
ordinary `.zbw` files at their own index).

**Pending for the next bump (archive `hud::VERSION`, death-anims):** the `death` rows of
`sound_layers.txt` are keyed by the new death context (`torso::death_context`: stance × `movestatus`
3 × `direction` 5 × BO2's ten `dmgType`s × `dmgDirection` 5 = 2,250 contexts, was 144); an archive
converted before reads as the wrong deaths (never a crash: every context still finds a clip or
none). The tuning pack gains `player_runThreshhold`, `player_moveThreshhold` and
`player_strafeAnimCosAngle` (the content CRC moves with it). No ABI change.

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

### Other players' actions: torso, third-person def sounds, voices (`client/remote.rs`)

* **The torso table** rides `sound_layers.txt` (the same file, the same `zhbo_load_sound_layers`
  call, which installs `crate::torso` too): `torso <set> <action> <context> <clip id>`,
  `torso_clip <clip id> <seconds> <name>`, `torso_note <clip id> <seconds> <event>` (actions and
  contexts: `zhbo_native::thirdperson::torso`). A note's event is the torso clip's `sndnt#` alias as
  a positional AudioEvent `ZHBO_NTN_<alias>` (`names::npc_notetrack_event`): BO2's third-person
  reload foley (`fly_assault_reload_npc_mag_out`, …) lives only there — no MP weapon names a
  `reloadSound`. A note whose alias no MP bank has is left out.
* **Third-person def sounds.** `WeaponDef::npc_sounds` (`NpcSounds`: `raise`, `first_raise`,
  `putaway`, `melee_swipe`, `melee_hit`, `pullback`) are BO2's fields without `Player`, each a
  positional event `ZHBO_BO2_SndNpc_<alias>` (`names::npc_def_sound_event`) apart from the owner's
  2D `ZHBO_BO2_Snd_<alias>` even where BO2 names one alias for both; a gun's swing and hit are the
  class knife's.
* **Voices.** `sounds::voice_event`: `ZHBO_BO2_PainSmall_<F><n>` / `ZHBO_BO2_PainDeath_<F><n>` per
  faction letter `F` and voice number `n` (0..2), from the faction's language zone
  (`en_faction_<team>_mp`, `vox_<team><n>_pain_small/_pain_death`). The client plays pain on a
  `Damaged` event when the General was not at full health before it, and death on `GeneralDied`
  where he was last seen, in seat `% 3`'s voice and the faction the bridge names to
  `zhbo_native_body` (the USA voice while none has, e.g. `ZHBO_NATIVE_BODY=0`). The General INI no
  longer plays Zero Hour's hero death voice.
* **More of another player** (follow-up): his dry fire (`NpcSounds` `empty_fire`, BO2's
  `emptyFireSound`, once per trigger pull on an empty magazine), his jump and landing
  (`_footsteps.csc`: the step alias and `ZHBO_BO2_LandNpc`), the script's `flinch` (asked from the
  front: the sim's damage has no direction) and its whole-body `jump`/`land` clips as torso
  actions (`torso::FLINCH`, `JUMP`, `LAND`; the last two replace every bone while they run), and
  BO2's battle chatter (`_battlechatter_mp.gsc` `reloadtracking`/`grenadetracking`:
  `ZHBO_BO2_Bc_<F><n>_<line>`, `sounds::CHATTER_LINES`; at the dvars' odds by a roll hashed from
  the seat and his chatter count, so every machine agrees; on his head for everyone, as
  `mpsaylocalsound` → `dosound` → `playsoundontag` plays it; never with Dead Silence). During a
  grenade's pin and throw the body pass does not draw the gun (`Loaded::hide_weapon`): BO2 draws
  the entity's one current weapon, the grenade, on `tag_inhand` (t6mp `FUN_0052a4c0`,
  `FUN_0052a470`); the grenade itself is not drawn there yet.
* **Who hears what.** Every seat's torso is tracked (the presented one too: an overhead General is
  drawn); its notes, the def sounds and the weapon change's sounds play at the General for everyone
  but a presented first-person General (his 2D versions play); the voices play for everyone, him
  included. Presentation only: events and records in, nothing back.

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
