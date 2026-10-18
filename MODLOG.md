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
(promoted into docs/CONTRACT.md "Engine gotchas")
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
51. BO2 and Zero Hour use opposite front-face winding. Copying BO2's triangle order made the engine
    cull the outside of every converted model, so they looked inside-out; the converter now swaps
    the last two indices of each triangle. My software preview draws both sides, so it can't
    catch this; check winding in game.
52. BO2 sound aliases live in each zone's `SndBank` asset (`zhbo_formats::sndalias`; SndAliasList
    is 20 bytes, SndAlias 96, since its flags field is 8 bytes). An alias variant's `assetId` can point
    into *any* `sound/*.sabl|.sabs`, not just the bank's namesake: most player sounds are in
    `cmn_root.all.sabl`, so the converter indexes every bank by id.
(promoted into docs/CONTRACT.md "Engine gotchas")
54. The first-person General's hidden third-person body still played its FX weapon's FireFX (a
    length-10 tracer and a size-10 lens flare) right at the camera: a white sheet over the screen
    when firing. `cbFireFx` now skips the local first-person General. This is lockstep-safe because the
    `*_FX` weapons deal no damage and have no scatter, so `fireWeaponTemplate` only plays client-side
    FX and sound for them. The viewmodel plays its own BO2 muzzle flash at `tag_flash`.
55. Shine (superseded by 74): opaque BO2 surfaces used to get their specular map baked into the colour
    map's alpha and a generated environment map on stage 1 (`DETAILCOLOR_MODALPHAADDCOLOR`, the
    world-space environment mapper). BO2's reflection probes are cube maps whose pixels sit in the
    zone's temp block, which the loader reuses, so they are not kept.

56. The field computer is a BO2 scorestreak. The tablet is its own viewmodel (`ZHBO_VMxxx_TAB`: arms plus
    `t6_wpn_tablet_view` on `tag_weapon`, hierarchy `ZHBO_VMT_SKL`), because the gun and the tablet
    both root at `j_gun`. The host swaps models by clip (`ZHBO_VM_TABLET_*`, ABI 6) and loads only
    the clips of the current model's hierarchy. The screen material layers three images in a BO2
    shader; the converter composites them into one self-lit texture and must zero its alpha, or the
    reflection stage washes it out. BO2 has no wrist-datapad animations in `common_mp`; the
    `viewmodel_pda_hacker_*` clips are Black Hat's.

57. BO2 HUD art is now found by *material* name in the decrypted zones (`zhbo-assets material <ff> <dir>
    <name>...` exports a material's images, also over dark grey). Most HUD materials are in
    `common_mp` (`compassping_*`, `hud_horizontal_compass_minimap`, `hud_mp_vis_*`, `damage_feedback`),
    and the in-game streak icons are `hud_ks_*_big` in `code_post_gfx_mp`. The `ui_mp` menu icons keep
    their pixels in the zone (lost with the temp block). Art BO2 blends additively (bright on opaque
    black) gets brightness as alpha. Zero Hour's faction logos are `USA_Logo`/`China_Logo`/`GLA_Logo`
    (by `PlayerTemplate::getBaseSide()`). `Display::setClipRegion` clips `drawImage` with UV
    adjustment, which the scrolling compass uses (ClientApi `clip`, ABI 7; ObjInfo gains the owner's
    colour).

58. Tab (BO2's scoreboard key) holds up Zero Hour's own radar as the whole-map view: ClientApi `radar`
    (ABI 8) calls `TheRadar->draw` under the control bar's rule (`rts::localPlayerHasRadar`), and
    returns the General's spot via `worldToRadar` + `findDrawPositions` for BO2's player bracket.
    The tactical-view box collapses to a stray line in first person, so patch 0005 adds
    `Radar::setDrawViewBox`. A long red line on it is Zero Hour's "under attack" radar event
    shrinking onto its spot, not a bug. `ZHBO_FORCE_RADAR=1` gives the autopilot player radar.
    The SDL key reader maps Tab (0x09).

59. Struct layouts: `tools/zonecode/RustLayoutTemplate.cpp` (preset `layoutrust`) prints every T6
    struct's size and field offsets into `crates/zhbo-formats/src/t6zone/layouts.rs`, so assets' plain
    fields are read by name (it agrees with the hand-measured SndAlias 96 / SndAliasList 20 / GfxImage 80).
60. BO2 weapons (`zhbo_formats::weapon`): every MP weapon is a `WeaponVariantDef` in `common_mp`.
    `gunXModel` and `worldModel` are arrays of pointers. Gun start/max ammo count magazines even when
    `ammoCountClipRelative` is false (AN-94 4/8 = 30/90, 210 max). The damage curve has six slots;
    unused middle slots have range 0 and the last is the minimum damage beyond the curve.
    `adsMoveSpeedScale` multiplies a base ADS walk (rifles 1, LMGs 0.8, SMGs/pistols/shotguns 2).
    Base weapons don't link their magazine model (match `t6_attach_mag_<part>_view` to the view
    model name). Some `common_mp` models are empty stubs that `patch_mp` replaces (the frag's view
    model): look in `patch_mp` first. `knife_mp` points at animations no longer shipped; quick
    melee uses `knife_held_mp`. HUD icons are materials inline in the weapon (follow `hudIcon`).
    English names are in `zone/english/en_code_post_gfx_mp.ff` (11,300 strings) with `en_patch_mp`
    over it; three grenades' keys differ (`WEAPON_FLASHBANG`, `WEAPON_EMPGRENADE`,
    `WEAPON_STUN_GRENADE`). BO2's perk descriptions come from the same strings; their tuning
    numbers are set in BO2 script, so `perk_values` uses documented defaults.
