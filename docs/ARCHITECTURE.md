# Architecture

The crate graph and the dependency rules. Read [CONTRACT.md](CONTRACT.md) for the engine ↔
Rust ownership split and [CONFIG.md](CONFIG.md) for every `ZHBO_*` variable.

## Crate graph

```
zhbo-requirements   zhbo-formats   zhbo-gfx   zhbo-native
        │               │   │         │            │
        └── zhbo-core ──┘   │         │            │
        │       │  └────────┼─────────┼────────────┤
        │       └──(optional, feature "renderer")  │
        │               │   └ zhbo-conformance     │
        │               │          │               │
        └─ zhbo-assets ─┘──────────┴───────────────┘
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
Parsers for game data: Zero Hour `.big` (from a file or, `big::members`, from memory), Black Ops II
`.sabs`/`.sabl` sound banks, `.ipak` image packs, `IWi` images, LZO1X, the BO2 fastfile decryption,
and the compiled UI Lua (`havok_lua`: Havok Script chunks and their constant stores). Depends on nothing. All
readers are hardened against corrupt input (refactor plan Phase 0.3) and used by the
converter (`zhbo-assets`) and the runtime (`zhbo-core` reads the BO2 zone through the host's
own data).

### `crates/zhbo-gfx`
The renderer: the engine's bgfx API subset implemented on wgpu, plus the build-time shader
translator. Depends on nothing. Split into `runtime` (`mod.rs` device/resource pools,
`frame.rs` frame phases), `state.rs` (pipeline/bind-group state), `shader.rs`, and `ffi.rs`
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
from `zhbo_logic_*` must stay deterministic. Depends on `zhbo-requirements` and optionally
`zhbo-gfx`. After Phase 2.3:
- `weapons/`: `def.rs` (WeaponDef, enums), `table.rs` (the weapon table's schema,
  parse and format), `catalog.rs` (the converted table or built-in stand-ins),
  `perks.rs` (the perk table: each perk's specialty set), `specialty.rs` (BO2's 62
  specialty bits), `feel.rs` (gun kick, sway, viewmodel offsets, reticle data),
  `recoil.rs` (per-shot kick, the gun kick spring, flinch, the spread cone), `gunpos.rs`
  (the client-side gun position: offsets, bob, sway), `loadout.rs`, `rng.rs` (xorshift),
  `arsenal.rs` (WeaponState, Reload, Offhand, Arsenal with HasSpecialty and the perk-timed
  weapon states, WeaponAction).
- `sim/`: `mod.rs` (Sim, PlayerSim, Event, the top-level `step` orchestration),
  `hack.rs` (Black Hat), `build.rs` (hammer/site), `rts.rs` (the overhead General:
  `rts_update`, the third slot's orders `rts_offhand`/`Third`, `rts_reload`, `RtsWeapons`
  with each slot's Zero Hour weapon, `RtsIcons`), `fire.rs` (`apply_weapon_action`,
  `melee_strike`, `gather_targets`, `throw_velocity`).
- `lui/`: BO2's LUI layout model shared by the HUD and the class screens: the anchor/edge
  resolution against a 720-unit root (`mod.rs`), BO2's converted UI fonts (`font.rs`), the
  BO2 materials the ports draw by name (`materials.rs`) and LUI's animation states (`anim.rs`).
- `hud/` and `classui/`: the first-person HUD and the create-a-class screens, one file per element
  or concern.
- `config.rs` (env vars parsed once at init), `names.rs` (the naming registry),
  `tuning.rs` (the gameplay tuning pack, BO2 units), `strings.rs` (the localised strings),
  `equipment.rs` (BO2 equipment behavior, driven by the weapon table's `Equipment` block).
- `nativevm/`: the native first-person viewmodel (Track R, `ZHBO_NATIVE_VIEWMODEL`): files asked
  of the bridge, composition and posing (pure, tested without a GPU), the renderer upload/submit
  (`gpu.rs`, feature `renderer`), the class camo swap (`camo.rs`: the `.zcm` recipe applied to the
  composed mesh, plus the core-global transport) and its C ABI (`ffi.rs`).

### `crates/zhbo-assets`
The BO2 → mod converter: reads your BO2 install (through `zhbo-formats`), converts HUD art,
sound, weapon viewmodels and the weapon table, and writes `ConquerOps_BO2.big`. Also a CLI
to list/export BO2 sounds, images and fonts. `fonts.rs` converts BO2's UI fonts. Depends on `zhbo-formats`, `zhbo-requirements`, and
`zhbo-core`. After Phase 2.5 the module is split into `textures.rs` (the texture pipeline),
`ini.rs` (the INI rewriters), `viewmodels.rs` (the viewmodel conversion), `generals.rs`
(the Zero Hour names, factions and the `files()` orchestrator), `camo.rs` (the camo art and the
`.zcm` recipes for every `WeaponCamo` a def or attachment unique points at) and `paks.rs` (pack-file access, the shrinker
and the zone loader).

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
starting the game. Depends on `zhbo-requirements` and `zhbo-assets`.

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
3. `zhbo-assets` may depend on `zhbo-core`, `zhbo-formats`, `zhbo-native` and `zhbo-conformance`.
   `zhbo-conformance` depends on `zhbo-formats` only — never on the core or the converter (the
   expected side must not share the mod's rules).
4. `zhbo-launcher` may depend on `zhbo-requirements` and `zhbo-assets`.
5. Engine code (`host/GeneralsGameCode`) is never modified; changes go through
   `host/patches/*.patch`. The bridge (`host/zhbo/*.cpp`) is the only engine-side mod code
   and it owns the ABI in `host/zhbo/zhbo_core.h`.
6. Nothing from either game is shipped; converted data stays local (see
   [CONTRACT.md](CONTRACT.md) "Asset rule").
