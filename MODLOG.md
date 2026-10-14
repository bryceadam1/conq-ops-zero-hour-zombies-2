# MODLOG - Conquer Ops (Zero Hour × Black Ops II)

Working journal. Facts, paths, versions, dead ends, next steps.

## Setup facts (2026-10-13)

* Machine: Windows 11 ARM64 (build 22000) under Parallels on macOS. x86 games run emulated.
  Documents is redirected to `C:\Mac\Home\Documents`.
* Zero Hour: Steam app **2732960**, `C:\Program Files (x86)\Steam\steamapps\common\Command & Conquer Generals - Zero Hour`.
  Retail `Generals.exe` + GenTool (`d3d8.dll`). Original Generals data is in `ZH_Generals\` (INI.big, W3D.big, ...).
  Registry `HKLM\SOFTWARE\WOW6432Node\Electronic Arts\EA Games\Generals\InstallPath` is **stale**
  (points to a non-existent `C:\Program Files\EA Games\...`).
* User data: `C:\Mac\Home\Documents\Command and Conquer Generals Zero Hour Data\`. Backed up with
  `um backup create ... --name zh-userdata` → `~\.universal-modder\backups\zh-userdata\20261013-165403.zip`.
  Restore: `um backup restore zh-userdata`.
* Black Ops II: Steam apps 202970 (SP) / 202990 (MP) / 212910 (ZM), one folder
  `...\common\Call of Duty Black Ops II`: `t6sp.exe`, `t6mp.exe`, `t6zm.exe`, `zone\all\*.ff` (153 files),
  `sound\*.sabs|*.sabl` (137 banks). BO2 is **never launched or hooked** (MP has VAC); only its files are read.
* Engine source: TheSuperHackers GeneralsGameCode @ `02b5b56e22fa6a1ca06a3fc2f7648e646e9f56cb` (2026-10-13), GPL-3.0.
  Their AI policy forbids autonomous-agent PRs upstream - this mod never submits upstream.
* Since 2026-10-14 the engine is **bobtista's fork** @ `4297a8e1fe2a53ce03984424e71d04694d7c5c7a` (submodule
  `host/GeneralsGameCode`): `IRenderBackend` (D3D8 or bgfx), SDL3, OpenAL, 64-bit presets. The DX8
  build (`cargo xtask build`) still passes the autotest 7/7 on it.
* Renderer (2026-10-14): `crates/zhbo-gfx` implements the bgfx API subset the fork's `BgfxBackend`
  uses on wgpu 30.0.1 (see `docs/RENDERER.md`); `cargo xtask build --wgpu` builds the 64-bit SDL3 game
  on it. bgfx headers pinned at b2a7d94d574c6b82b54b6e8631cb6faafe807112 (via bgfx.cmake c480227).
* Toolchain installed: rustup (aarch64 host, + `i686-pc-windows-msvc`), VS 2022 Build Tools 14.44 with
  x86/x64, **ARM64** (for Rust build scripts), ATL, MFC (`afxres.h` for RTS.RC), CMake 3.31, Ninja.
  `vcvarsarm64_x86.bat` = native ARM64 compilers emitting x86 → fast builds.
* `um` (universal-modder CLI) installed with `uv tool install`; `um win setup` done (gfxcapture ffmpeg).
* WinDbg/cdb installed (`Microsoft.WinDbg` package, `x86\cdb.exe` inside WindowsApps).

## Route

Rebuilt guest mechanic inside a real host (see `docs/CONTRACT.md`). Zero Hour is rebuilt from the
SuperHackers source with ~110 lines of hooks; BO2's first-person rules are reimplemented in Rust
(`crates/zhbo-core`). Running BO2 alongside was rejected: its MP client is VAC-protected, and its world
has nothing to do with ZH maps, so a live passthrough would only add latency and risk.

## Gotchas found

1. The engine sets its working directory to the exe's folder unless `-useCwd` is passed. Running a
   separately-built `generalszh.exe` against the Steam data needs `cwd = game dir` **and** `-useCwd`.
   Symptom without it: "Uncaught exception during initialization" right after the EXE CRC log line.
2. Stale Generals `InstallPath` → base Generals `.big`s not loaded. Patched `Win32BIGFileSystem::init` to
   fall back to `ZH_Generals\`.
3. `-mod <dir>` only loads `*.big` files from that dir (no loose files) → `cargo xtask pack` builds
   `ConquerOps.big`.
4. `INI::loadFileDirectory("Data\INI\X")` loads `X.ini` **and** every `Data\INI\X\*.ini`, so the mod adds
   files instead of replacing stock INI. Objects load in sorted order → our file is `ZZZ_ZHBO_General.ini`.
5. `ObjectReskin` only accepts visual fields; cost/command set/prereqs need a full `Object`.
6. `WeaponTemplate::fireWeaponTemplate` dereferences `firingWeapon` unconditionally → the bridge
   allocates one `Weapon` per FX template with `TheWeaponStore->allocateNewWeapon`.
7. `W3DView`'s near clip plane is `MAP_XY_FACTOR` (10) = the General's eye height → set 0.5 in first person.
8. Windows installer detection: any exe with "install" in its name requires elevation (os error 740) -
   the requirements crate is `zhbo-requirements`, not `zhbo-install`.
9. Rust `windows-sys` makes the staticlib depend on `windows.0.52.0.lib`, which the C++ link can't
   find → declare the few Win32 functions directly (`advapi32`, `user32`).
10. Rust build scripts compile for the ARM64 host → MSVC ARM64 tools component is required.
11. BO2 `.sabs`/`.sabl` entries are `{id, size, offset, frames, rate_idx, channels, loop, format}`.
    `.sabs` (streamed, long) = format 8, plain FLAC. `.sabl` (loaded, short effects incl. gunshots) =
    format 0, raw 16-bit PCM. Names are hashes only; the name tables are in the encrypted fastfiles,
    which we deliberately do not decrypt.
12. Objects must be created inside `GameLogic::update`. W3DModelDraw resolves weapon barrel bones only
    while the logic updates; an object spawned from the client update gets a zero-sized recoil vector
    and crashes in `W3DModelDraw::handleWeaponFireFX` the first time it fires.
13. The VM runs at 200% DPI: a 1280x720 game window reports a 640x360 client rect, so `um win drive`
    click coordinates equal the half-scale screenshot coordinates.
14. The RTS camera never shows sky, and `new_skybox` is drawn only by the water renderer, so first
    person paints its own gradient before the 3D pass (which clears depth but not colour).
15. After the camera override, reset pitch/zoom/pivot (`resetPivotToGround`) or the RTS camera stays low.
16. GLA rebels spawned next to the General preferred to shoot the nearby base; ordered onto the
    General they hit it (5 dmg/round) and the HUD's damage flash and regen engage.

17. BO2 `.ipak` image packs (`zone\all\*.ipak`) are LZO1X-compressed, **not encrypted**: `KAPI` v0x50000,
    sections {type 1 index, type 2 data}, 16-byte index entries {name hash, data hash, offset, size},
    data in 0x80-aligned blocks with up to 31 commands (`size | flag<<24`, flag 1 = LZO1X). Each entry
    is a whole `IWi` v27 image: 0x40 header (format, w, h, mip end offsets at 0x20), mips smallest
    first. Formats: 1 ARGB32, 11 DXT1, 12 DXT3, 13 DXT5, 14 BC5. Names are hashes; art identified by eye
    with `zhbo-assets sheets`.
18. Real BO2 HUD art found in `mp.ipak`: slanted numerals 0-9 and "/" (32x64), red-dot glow, cyan lock
    bracket, blood splatter (hashes in `crates/zhbo-assets/src/hud.rs`). The small glyph atlas in
    `base.ipak` is a font page, but its metrics are in the fastfiles, so text uses Bahnschrift.
19. Zero Hour `MappedImage` `Coords` are inclusive; extra files in `Data\INI\MappedImages\HandCreated\`
    load automatically.
20. MinGW: the Rust core for `i686-pc-windows-gnu` needs only the same system libs as MSVC
    (panic=abort). MSYS2's MinGW has prefixed compilers but unprefixed binutils.
21. The fork rejected three hunks of the original hook patch (W3DDisplay draw hooks moved inside a
    `GGC_RPROFILE` block, W3DView include, GameLogic step); re-anchored by hand. xtask now pins the
    engine to the submodule's checked-out HEAD so a bumped submodule builds before it is committed.
22. bgfx shaders through naga's GLSL frontend need: `mtxFromCols/Rows` macros, `#define` constants as
    uniform array sizes, and helper functions taking `sampler2D` split into `texture2D` + `sampler`
    (Vulkan GLSL has no combined-sampler parameters). Vertex and fragment uniform blocks must use
    different bindings (their layouts differ).
