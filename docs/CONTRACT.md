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

## Versioning

`ZHBO_ABI_VERSION` (`host/zhbo/zhbo_core.h`) must equal `zhbo_abi_version()`; struct sizes are asserted
on both sides (`static_assert` in the header, `const _: ()` in `ffi.rs`).