61. The hand-picked hash for the HUD numeral 9 was wrong (every "90" showed as "0"); numerals now
    come by material name (`hud_mp_num_big_*`).
62. Weapons in the core (ABI 9): `weapons.rs` holds the catalog (the converted table, or a built-in
    stand-in), loadout, perks and arsenal; `projectiles.rs` flies grenades and rockets (bounce,
    stick, cook, fuse, blast, stun = `DISABLED_PARALYZED`, EMP = `DISABLED_EMP`). Viewmodel clips
    are BO2's `weapAnimFiles_t` slots (`ZVnn_SKL.ZVnn_Ann`). `conformance::check` measures a
    weapon against its definition; `generals::tests::every_mp_weapon_converts_and_plays_like_bo2`
    runs it on all 59 player weapons in the install, with every viewmodel and animation converted.
63. Attachments (`zhbo_formats::weapon::attachments`, `zhbo_core::attachments`): each weapon
    lists the `WeaponAttachment` variant it uses per kind (`WeaponVariantDef::attachments`, an
    array of 30 pointers by `eAttachment`); `iAttachments` is the allowed set. Every attachment's
    effect is a scale on the weapon (a scale of 1 is no change; `fAdsZoomFov1` of 1 means no zoom).
    Select fire, the grenade launcher and the second optic are alt weapons of their own
    (`sf_an94_mp`, `gl_an94_mp`: inventory `ALTMODE`); the launcher keeps its own grenades, the others
    share the magazine. Art comes from `WeaponAttachmentUnique` (`au_<weapon>_<attachment>`): model,
    offset/rotation from the weapon's origin or its `viewModelTag`, the weapon's bones to hide
    (`hideTags`: iron sights under an optic), the suppressed gunshot, and a full animation set (the
    combination's entry if there is one, else the attachment with the most). Optics have one bone at
    their base: `tag_sights` is moved up to the middle of the optic for aiming. Alt-mode clips are
    the BO2 slot + 0x80 on the weapon's viewmodel. BO2 counts spare ammo in magazines, so extended
    mags carry more spare rounds; the tri-bolt fires all three bolts with one pull.
64. Third person (ABI 10): every converted weapon's world model is on each soldier model as
    `W<vm><k>` sub-objects rigid on `tag_weapon_right` (attachments at their world offsets), the
    tablet as `TB<k>` on `tag_inhand` (BO2's tablet animations carry it there). Every condition
    state hides all but the primary (lines of 80 names: Zero Hour reads INI lines up to ~1 KB, and
    `HideSubObject` lines add up); the bridge's `showSubObject` for the weapon in hand wins over
    them. USER_1 / USER_2 play `pt_tablet_stand_raise` / `_putaway`.
65. Equipment (`zhbo_core::equipment`): planted (`Place::Plant`) or thrown pieces that land
    (`Outcome::Landed`) stay as a Zero Hour object; relationships are asked through that object, so
    they work after the owner dies. Black Hat captures use `Object::defect` for units and `setTeam`
    for inert objects (LogicApi `capture`). Fills: `objflags::DRONE`, `PROJECTILE` (the Trophy
    System), `FIRING` (minimap), and `SHROUDED` (client queries: the minimap follows Zero Hour's radar
    rule, except for Sensor Grenades and enemies firing).
66. BO2 effects (`zhbo_formats::fx`): MP grenades and rockets have no `projExplosionEffect`; their
    blast is the impact table (`impactfx`) entry for their `impactType`, but the table's order
    differs from the enum (grenade 7 → entry 12 `explosions/fx_grenadeexp_dirt`, rocket 9 → 13).
    Runner elements name their child effect. Element kinds: 0-2 sprites, 3-6 tails/lines/clouds, 7
    model, 8-9 lights, 10 sound, 11 decal, 12 runner. Velocities are inches per millisecond, gravity a
    fraction of 800 in/s², sizes centre-to-edge. Some HUD icons are empty stubs in `common_mp`; the
    real material is in `code_post_gfx_mp`. Viewmodel foley is mostly in `mpl_common`, gear rustle
    only in the map's bank; the `*_futz` notetracks have no sound in any MP bank.
67. The General under RTS control (ABI 11): weapon slots PRIMARY rifle, SECONDARY pistol,
    TERTIARY lethal, switched like the Ranger (`SWITCH_WEAPON`, `CHECK_LIKE`; auto-choose leaves out
    `FROM_PLAYER` so the lock decides; pistol and lethal auto-choose `NONE`). The bridge counts shots
    each logic frame by `Weapon::getLastShotFrame` changes and reports them with the current slot
    (`zhbo_logic_rts_update`); the core charges the same arsenal first person uses and returns a
    slot to lock to (the weapon in hand after the field computer, or one with ammo when the current
    one runs dry). An empty weapon can't fire (`setPossibleNextShotFrame` pushed ahead each frame)
    and patch 0006 restricts its button. The ammo is drawn over the buttons (found by the window
    names `ControlBar.wnd:ButtonCommandNN`, `GadgetButtonGetData` is the `CommandButton`). Command
    set slots are column-major: odd numbers are the top row. A later `MappedImage` of the same name
    re-parses into the earlier one, so `data/` ships Zero Hour stand-ins (`ZHBO_A_Buttons.ini`, which
    sorts before `ZHBO_BO2.ini`) that the BO2 archive's composed button tiles replace. Objects
    spawned by a test script must be made in the logic step (`rtsAutopilotWave`).
68. `XModel::trans` (bone offsets from the parent) is packed at 12 bytes per bone, not 16: read at
    16 every bone after the first was shifted by a float, so animated translations started from the
    wrong place (the reload's new magazine floated). BO2 reloads use two magazines: `tag_clip` (in
    the gun, dropped) and `tag_clip1` (in the hand, off screen at idle); both get one.
69. Optics: the reticle surface is left out, because BO2 projects the reticle to infinity, which a
    surface on the glass can't do; the HUD draws BO2's red-dot glow on the line of sight instead.
    The glass (`*_lens`) blends as BO2 draws it (73). The sight
    line sits at 72% of the optic's height (its window, above the mount).
70. Smoke (ABI 12, LogicApi `smoke`): BO2's Willy Pete names no effect (its script plays
    `weapon/grenade/fx_smoke_willy_pete`); looping effect parts get a finite `SystemLifetime` (0 is
    forever). For 15 s anything attacking into or out of the cloud goes back to idle.
71. Camera transitions blend the field of view (ClientIn `rts_hfov`, `View::getFieldOfView`) and
    end on the overhead camera's own pose (its direction, distance and height, recentred on the
    General); entering swings round behind the General, then moves in, and leaving plays the same
    move backwards (`transition_pose`), with the General hidden while the camera is in the head. Relative mouse mode leaves
    Zero Hour's mouse "outside the window" until a click: releasing it now says the cursor is back.