23. In this VM wgpu sees two adapters: Parallels' GL driver on the M1 (no BC/DXT sampling, no compute,
    no texture view formats) and D3D12 WARP (software). Default limits fail on the GL one ("max compute
    workgroups ... better than allowed 0"), so the renderer requests the adapter's own limits, decodes
    BC1-3 on the CPU and creates sRGB textures as sRGB outright when views can't reinterpret.
24. wgpu loses the whole frame if one draw fails validation, so draws are range-checked before
    encoding and pipelines are built in an error scope.
25. Rust's x86_64-msvc staticlib with wgpu adds only `opengl32` to the native libs (D3D12/DXGI are
    loaded through `windows-link` raw-dylib).
26. The fork's `win64` preset needs FFmpeg even with `RTS_BUILD_OPTION_FFMPEG=OFF`, because OpenAL
    audio decodes with it. The preset sets `FFMPEG_ROOT=<engine>/.deps-cache/ffmpeg-x64`, overriding
    the environment. bgfx.cmake's FetchContent patch step runs `sh`, which a plain cmd lacks.
27. Engine defines such as `SAGE_USE_SDL3` live on the `core_config` interface target. A glue
    target that doesn't link it silently compiles the non-SDL branch.
28. In the bgfx backend, 2D draws go to the UI view (last), so a sky painted with `drawFillRect`
    before the 3D pass hides the world. The wgpu build uses the scene view's clear colour.
