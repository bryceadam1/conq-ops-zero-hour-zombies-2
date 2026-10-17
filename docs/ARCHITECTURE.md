# Architecture

The crate graph and the dependency rules. Read [CONTRACT.md](CONTRACT.md) for the engine ↔
Rust ownership split and [CONFIG.md](CONFIG.md) for every `ZHBO_*` variable.

## Crate graph

```
zhbo-requirements   zhbo-formats   zhbo-gfx
        │               │             │
        └── zhbo-core ──┘             │
        │       │                     │
        │       └──(optional, feature "renderer")
        │               │
        └─ zhbo-assets ─┘
                │
        └─ zhbo-launcher

host/zhbo (C++) ←─ ABI ─→ zhbo-core
```

## Crates

### `crates/zhbo-requirements`
Finds and validates both game installs: Steam libraries, SteamCMD folders,
Wine/CrossOver/Proton prefixes. Depends on nothing. Used by the launcher and by zhbo-core
(install paths for the weapon table).

### `crates/zhbo-formats`
Parsers for game data: Zero Hour `.big`, Black Ops II `.sabs`/`.sabl` sound banks, `.ipak`
image packs, `IWi` images, LZO1X, and the BO2 fastfile decryption. Depends on nothing. All
readers are hardened against corrupt input (refactor plan Phase 0.3) and used by the
converter (`zhbo-assets`) and the runtime (`zhbo-core` reads the BO2 zone through the host's
own data).

### `crates/zhbo-gfx`
The renderer: the engine's bgfx API subset implemented on wgpu, plus the build-time shader
translator. Depends on nothing. Split into `runtime` (`mod.rs` device/resource pools,
`frame.rs` frame phases), `state.rs` (pipeline/bind-group state), `shader.rs`, and `ffi.rs`
(the C ABI the engine's backend calls). Optional dependency of `zhbo-core` behind the
`renderer` feature.

### `crates/zhbo-core`
The Rust gameplay core: BO2-style movement, weapons, hitscan, field computer, camera, HUD,
and the deterministic lockstep simulation. This is the mod's brain; everything reachable
from `zhbo_logic_*` must stay deterministic. Depends on `zhbo-requirements` and optionally
`zhbo-gfx`. After Phase 2.3:
- `weapons/`: `def.rs` (WeaponDef, enums), `table.rs` (the weapon table's schema,
  parse and format), `catalog.rs` (the converted table or built-in stand-ins),
  `perks.rs` (the perk table), `loadout.rs`, `rng.rs` (xorshift), `arsenal.rs`
  (WeaponState, Reload, Offhand, Arsenal, WeaponAction).
- `sim/`: `mod.rs` (Sim, PlayerSim, Event, the top-level `step` orchestration),
  `hack.rs` (Black Hat), `build.rs` (hammer/site), `rts.rs` (`rts_update`,
  `rts_offhand`, `RtsWeapons`), `fire.rs` (`apply_weapon_action`, `gather_targets`,
  `throw_velocity`).
- `config.rs` (env vars parsed once at init), `names.rs` (the naming registry),
  `tuning.rs` (the gameplay tuning pack), `strings.rs` (the localised strings),
  `equipment.rs` (BO2 equipment behavior, driven by the weapon table's `Equipment` block).

### `crates/zhbo-assets`
The BO2 → mod converter: reads your BO2 install (through `zhbo-formats`), converts HUD art,
sound, weapon viewmodels and the weapon table, and writes `ConquerOps_BO2.big`. Also a CLI
to list/export BO2 sounds and images. Depends on `zhbo-formats`, `zhbo-requirements`, and
`zhbo-core`. After Phase 2.5 the module is split into `textures.rs` (the texture pipeline),
`ini.rs` (the INI rewriters), `viewmodels.rs` (the viewmodel conversion), `generals.rs`
(the Zero Hour names, factions and the `files()` orchestrator), and `paks.rs`
(pack-file access, the shrinker and the zone loader).

### `crates/zhbo-launcher`
`ConquerOps.exe` / `conquerops`: requirement check, BO2 art conversion on first launch,
starting the game. Depends on `zhbo-requirements` and `zhbo-assets`.

## The host bridge (C++)

`host/zhbo/*.cpp` is the engine-side glue. After Phase 2.1 it is split by responsibility:

| File | Responsibility |
|---|---|
| `ZhboBridge.cpp` | Globals, API-table init, `Zhbo_Init/Reset/…` entry points |
| `ZhboLogicApi.cpp` | Logic callbacks (`cbLog`…`cbFx`) |
| `ZhboRtsWeapons.cpp` | RTS shot tracking, ammo overlay |
| `ZhboHud.cpp` | Text cache, fill/line/text/image/clip/radar/cash |
| `ZhboInput.cpp` | `ZhboTranslator`, mouse capture, hidden object, RTS UI hide |
| `ZhboAutopilot.cpp` | The in-game test harness |
| `ZhboScreenLook.cpp` | Sky/BO2-look toggles |
| `ZhboControlBar.cpp` | `ZhboControlBarAccess`, idle-worker tracking |
| `ZhboBuilding.cpp` | First-person build/repair/ghost |

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

1. `zhbo-formats`, `zhbo-requirements`, `zhbo-gfx` depend on nothing in this repo.
2. `zhbo-core` may depend on `zhbo-requirements` and, behind the `renderer` feature,
   `zhbo-gfx`. It must never depend on `zhbo-assets` (the converter depends on the core,
   not the other way round).
3. `zhbo-assets` may depend on `zhbo-core` and `zhbo-formats`.
4. `zhbo-launcher` may depend on `zhbo-requirements` and `zhbo-assets`.
5. Engine code (`host/GeneralsGameCode`) is never modified; changes go through
   `host/patches/*.patch`. The bridge (`host/zhbo/*.cpp`) is the only engine-side mod code
   and it owns the ABI in `host/zhbo/zhbo_core.h`.
6. Nothing from either game is shipped; converted data stays local (see
   [CONTRACT.md](CONTRACT.md) "Asset rule").