72. Loading a BO2 viewmodel took about a second, on the render thread, the first time each was shown
    (entering first person, each first switch, first equipment use). Nearly all of it was the bgfx
    port's coplanar-triangle scan in `MeshModelClass::post_process`: it tested every pair of
    triangles and renormalized one plane per pair, O(n^2) on meshes of tens of thousands of
    triangles. Patch 0008 gives the same answer in O(n log n) (planes flipped to a canonical side
    and sorted by distance; only neighbours within the 0.001 tolerance are tested): 3-4 ms a model.
    Patch 0007 also loads the mod's objects and the local General's viewmodels and clips on the
    loading screen (`Zhbo_Preload`, about 0.4 s). Retail never preloads: `KINDOF_PRELOAD` only works
    with `-preload`. `ConquerOps.log` lists frames over 60 ms as `hitch:`.
73. Blending: each BO2 material's render state (`GfxStateBits::loadBits[0]` of its lit technique,
    or the unlit/emissive one for glows: bits 0-3 source and 4-7 destination blend, bit 11 alpha
    test off) says how it draws (`xmodel::Material::blend`). The converter writes the same W3D
    shader (`w3d::Blend`: opaque, alpha test, alpha, premultiplied, additive; blended ones don't write
    depth, and Zero Hour sorts any mesh whose shader blends) and keeps the colour map's own alpha for
    non-opaque surfaces (`*_a.tga`). The tablet's buttons are an additive glow (they drew as black
    panels), optic glass and watch glass mix by alpha. The clan tag and emblem surfaces stay out: BO2 fills them
    from the player's tag and emblem, and drawn as is they show the whole font atlas. BO2
    effects take additive or alpha from their material too (the "no alpha means additive" guess is
    only the fallback).
