# Configuration: the `ZHBO_*` environment variables

Every `ZHBO_*` variable, who reads it, and when. The core reads its variables exactly once at init
into `zhbo_core::config::Config`; the bridge reads its variables exactly once into `ZhboConfig`
(`ZhboBridge.cpp::readConfig`). Deterministic sim code reads only those structs — never the
environment mid-game.

## The Rust core (`crates/zhbo-core/src/config.rs`, parsed in `zhbo_init`)

| Variable | Meaning |
|---|---|
| `ZHBO_LOADOUT` | The General's kit spec (`weapons::Loadout::with_spec`, e.g. `primary=dsr50_mp+acog;perks=Hardline`). Every player in a lockstep game must set it identically — it is a sim input, like the map. Empty or unset: the built-in kit. The converter reads the same variable so the kit's art gets converted. |
| `ZHBO_NATIVE_VIEWMODEL` | `1`/`true`/`yes`/`on`: draw the first-person weapon through the renderer's native BO2 pass (`nativevm`, Track R) instead of the W3D conversion. Presentation only (never a sim input); wgpu builds only (elsewhere it is ignored and W3D draws). Off by default until verified in game. A weapon whose native files are missing falls back to W3D, logged once. |
| `ZHBO_AUTOPILOT` | The in-game test harness. `manual`: the scene spawns but input stays with the player or a test driver. Any other non-empty value: scripted input drives the General; `equipment` adds the Black Hat / Trophy System scene, `build` scripts first-person construction, `classpick` runs the create-a-class editor (open from the pick screen, name the class, change the secondary, save), `showcase` films the General (see the bridge's showcase variables below). |

## The bridge (`host/zhbo/ZhboBridge.cpp`, parsed in `readConfig` at `Zhbo_Init`)

| Variable | Meaning |
|---|---|
| `ZHBO_AUTOPILOT` | Same modes as above; parsed once here and once in the core. |
| `ZHBO_FORCE_RADAR` | Give the autopilot's test player radar, to film the Tab map. |
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