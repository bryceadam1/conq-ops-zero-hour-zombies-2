# Configuration: the `ZHBO_*` environment variables

Every `ZHBO_*` variable, who reads it, and when. The core reads its variables exactly once at init
into `zhbo_core::config::Config`; the bridge reads its variables exactly once into `ZhboConfig`
(`ZhboBridge.cpp::readConfig`). Deterministic sim code reads only those structs — never the
environment mid-game.

## The Rust core (`crates/zhbo-core/src/config.rs`, parsed in `zhbo_init`)

| Variable | Meaning |
|---|---|
| `ZHBO_LOADOUT` | The General's kit spec (`weapons::Loadout::with_spec`, e.g. `primary=dsr50_mp+acog;perks=Hardline`). Every player in a lockstep game must set it identically — it is a sim input, like the map. Empty or unset: the built-in kit. The converter reads the same variable so the kit's art gets converted. |
| `ZHBO_NATIVE_VIEWMODEL` | Draw the first-person weapon through the renderer's native BO2 pass (`nativevm`, Track R) instead of the W3D conversion. **On by default** (verified in game 2026-10-21: shading, culling, skin weights, the near plane, hide tags, placement); `0`/`false`/`no`/`off` falls back to the W3D conversion. Presentation only (never a sim input); wgpu builds only (elsewhere it is ignored and W3D draws). A weapon whose native files are missing falls back to W3D, logged once. |
| `ZHBO_NATIVE_BODY` | The overhead (third-person) General through the native BO2 pass (`nativebody`). **`1`, the weapon, by default**: the native world weapon rides the W3D body's `tag_weapon_right`, the only third person that shows a combination's attachments and the class camo (the W3D weapon is the base mesh, stock). `2`/`full`: the native body too (the W3D body is hidden). `0`/`false`/`no`/`off`: the W3D weapon and body (the fallback). Presentation only; wgpu builds only. A weapon whose native files are missing, still loading, or without the engine's tag falls back to the W3D weapon. |
| `ZHBO_AUTOPILOT` | The in-game test harness. `manual`: the scene spawns but input stays with the player or a test driver. Any other non-empty value: scripted input drives the General; `equipment` adds the Black Hat / Trophy System scene, `build` scripts first-person construction, `classpick` runs the in-match class screen (Escape declines the first spawn, the next take-control picks the profile's seeded custom by its row key, a third forces the screen open and Escapes it; then the General is killed and the next one taken, who must come in with the class), `showcase` films the General (see the bridge's showcase variables below), `smoke` throws the seeded class's smoke grenade and tests Zero Hour's units against its screen (the bridge's `smokeAutopilotFrame`: a Ranger and a rebel across it, a neutral Ranger ordered through it). `mp` is the two-instance multiplayer run (`scripts/mptest-mac.sh`): nothing is spawned; each machine orders its own General from its Command Center, walks him to a meeting point between the bases, takes control and fights the other player's General through the replicated messages only (`client/autopilot.rs` `mp_input`, the bridge's `mpAutopilotFrame`). Like `ZHBO_LOADOUT` it is folded into the content CRC, so every machine of a game must set it the same. |
| `ZHBO_AUTOPILOT_ROLE` | The `mp` run's part: `target` fires a short burst and is the one meant to die and come back; anything else (the default) is the shooter, who fires the long aimed burst. It only scripts the local player's own input, so the two machines may (and do) differ. |
| `ZHBO_AUTOPILOT_ROUNDS` | The `mp` run's length: how many first-person lives each player plays (default 1, the short run). More is the long run (`scripts/mptest-mac.sh` with `ZHBO_MPTEST_ROUNDS`, TESTING.md "Multiplayer in game"): a declined first spawn, rounds with tacticals, a mid-life class change, and the joiner leaving in his last life. Script only, read by the core and the bridge. |
| `ZHBO_STATE_HASH_LOG` | `1`: log `Sim::fingerprint` after every logic step (`state hash: frame N <hash> world <hash> seats <seat>:<hash> …`, the parts the CRC frame line prints), so the logs of the machines in one network game can be compared frame by frame (the `mp` test does; on two real machines, set it on both and compare the lines). A log only, never a sim input. |

## The bridge (`host/zhbo/ZhboBridge.cpp`, parsed in `readConfig` at `Zhbo_Init`)

| Variable | Meaning |
|---|---|
| `ZHBO_AUTOPILOT` | Same modes as above; parsed once here and once in the core. |
| `ZHBO_AUTOTEST_SKIRMISH` | Test only: from the main menu, open Skirmish and press Start (the replay test, `autotest-mac.sh --replay`, needs a recorded game; a loaded save records none). Not folded into the content CRC: it changes no sim input. |
| (none) | Several game instances on one machine share the user-data directory: every instance after the first (the engine's multi-instance ids, as its own `Options_Instance02.ini`) writes `ConquerOps_InstanceNN.log` and reads/writes `ConquerOps_InstanceNN.profile`. |
| `ZHBO_FORCE_RADAR` | Give the autopilot's test player radar, to film the full map (M). |
| `ZHBO_SHOWCASE_OBJECT` | Template name to spawn in front of the camera in showcase mode (default `ZHBO_General_USA`). |
| `ZHBO_SHOWCASE_ZOOM` | Camera zoom distance for showcase captures (default `0.12`). |
| `ZHBO_SHOWCASE_PITCH` | Camera pitch in radians (default: the view's own). |
| `ZHBO_SHOWCASE_ANGLE` | Camera angle offset in radians around the General (default: the view's own). |
| `ZHBO_CLOSEUP` | Follow the General closely in captures; the value is the zoom distance. |
| `ZHBO_BO2_DEBUG` | BO2 shading term view, read by the backend's `bo2Shade` (host patch 0009) and passed to the native pass by `ZhboViewmodel.cpp`: -1 BO2 shading off (W3D path only), 1 flat normals, 2 normals, 3 diffuse light, 4 specular, 5 reflection. |

## The renderer (`crates/zhbo-gfx`, read at device init; client-side, exempt from determinism)

| Variable | Meaning |
|---|---|
| `ZHBO_GFX_BACKEND` | Force the backend: `vulkan`, `metal`, `dx12`, … (default: the platform's preferred order). |
| `ZHBO_GFX_FALLBACK` | Set to force the software fallback path even when a hardware adapter is present. |

## Tools and the launcher (offline, not part of the game)

| Variable | Meaning |
|---|---|
| `ZHBO_ZH_DIR` | Path to the Zero Hour install (override for the auto-detection; also passed to the converter by the launcher). |
| `ZHBO_BO2_DIR` | Path to the Black Ops II install (same). |
| `ZHBO_WORK_DIR` | Where the launcher keeps its work tree (converted data, engine checkout); defaults to the user data dir. |
| `ZHBO_GLUE_DIR` | Set by `cargo xtask build` for the engine build: where the bridge glue sources are copied. |
| `ZHBO_CONVERT_ALL_WEAPONS` | Set by the converter's user: convert every BO2 player weapon, not just the loadout's kit. |
| `ZHBO_CONVERT_ALL_COMBOS` | Set to `0` to skip the legal Create-a-Class combination sweep and build only the base set; on by default, so a locally built archive covers every legal combination (the archive stamp records the flag). |
## The engine's test switches (host patch 0022; release builds, off unless set)

| Variable | Meaning |
|---|---|
| `GGC_NETWORK_AUTOSTART` | Lets a release build obey the engine's network auto-start driver (`-autoNetworkMode direct` with `-autoNetworkHost <players>` / `-autoNetworkJoin <ip>`, `-autoNetworkMap`, `-autoNetworkName`, `-autoNetworkTimeout`; a debug-build feature the mod's build compiles in with `RTS_NETWORK_AUTOSTART`). Without it those arguments are ignored. The driver also turns on multi-instance (no single-instance lock; instance ids from 2). |
| `GGC_LOOPBACK_INSTANCE_PORTS` | Multi-instance runs only: keep each instance's 127.0.0.<id> as its identity but bind 127.0.0.1 at ports offset by 100 × id (`Transport.cpp`), so several instances talk on one Mac without `sudo ifconfig lo0 alias`. |

## Test and diagnostic knobs (test runs only; never read by the game)

| Variable | Meaning |
|---|---|
| `ZHBO_CORE_LIB` | Set by `cargo xtask build` with `ZHBO_GLUE_DIR`: the built `zhbo_core` static library the engine links (`host/zhbo/CMakeLists.txt`). |
| `ZHBO_BO2_ARCHIVE` | The class-screen screenshot harness's archive when `--archive` is not given (TESTING.md). |
| `ZHBO_SWEEP_TGA` | The viewmodel sweep test also writes each composition's textures under this directory (`viewmodel_sweep.rs`). |
| `ZHBO_SFX_DUMP` | The real-install sound test writes the converted `ZHBO_BO2.ini` sound effects to this file (`sfx.rs`). |
| `ZHBO_DUMP_DIR` | The real-install range-finder test writes the converted display images here (`rangefinder.rs`). |

## The archive and the camo conversion

`ConquerOps_BO2.big` carries the class camo art (the set images `Art\Textures\zhbo_t6_camo_*`,
the style materials' colour maps such as `zhbo_camo_gold_col` and their `_nsg` maps, …) and one `Art\Native\<camo_<weapon>>.zcm` recipe per converted weapon. The
archive's version (`hud::VERSION`) is bumped with a conversion that changes its contents, so after a
mod update the launcher regenerates the archive. With a stale
archive the native viewmodel logs `the camo art is not in the archive (reconvert the BO2 art)` and
draws the stock finish; the class camo never reaches the W3D fallback or the third-person body
(CONTRACT.md, "The native viewmodel").

## Graphics detail (Zero Hour's `GameLOD`)

Zero Hour's detail level is its own: `Data\INI\GameLODPresets.ini` decides which level a machine is
recommended, `Data\INI\GameLOD.ini` what each level sets (the `StaticGameLOD` blocks: particle cap,
shadows, texture reduction, …) and the dynamic levels the render frame rate picks (`DynamicGameLOD`).
The mod overrides both from `ConquerOps.big`, in files loaded after the stock ones; a block there
changes only the fields it names:

| File | What it sets |
|---|---|
| `data/Data/INI/GameLODPresets/ZHBO_GameLODPresets.ini` | `UnclassifiedHardwareLOD = High`: the level recommended for hardware the 2002 presets cannot classify (host patch 0025; without the line the stock fallback, Low, stands). |
| `data/Data/INI/GameLOD/ZHBO_GameLOD.ini` | The dynamic `Medium` and `Low` levels (below 20 fps) stop thinning at `AREA_EFFECT`, so the smoke screen looks whole at every frame rate. |

**Why the stock detector says Low.** `GameLODPresets.ini` knows three CPUs, P3, P4 and K7, and
classifies anything else by a benchmark that SuperHackers replaced with a stub (fixed scores of 10,
below the P4 and K7 profiles' float scores of 15–21). Every CPU since about 2005 (CPUID family 6 with
a newer model, AMD's later families) and every ARM CPU is none of the three, so on every platform the
mod builds (Windows DX8 and wgpu, macOS, Linux) the detector fell back to Low: 500 particles, half-
resolution textures, no shadows. The video side was never the cause (the wgpu backend reports a
shader-model-2 card, a DX8 card is read from its caps or NVIDIA id). Host patch 0025 gives such
hardware the data's level instead, with the stock 256 MB memory check still in force.

**Detected versus chosen.** `Options.ini` holds `IdealStaticGameLOD` (the detector's pick) and
`StaticGameLOD` (the level in use, written by the Options menu only when the player changes it), so
it cannot say whether a level was chosen. With the patch, a `StaticGameLOD` equal to the
`IdealStaticGameLOD` is treated as the detector's: the game detects again at start (and writes both),
so a file holding the old detector's Low gets High. A level that differs from the recommendation
(Low, Medium, Very High or Custom set in the Options menu) is the player's and is kept. The one case
it cannot tell apart is a player who explicitly chose the very level the old detector had picked:
he gets High once, and Low set again then persists (the recommendation is now High).

**The dynamic levels** follow the average render frame rate (`MinimumFPS`: VeryHigh above 25, High
above 20, Medium above 10): above 25 fps everything draws; from 25 down the lowest priorities stop
drawing (the converted casings and blast dressing are `WEAPON_EXPLOSION`; blasts, impacts and
muzzle flashes are `CRITICAL` and always draw, `fx::Role`), and below 20 fps the skip mask thins what
is left under the skip priority. `DebrisSkipMask` and `SlowDeathScale` are read by the logic, so they
must stay 0 and 1.0 at every level: the dynamic level is each machine's own. The thresholds are absolute
frame rates, so with the render at the display's rate (below) they engage only on a machine that
falls under 25 fps. Measured on an M1 Pro Mac at High (the `--splash` leg: a SMAW and two frags into
an armour group) with the old 30 fps cap: a steady 30 fps, never leaving VeryHigh; uncapped
(`-noFPSLimit -fixedLogicFPS 30`) 87–191 fps, against 103–247 at Low. The tests in
`crates/zhbo-assets/src/detail.rs` hold the data to these rules (the real-install one over the
player's own `GameLOD.ini`). The bridge logs the applied level once (`graphics detail: …`) and, in a
game, the dynamic level's changes and the frame rate every 5 s (`frame rate: …`). Graphics detail is
presentation only: it is not in the content CRC and never reaches the sim.

## The render rate (host patches 0026, 0027)

Zero Hour capped rendering at `FramesPerSecondLimit` (30) and, outside network games, ran one logic
frame per rendered frame, so the render cap was also the game's speed. The mod separates them at
every game start (`MSG_NEW_GAME` → `Zhbo_PaceNewGame`, `ZhboDetail.cpp`):

* **The logic** runs at the game's own speed: the skirmish or replay's speed argument, else
  `LOGICFRAMES_PER_SECOND` (30), through SuperHackers' logic time scale. A replay plays at its
  recorded speed (`FramePacer::setGameLogicFps`; stock tied a replay's logic to the render update). A
  network game's logic stays paced by the network. The sim steps exactly the frames it did.
* **The render rate** follows the display: the refresh rate of the display the window is on (SDL3's
  current display mode; `EnumDisplaySettings` on the DX8 build), at least 60. The player's
  `Options.ini` can set it: `MaxRenderFPS = 144` caps there (never below the logic rate),
  `MaxRenderFPS = Uncapped` removes the cap, and `0`, `Display` or no line follows the display. It is
  a per-machine setting, like the detail level, and never reaches the sim. SuperHackers' in-game
  render-rate keys still change it until the next game. Vsync stays off (the wgpu backend's
  default): the engine's own limiter paces the frames on every build, DX8 windowed included.
* **Between logic frames** what is drawn moves at the render rate: the first-person eye, view kick
  and roll (the core, CONTRACT.md "Time"); every object's drawable, drawn between its last two logic
  transforms at the render's logic-frame phase (`Drawable::getRenderTransformMatrix`, one logic frame
  behind like the eye; a jump over 100 units in one frame is a teleport and snaps); the native
  third-person weapon and body, placed from that transform; W3D animation, whose clock now steps every
  render update by its share of the logic frame while the logic runs; particles (SuperHackers already
  integrate them by the phase). The viewmodel, sway, bob, ADS, HUD and tracers run on the client's
  real `dt`.

* **A script's game speed.** A map script's `SET_FPS_LIMIT` (and `ScriptEngine::reset`, which put
  the default back) set the render cap, which in stock *was* the game's speed. Host patch 0027 makes
  it set the logic rate instead (`Zhbo_ScriptGameSpeed`; 0 restores the game's own speed, a replay
  follows it), and leaves the render rate to the display.
* **The autotest captures** (`GGC_BGFX_SCREENSHOT_AFTER`/`_INTERVAL`, a harness flag of the bgfx
  backend) count logic frames since host patch 0027, so a capture schedule is the same game time at
  any render rate. The DX8 backend's `GGC_DX8_SCREENSHOT_*` still count render frames.

The bridge logs `render rate: up to N fps (<why>; display M Hz), logic L fps` when a game starts,
and `game speed: N fps (a script's | the game's own)` when a script changes it.