74. BO2 shading (host patch 0009): every opaque BO2 surface gets its own maps packed into one more
    texture on stage 1 (`*_nsg.tga`: BC5 normal X and Y, the specular map's alpha as gloss, its
    luminance as specular level; `generals::surface_maps`) and its occlusion map in the colour map's
    alpha. The backend flags a draw whose stage 1 is such a texture (name ends `_nsg.`) with its colour
    op off (`u_material[25]`), and `fs_uber`'s `bo2Shade` lights it: per-pixel normals with the tangent
    frame from screen-space derivatives (W3D meshes carry no tangents), a normalized Blinn-Phong
    highlight whose power comes from gloss, Schlick Fresnel, and a reflection of a sky and ground
    built from the scene's lights. It takes Zero Hour's material terms and its per-channel clamp of
    diffuse light, but keeps only 35% of the lights' colour: Zero Hour's accent lights are violet
    and blue, which tinted the lifted BO2 surfaces violet (BO2 lights near neutral). In first person the
    screen gets bloom, a slightly cool grade and a vignette (`setBo2Look`; the player's settings come
    back overhead), and the scene renders in HDR throughout so highlights roll off and bloom.
    `BO2_NORMAL_Y` flips the normal map's green if bumps ever look lit from below. `ZHBO_BO2_DEBUG`
    shows one term: -1 BO2 shading off (A/B), 1 flat normals, 2 normals, 3 diffuse light, 4
    specular, 5 reflection.
75. Leaving first person flies back to the overhead camera as first person found it (its offset from
    its pivot, `ClientIn::rts_pivot`, ABI 13), not to what the view reports then (the first-person
    camera). The engine patch no longer resets the zoom and pitch to their defaults afterwards.
76. After relative mouse mode macOS keeps the cursor "outside" (no edge scroll, no selection) until
    a click: the cursor is warped to where it is, and for 3 s Zero Hour's mouse is told it is inside
    and recaptured whenever the system cursor is over the window (`reclaimMouse`). The real cause of
    the dead first click was the window manager: the Take Control button grabs the mouse on the
    press that starts first person, first person swallows the release, and the next click overhead
    went to the button. The grab is cleared whenever first person takes or releases the mouse.