29. Unaligned dynamic index updates (`write_buffer` offset 438) and failed R32F render targets
    made buildings vanish in RTS and the whole world vanish in first person. Both were silent in
    the game; only `zhbo-gfx.log` showed them.
30. The fork's SDL builds use `StdBIGFileSystem`, not `Win32BIGFileSystem`, so the `ZH_Generals`
    fallback in patch 0001 only matters for the DX8 build. `StdBIGFileSystem` finds base Generals
    by itself: it checks `CNC_GENERALS_INSTALLPATH`, then the registry, then searches relative paths.
31. The autopilot replaces all input inside the Rust core, so `autotest.ps1` never exercises the
    OS input path. `ZHBO_AUTOPILOT=manual` sets up the scene without scripted input, and
    `scripts/inputtest.ps1` drives real keys and mouse through it. `um win drive "hold"` needs a hex
    VK code (`hold 0x57 2500`), and SDL needs `scanmode on`.
32. macOS first build: wgpu 30 needs a recent Rust (1.76 fails; 1.99 works). The only compile
    error was a leftover `ExitProcess` in `ZhboBridge.cpp`, now `zhbo_platform::exitProcess`.
33. xtask's `copy_dir_if_changed` used `fs::write`, which dropped the execute bit on `generalszh`
    when copying the stage into dist. It now copies permissions as well.
34. On macOS/Linux, the launcher's error path pops an `osascript`/`zenity` dialog and waits for it,
    so `conquerops --check` looked hung when the games were missing. `--check` now only prints.
35. BO2's HUD art is in `zone/all/mp.ipak`, which ships with BO2 **Multiplayer** (Steam app 202990),
    not the campaign app (202970). Without it the launcher logs "could not read Black Ops II art"
    and the HUD uses its drawn fallbacks. SteamCMD must fetch both apps into the same folder.
36. The fork's SDL3 entry point (`SDL3Main.cpp`) never called `initMemoryManager()`, so with the null
    memory manager `TheDynamicMemoryAllocator` stayed null and a `--log` build asserted on every
    string allocation (1.17 million times, 151 MB of log, in 25 s). Patch 0003 calls it.
37. Retina: the fork creates the SDL window without `SDL_WINDOW_HIGH_PIXEL_DENSITY` and sizes the
    swap chain from `SDL_GetWindowSize` (points), so `-xres 1280` renders 1280x720 and macOS
    scales it 2x. Same as the fork's own renderer and as Windows at 200%; full-resolution Retina
    is a separate feature (window flag, pixel swap size, mouse and UI mapping).
38. macOS tests skip the menus with the fork's `-loadsave` (made once with `-saveatframe 90 -saveto`).
    `ConquerOps.log` survives between runs, so delete it before launching, or a wait for "entered
    first-person" matches the previous run and input is sent during the loading screen.
39. BO2 fastfiles (`TAff0100`, version 147) decrypt with `zhbo_formats::fastfile` (format from
    OpenAssetTools, GPL-3.0): 4 round-robin chunk streams, Salsa20 with an IV ring seeded by the zone
    name and advanced by each chunk's SHA-1, then raw deflate. The zone's declared size excludes
    the 8-byte sizes and the 32-byte block-size table. All 115 zones in `zone/all` decrypt.
    MP characters are per map (`mp_raid.ff`: `c_usa_mp_fbi_assault_fb` bodies,
    `c_usa_mp_fbi_longsleeve_viewhands` arms); guns (`t6_wpn_*_view`/`_world`), `viewmodel_*` and
    third-person player animations are in `common_mp.ff` (481 models, 4233 anims).
40. MP character models are in the faction zones (`faction_fbi_mp.ff`, `faction_seals_mp.ff`, ...);
    a map zone only lists their names. `zhbo_formats::xmodel::find` reads a model by finding its
    name right after its 248-byte struct, so no full zone walk is needed. Shared ("reusable") data
    that an earlier asset already loaded is a back-reference and is not resolved yet. Verified with
    rendered previews (`zhbo-assets model <ff> <name> --png`): FBI body (102 bones, 15000 tris),
    FBI viewhands (72 bones), AN-94 viewmodel. Rigid and blended vertices are in model space; a
    viewmodel's bind pose parks its magazine on `tag_clip1` behind the stock, and animation moves it.
41. `zhbo_formats::xanim` reads XAnimParts the same way (104-byte struct, then names, notifies,
    then the byte/short/int/random pools and indices) and decodes bone tracks; every pool must be
    consumed exactly, which catches layout mistakes. Animations with a delta (root motion) part
    are refused for now. `zhbo-assets pose` skins models with a frame: arms + AN-94 (gun root
    `j_gun` hung on the arms' `tag_weapon`) posed by `viewmodel_an94_reload` look right. Anim
    values are full local transforms (replacing the bind local); unanimated parts keep the bind.
42. Models share data across assets (skeletons, materials, images, even vertex buffers): those
    pointers are back-references `(block << 29 | offset) + 1` into the game's zone memory, so only
    a full zone load resolves them. `zhbo_formats::t6zone` is that loader: per-asset loaders
    generated by OpenAssetTools' ZoneCodeGenerator through a Rust template (`tools/zonecode/`),
    plus a runtime mirroring OAT's ZoneInputStream. All 115 zones load with 0 bytes left over
    (`zhbo-assets walk`). Model/material/anim readers read the loaded memory.
43. Textures: a material's image is found in the `.ipak` packs by `streamedParts[0].hash`
    (`GfxImage` + 40), not `GfxImage::hash`. MP character and weapon textures are in `base.ipak`.
44. A viewmodel's in-gun magazine is a separate attachment model (`t6_attach_mag_an94_view`, root
    bone `tag_clip`) hung on the gun's `j_gun`; `tag_clip1` is the spare used by reloads, which the
    idle animation parks out of view below the camera.