77. The overhead General: a forced switch to the pistol re-issues the attack (Zero Hour drops one
    out of the new weapon's range); the third slot is only the throw (`ZHBO_GeneralGrenade`), and
    zhbo-core throws the BO2 lethal or tactical at the AI's target (`Sim::rts_update`), as first
    person does. The Lethal and Tactical buttons share it; patch 0010 tells the bridge which was
    pressed (MSG_ZHBO_TAKE_CONTROL with an action), patch 0006 lights only the chosen one.
77b. When a gun runs dry with no recent target (between fights), the General takes on the closest
    enemy the empty gun could have reached: the new gun's shorter reach left enemies firing from
    further away unanswered ("he stops firing on the pistol"). `--rts-dry` runs the overhead test
    with no spare rifle rounds, then none for the pistol (`ZHBO_LOADOUT=primary_reserve=0`, a
    loadout key that changes no art, so it doesn't force a reconversion).
78. The soldier model: BO2's torso animations (`pt_*`) layered over stances (`pb_*`) for reloads,
    throws and aimed fire, the pistol stance (JAMMED, set by the bridge while the sidearm is in hand;
    only missiles use it otherwise), a marker bone per gun at its `tag_flash` (`ZF<vm>`) for its
    shots and BO2 world muzzle flash, and a `BOUNDINGBOX` to click (skins ray-test unposed, by the
    feet). BO2 bullet impacts play on what he hits (`rts_fx.txt`: flesh, metal, other).
79. Sorted (translucent) draws replay from the render state captured at insert; bgfx's
    `Set_Index_Buffer_Index_Offset` did not update it (patch 0011), so every translucent mesh in a
    shared vertex buffer read another mesh's vertices: streaks from the Reflex glass to the watch.
80. Aiming lines `tag_sights` up with the eye, and most BO2 viewmodels (every pistol, several rifles
    and snipers) have none: the converter adds one at `tag_sights_on`, or else at the top of the
    front sight (the highest point of the front third of the gun); an optic still moves it to its
    window. Blending toward the ADS fire clip's first frame does not work: BO2's fire clips are not
    whole poses (the arms fold up).

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
* **BO2 feel (2026-10-14)**: BO2 sounds converted at launch: 46 sounds and 14 events (AN-94 and
  B23R shots with their tails, dry fire, hit marker, heartbeat, hurt, footsteps, sprint, landing,
  impacts, other players' shots), plus 4 reload notetrack events. The autopilot log shows each one
  playing. The viewmodel sways, bobs, kicks and dips on landing, with a BO2 muzzle flash and ejected
  brass. Shiny weapon and gear: captures show the sheen on the AN-94's metal and the soldiers'
  armour. 7/7 still pass.
* **Scorestreak tablet (2026-10-14)**: F brings up BO2's tablet (pullout, then a moment on its screen)
  before the camera leaves for the overhead view; taking control again puts it away, then the AN-94
  comes up. Captures show the raise, the lit screen and the putaway; 7/7 pass.
* **BO2 HUD layout (2026-10-14)**: matched to BO2 MP screenshots. Changes: minimap tight top-left with
  BO2's gold player triangle and a compass strip; bottom-left team panel (emblem, funds in BO2
  numerals) with the scorestreak name above it while the tablet is up; bottom-right scorestreak
  column (field computer, gold), the weapon name in a slanted box with its fire mode, and mag/reserve
  at similar sizes. The HUD stays up while the tablet is up, without the crosshair. 7/7 pass.
* **BO2 weapons (2026-10-14)**: AN-94, Tac-45, frag, concussion and the knife play with their BO2
  definitions and viewmodels. The autotest (12/12) swaps, fires the pistol, knifes, cooks and throws
  a frag, throws a concussion, and both go off; captures show each animation and the HUD's
  "Tac-45 SEMI-AUTO 10/30", "9 / 90" and the grenade counters. All 59 player weapons pass the
  every-weapon test (4 s).

* **Attachments, equipment, scorestreaks, BO2 effects (2026-10-14)**: the autotest (15/15) also
  switches to the AN-94's grenade launcher, fires it and switches back; captures show the Reflex
  over hidden iron sights, the launcher's raise and HUD, and BO2's grenade and flashbang effects. With
  `ZHBO_LOADOUT="lethal=claymore_mp;tactical=sensor_grenade_mp"` a Claymore is planted, arms and
  kills a rebel. All 59 player weapons and all 416 weapon+attachment combinations pass their tests.
* **Equipment in game (2026-10-14)**: scorestreaks removed (the Generals' upgrades and
  superweapons replace them). `scripts/autotest-mac.sh --equipment` runs the General with C4 and a
  Trophy System, then C4 and a Black Hat, against an enemy Sentry Drone and two GLA rocket
  troopers: the C4 sticks and goes off on a double-tapped reload (and kills), the Trophy System
  shoots down both incoming rockets, the Black Hat hacks the drone in 3 s and it defects. 8/8 pass;
  the normal autotest passes 13/13.
* **The General in the overhead view (2026-10-15)**: `scripts/autotest-mac.sh --rts` selects the
  General, lets Zero Hour's AI fight with the rifle, picks the frag (thrown, then back to the rifle)
  and the pistol from the command bar, then takes control: 8/8 pass, and first person starts with
  the pistol and its remaining ammo. The capture shows the reticle Take Control button, the AN-94,
  Tac-45 and frag buttons with their BO2 icons and ammo, and the empty frag greyed out with a red 0.
* **BO2 shading (2026-10-15)**: normal maps, gloss highlights and reflections on the viewmodels
  and tablet (screws, bezel edges and the receiver's wear catch the light), bloom, grade and vignette
  in first person; the overhead view unchanged. Autotest 13/13, overhead 8/8.
* **Overhead General, sorted streaks, pistol aim (2026-10-15)**: autotest 13/13 (the pistol now
  aims and fires twice; the timeline after it moved 1.5 s), overhead 8/8, equipment 8/8. Captures:
  the pistol's front sight sits on screen centre, the Reflex window is clean, leaving first person
  ends on the overhead camera's real pose (logged: within 2.5 units, same direction and field of
  view), and close up (`ZHBO_CLOSEUP=<zoom>` with `--rts`) the General aims the rifle and holds the
  pistol in BO2's stances. With a real mouse (`ZHBO_AUTOPILOT=manual`: the scene, the player
  starts overhead): selecting him by his body works, and after first person the first click
  selects (it went to the Take Control button before).
* **Load stalls (2026-10-15)**: the equipment autotest used to stall 0.85-1.25 s the first time
  the tablet, AN-94, Tac-45 and C4 viewmodels were shown; they now load in 3-4 ms (0 ms after the
  preload), and the loading screen preloads 26 objects and 6 viewmodels in about 0.4 s. 8/8 pass.
* **Guns keep firing; the General builds (2026-10-15)**: under RTS control an idle General whose
  gun ran dry picks the closest enemy he can see and attacks with the other gun (`--rts-dry` 5/5).
  Building overhead: his Build button opens his faction's dozer/worker menu (`ZHBO_Build_<set>`,
  copied at load with Back added); he builds as a dozer at `GENERAL_BUILD_RATE` (0.5) of its speed
  (host/patches/0012; a GLA supply stash took 41.4 s against 20.0 s). In first person: B brings up
  a hammer (BO2's `p_glo_tools_hammer` from `haiti_gump_front_door`, on the knife's bones with the
  knife's `viewmodel_strider_*` clips), the wheel picks from the same menu, fire places a site where
  he looks (Zero Hour's placement checks and payment, `buildObjectNow`), and holding F at a site
  builds it at a dozer's speed with the hammer swinging (the dozer's per-frame build and completion
  code); F opens the field computer only away from sites. ABI 14. `--build` 9/9 (the first-person
  stash took 21.6 s with the walk), default 13/13, RTS 8/8, equipment 8/8.
  Open: the hammer prop renders untextured grey (its campaign material's image is not found in the
  paks); the placement ghost has not been checked in a capture yet.
* **The General as a worker (2026-10-15, playtest fixes)**: his build menu is the worker his
  faction's Command Center trains (PlayerTemplate StartingBuilding → its command set's dozer, else a
  starting unit). Matching the side picked `GC_Chem_*` (Generals' Challenge) for the Toxin General,
  so his barracks didn't count for his workers. Every faction's choice is logged at load. The
  dozer's per-frame build and repair moved into `DozerAIUpdate::buildStep`/`repairStep`
  (host/patches/0012, now `general-dozer-work`). The dozer task and first person share them, so first
  person runs Zero Hour's own completion code. `Zhbo_WorkRate` scales his repairs too. In first
  person his dozer state machine is paused. Taking control is a player order, which drops his current
  task. A site laid out in first person is his pending task: he finishes it when released, and it is
  cleared if he builds it by hand. The leftover task is what had kept him off the Next Idle Worker
  list. Hammer: one placement per raise; the wheel turns the structure 15° a notch; ADS picks the
  next structure. F at a damaged structure repairs it (`ActionManager::canRepairObject`;
  `canResumeConstructionOf` for sites). Host API: `work_site`/`work` with a 64-byte `Site` (kind,
  name); ABI 15. The hammer was turned 180° about its handle so the face leads the chop; check it
  with `zhbo-assets hammer <png> ZHBO_VMH_SWG <frame>`. Patch 0013 makes `ZhboControlBarAccess` a
  friend of InGameUI (idle-worker list). `--build` 17/17: FPS repair, idle worker back overhead, an
  ordered repair at 15.0 s against a dozer's 7.5 s. Default 13/13, RTS 8/8, equipment 8/8, rts-dry 5/5.

## Docs housekeeping (2026-10-18)

The refactor plan's last phase is done: **AGENTS.md** is now the front door for agents
(ground rules, doc read order, verification ladder, commit/journal discipline). The rule
that matters most for future sessions: *plan/handoff documents in `docs/` are scaffolding —
fold anything durable into the permanent docs and delete the file in the same commit*.
`HANDOFF-MAC.md` and `REFACTOR-PLAN.md` were deleted this way (macOS gotchas →
PLATFORMS.md; the open backlog lives below). When adding features, update the doc the
behavior contradicts in the same commit, and keep gameplay numbers out of code — they come
from the tuning pack and the weapon-table schema.

## Class pick (2026-10-18)

BO2's create-a-class, phase 1: at first control of a General the player picks one of the
five basic BO2 classes (Assault/Marksman/Pointman/Breacher/Engineer) and the General assumes
it — the kit replaces the base kit, the weapon raises like after take_control, and the pick
overlay goes away. All data-driven: `crates/zhbo-core/src/classes.rs` (registry + the five
basics built in) and `data/ConquerOps/classes.ini` (shipped in the mod's BIG file, loaded by
`zhbo_load_classes` at init — the list must be identical on every client, so the pick crosses
the lockstep stream as a plain index). The pick menu is a HUD overlay (`draw_class_pick`)
driven by fresh presses of 1-5 (`class_pick_input` reads `Key::Key1`..`Key5` even while the
game's input is locked); the choice rides a new one-shot in `ClientOut` (`choose_class`,
`choose_class_general`) that the bridge turns into `MSG_ZHBO_CHOOSE_CLASS` (patch 0001: enum,
label, dispatch; `Zhbo_OnChooseClassMessage` → `zhbo_logic_choose_class`). `PlayerSim` keeps
the assumed class id (`class: Option<String>`), `EventKind::ClassChosen` plays the plant
sound, and `describe_event` names it. ClientIn grew `mouse_x`/`mouse_y`/`mouse_buttons` for
future menu work (ABI: ClientIn 128, ClientOut 140 — KeySet's 8-byte alignment inserts 4 bytes
of hidden padding after `logic_frame`; the old 112 assumed none).

**Open (next session):** custom classes. The engine's `GameMessage` has no string argument
type (`MessageStream.h`: integer/real/bool/objectid/drawableid/teamid/location/pixel/...), so a
custom class's spec cannot ride the pick message as text — it needs a packed-integer encoding
(weapon-table ids + attachment/perk masks, ~7 integer args) or a shared preset id. The
create-a-class screen (weapon/attachment/perk lists) and the lobby button
(`SkirmishGameOptionsMenu.wnd`/WOL lobby) are unstarted; the skirmish lobby's buttons are
`.wnd`-based (`TheNameKeyGenerator->nameToKey("SkirmishGameOptionsMenu.wnd:ButtonBack")`), so
the button likely needs a runtime-injected window on the pattern of
`ZhboControlBarAccess::injectConstructButtons`.

## Next steps

The macOS port is built and passing its in-game autotest (`scripts/autotest-mac.sh`); setup and
macOS-specific gotchas live in [docs/PLATFORMS.md](docs/PLATFORMS.md). Start from `AGENTS.md` and
the docs read order there. The list below is the older gameplay backlog.

1. Manual playtest by a human with real mouse/keyboard (the autotest injects input inside the mod).
2. More BO2 weapons (only the AN-94 viewmodel and the B23R sound are converted).
3. BO2's own reflection probes: keep `GfxImageLoadDef` data when the zone loader reads it.
4. Save/load of first-person state; two-client multiplayer test.
5. Strike Force (`so_rts_*`) content as a natural next theme.