45. W3D (from the engine source): skin vertices are stored relative to their bone's bind transform
    (drawn at `bone * vertex`), one bone per vertex; pivots are `parent * base * T(t) * R(q)`, so
    animation channels are deltas from the base pose; texture V is stored flipped; names are 15
    characters. Animation `SKL.ANIM` loads from `Art/W3D/ANIM.w3d`.
46. BO2 animation translations are offsets added to the bone's local offset; rotations replace it
    (`pb_death_faceplant` keys `j_mainroot` z from -5.8" to -32.3"). Treating translations as
    absolute sank the General to the hips. Viewhands have zero local offsets, so there both
    readings agree.
47. The General's BO2 art is generated into `ConquerOps_BO2.big` with a same-path
    `ZZZ_ZHBO_General.ini`; mod archives load in name order with overwrite, so it replaces
    `ConquerOps.big`'s Burton fallback. Bump `hud::VERSION` whenever converter output changes,
    or launchers keep the old archive.
48. `ZHBO_AUTOPILOT=showcase` (with `ZHBO_SHOWCASE_OBJECT/_ZOOM/_PITCH/_ANGLE`) films a General close
    up: idle, a walk out and back, death. `scripts/autotest-mac.sh --showcase [object]`.
49. An attached model (a gun on `tag_weapon`, its magazine on `j_gun`) must have *all* its bones
    placed relative to the tag, not only its root; otherwise child bones (barrel, sights, flash)
    float at their model-space positions. `zhbo-assets posew3d` renders converted data through the
    engine's pivot formula, which is how this was found.
50. The first-person viewmodel is a W3D render object added straight to `W3DDisplay::m_3DScene`
    (`host/zhbo/ZhboViewmodel.cpp`, in z_gameenginedevice; hook in patch 0004), posed with
    `Set_Animation(..., ANIM_MODE_MANUAL)` at the clip and time zhbo-core picks
    (`zhbo_core::viewmodel`, ClientOut ABI 3). Aiming slides it so `tag_sights` is on the view axis.

## Verification log

* Baseline source build (no mod) runs against Steam data with `-useCwd -win`: intro video plays (screenshot).
* `cargo test --workspace`: 41 tests (movement, weapons, hitscan, sim, client, HUD, formats, requirements).
* Requirements check on this machine: ZH found via Steam app 2732960, BO2 via Steam app 202970.
* Patched engine + Rust core: compiles and links.
* `scripts/autotest.ps1` (skirmish, Alpine Assault vs. Medium AI): 7/7 PASS - entered first person,
  moved, killed a rebel, opened the field computer, returned to RTS, re-entered, took enemy fire.
  Screenshots confirm eye-level camera, HUD (compass, health/stance, ammo, reload), sky, RTS hand-back.
* `zhbo-assets` decodes real BO2 banks: 34 short MP effects exported from `mpl_common.all.sabl` as valid WAV.
* BO2 HUD (2026-10-14): launcher built `ConquerOps_BO2.big` with 14/14 sprites; autotest 7/7 PASS;
  screenshots show BO2 numerals ("22 / 120", amber when low), "+100" kill popup, red-dot reticle,
  blood overlay, minimap, funds panel, Bahnschrift text.
* Cross-platform: all crates type-check for x86_64-unknown-linux-gnu and aarch64-apple-darwin; core
  builds for i686-pc-windows-gnu; discovery unit-tested against a simulated CrossOver bottle.
* Re-base onto bobtista's fork (2026-10-14): DX8 build, autotest 7/7 PASS.
* **Platform layer and native build path (2026-10-14)**: the bridge's OS calls moved to
  `host/zhbo/ZhboPlatform.cpp` (SDL3 or Win32). The launcher starts the native game directly, and
  the Wine/Proton runner is gone. `cargo xtask build` on macOS/Linux uses the fork's presets and
  deploy scripts. On Windows, both builds pass `autotest.ps1` (7/7) and `inputtest.ps1` (W moves
  the General about 77-81 units; the mouse turns the view). The macOS/Linux build has not been
  run yet.
* **wgpu build in game (2026-10-14)**: `cargo xtask build --wgpu` gives a 64-bit SDL3 `generalszh.exe`
  on the Rust renderer. It runs here on Parallels' GL adapter (M1). The intro video, shell map (3D
  water, battleship, particles) and menus render. The autotest passes 7/7, and screenshots show the
  RTS view (terrain, base, units, minimap, shroud) and the first-person world (terrain, trees,
  depot, enemy infantry, HUD) matching the DX8 build. No wgpu errors in `zhbo-gfx.log`.
* zhbo-gfx (2026-10-14): all 27 engine shader variants translate, validate and emit WGSL;
  `tests/headless.rs` draws a frame (transient + static buffers, BC1 + R5G6B5 textures, model
  transform, partial clear, alpha blend) and checks read-back pixels - PASS on D3D12 WARP and on the
  Parallels GL adapter (M1, CPU BC decode).
* **Native macOS build (2026-10-14)**: `cargo xtask build --log` configures, compiles, links and
  stages on macOS 15.7 (M1 Pro, arm64), with the dylibs bundled into dist. `cargo test --workspace`
  passes 73 tests, the headless renderer test included, now on Metal.
* **macOS in game (2026-10-14)**: the renderer runs on `Apple M1 Pro (Metal)` with no CPU BC decode,
  no R32F substitutes and no wgpu errors, at a steady 30 FPS. The intro video, RTS view and first
  person (world, BO2 HUD with `DIN Alternate`, all 14 BO2 HUD images) render. `scripts/autotest-mac.sh`
  passes 7/7, and `--input` passes (real W reaches the game, the General moves, the mouse turns the view).
  In this save the General dies at frame ~496 before re-entry, so the "re-entered" check only
  proves the line is logged.
* **BO2 Generals in game (2026-10-14)**: the three Generals are BO2 MP soldiers converted at launch
  from the player's install (USA SEAL Team 6, China PLA, GLA PMC; 18-23 meshes each with the
  AN-94, 30 textures, one shared hierarchy, idle/run/aim/2 deaths from `pb_*`). Showcase captures
  show them standing, running and dying correctly; the autopilot test still passes 7/7.
* **First-person viewmodel (2026-10-14)**: each faction's BO2 arms hold the AN-94 with its
  magazine, playing BO2's idle, fire, reload, empty reload, sprint and raise clips. In game the
  autopilot shows hip fire, a reload and aiming through the iron sights; 7/7 still pass.

## Next steps

**Continuing on macOS: start at `docs/HANDOFF-MAC.md`.** It covers state, setup, the expected
first-build problems, verification and the task list. The list below is the older gameplay backlog.

1. Manual playtest by a human with real mouse/keyboard (the autotest injects input inside the mod).
2. Weapon viewmodel (a gun in hand), muzzle flash near the camera, hit/kill sounds.
3. BO2 audio: audition exported bank entries, map ids to weapon slots, and have a converter build a
   private `ConquerOps_BO2.big` with AudioEvents that override the ZH fallbacks.
4. Save/load of first-person state; two-client multiplayer test.
5. Strike Force (`so_rts_*`) content as a natural next theme.
