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
81. BO2 hides equipment sub-bones by **parking** them hundreds of inches away, and the trophy's and
    the M84's track extremes are authored, not a conversion bug: `viewmodel_trophy_system_hold_fire`
    flies the panels off (`sndnt#fly_falling_panels` at 0.463) to the park the idle holds
    (`tag_left_door` `[25.11,-554.70,-547.91]`, `tag_right_door` `[22.66,-530.85,-547.27]`), and
    `viewmodel_m84_throw` carries `tag_clip` to `[-198.27,-171.94,-203.84]..[100.47,73.31,9.66]`
    (the pin leaves the hand) after `_pullpin` brings it in near the palm. Do not filter large
    translations and do not read a track as absolute: playback composes **posed local = the model's
    stored local + the track**, which is BO2's rule (proven on the MP body rig, where
    `pb_death_faceplant`'s `j_spinelower` is a faceplant as an offset and a collapse as an
    absolute). The sweep's aimed-half blow-outs are a *staging* signature — the harness renders
    slot `IDLE`, which the game never plays for off-hand equipment (`HOLD_FIRE`/`FIRE`), and
    `nativevm::place::aligned` aims it though equipment has no ADS clips (the M84 has none at all;
    the trophy's key only `tag_torso`) — so the equipment-silhouette fix belongs to its own staging
    cycle, not to the track rule. Pinned by the ignored real-install test
    `equipment_subbone_tracks_are_bo2s_authored_parking`; evidence in
    `~/.opencode/plan/equipment-subbone-tracks/`.

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
(weapon-table ids + attachment/perk masks, ~7 integer args) or a shared preset id. *Resolved in the
section below: the packed-integer encoding and the create-a-class screen both landed.* The
skirmish lobby's
Create-a-Class button now ships as `host/patches/0014-create-a-class-lobby-button.patch`
(`SkirmishGameOptionsMenu.cpp` includes `ZhboBridge.h` and looks up
`SkirmishGameOptionsMenu.wnd:ButtonCreateClass` and calls `Zhbo_ShowClassPick`; the `.wnd` file must carry the button — a runtime-injected window on
the pattern of `ZhboControlBarAccess::injectConstructButtons` is the fallback if the art does
not). It reaches the pick overlay through the same `Zhbo_ShowClassPick` latch as the in-game
pick, which the client consumes via `ClientIn.class_pick_force`.

## Create-a-class editor and custom classes (2026-10-18)

BO2's create-a-class, phase 2/3: the editor and the player's own classes. The pick screen now
carries a sixth row — key **6** opens the create-a-class editor instead of picking a preset
(`class_pick_input` returns "open the editor"; the client keeps it in `Client::class_editor`,
drawn by `draw_class_editor` in BO2's skin: the ten element rows — Primary/Attachments/Secondary/
Attachments/Lethal/Tactical/Perk 1-3/Wildcards — plus the final **SAVE CLASS** row, the option
list on the right when a row is open, and the Pick-10 counter). The whole editor lives in
`crates/zhbo-core/src/classmenu.rs`: `Draft` is the Pick-10 choice set, `Editor` is the state
machine (`Outcome::{Continue,Save,Cancel}`, arrows to move, Enter to open/choose/confirm, Escape
to back out), and the arrows/Enter/Backspace/Left/Right keys were added to the Rust `Key` enum and
mirrored in the C++ host (`ZhboPlatform.h`'s enum, `ZhboPlatform.cpp`'s polls, the bridge's
`keyNames`).

Confirming SAVE on a valid draft (never over ten points) resolves it into a `CustomClass`
(`Draft::to_custom`), saves it into the profile, and sends the pick. Because `GameMessage` has no
string argument, the spec rides the payload as packed integers: `ClientOut.choose_class_payload`
carries `CustomClass::to_wire_words` (UTF-8 as `u32`s) with `choose_class = i32::MIN` (the custom
marker), and the sim reassembles it with `CustomClass::from_wire_words` — the bridge appends them
after the two `choose_class`/general integers (patch 0001's message shape is unchanged; only the
custom marker + payload are new).

The profile is the player's own classes, kept between sessions in `ConquerOps.profile` next to the
mod's log in the game's user-data directory (`crates/zhbo-core/src/profile.rs`: pure
`to_text`/`from_text`, up to BO2's ten slots, re-saving a name replaces it). The bridge reads it at
init (`zhbo_load_profile`) and, on a custom pick, writes the whole file back
(`zhbo_profile_save` returns the text; `ZhboBridge.cpp`'s `readProfileFile`/`writeProfileFile` do
the I/O). Re-opening key 6 loads the most recently saved class into the editor so a kit can be
re-edited. The **General's submodule was not touched**: the skirmish lobby's Create-a-Class button
ships as `host/patches/0014-create-a-class-lobby-button.patch` (applied and reverted in the
submodule), and the mod's own art/skin come from the mod's BIG file. ABI 15.

## Create-a-class parity with BO2 (2026-10-18)

Three gaps against BO2's create-a-class closed in the editor:

1. **Class naming.** The editor grew BO2's top **CLASS NAME** row (`Slot::Name`): Enter opens text
   entry (`Editor.naming`), A-Z/Space append (capped at `NAME_MAX`, 24), Backspace deletes,
   Enter/Escape leave the field. The name is part of the draft, resolves into `CustomClass.name`
   on save, and the profile assigns BO2's default slot name to a blank one. The Rust `Key` enum's
   letters and Space, and the C++ host's polls and `keyNames`, carry the typed edges
   (`Edges.typed`, in `Key::ALL` order).
2. **Greed's second perk.** A tier whose Greed wildcard (`Perk1Greed`/`Perk2Greed`/`Perk3Greed`) is
   in play now shows a duplicate **PERK n** row (`Slot::Perk1b`/`Perk2b`/`Perk3b`, `Editor.slots`),
   so a tier can hold two perks as BO2's Greed wildcard allows. The draft carries the second picks
   in `extra_perks`; `Draft::spent`/`to_custom`/`from_custom` and `wildcards_of` account for them,
   and turning the wildcard off drops the extra perk and its row. The row is *not* in `Slot::ALL`
   (it exists only while its wildcard does); the HUD and the walk both use `Editor.slots`.
3. **The in-game autotest run.** `ZHBO_AUTOPILOT=classpick` runs a new scripted run
   (`Autopilot::classpick_input`) that opens the editor from the pick screen, names the class,
   changes the secondary, and saves — so an in-game run exercises the editor's drawing and the
   profile write end to end. The C++ side needs no new scene (the default scripted scene and
   control suffice); the flag rides `ZhboConfig.autopilot_classpick` and the Rust
   `Autopilot.classpick`.

Tests: `cargo test --workspace` — zhbo-core 121 (the classpick autopilot run's full editor path is
now covered, alongside the naming row, the Greed second-perk rows, the Pick-10 budget, the
draft↔`CustomClass` round trip and the key-6 → edit → save → custom-pick integration); clippy clean
but for the pre-existing dead-code warnings. Open: the editor's in-game *drawing* still has not been
captured in a real frame on macOS; the classpick run is ready for that capture.

## Class selection from the overhead view and the lobby button, take two

Three gaps the first create-a-class pass left open are now closed:

1. **The Create-a-Class button in the skirmish lobby.** Patch 0014's button is built in code when
   the base `.wnd` art lacks the element. It now injects from the menu's `Init`, once the layout is
   settled, so the button lands relative to the menu's own rect and is visible; the first attempt
   injected during gadget setup, before the layout was real, so it sat at an unset position.
2. **The General's kit at spawn.** A class picked while commanding (overhead) applies its kit
   immediately and without first-person control; the sim makes the General's record on the pick
   when he was never controlled. The kit replaces the base one; the weapon still comes up on the
   next first-person control, so the "old default weapons" only showed because no class had landed.
3. **Class selection from the command bar.** The General's command set carries a new
   `Command_ZHBO_ChooseClass` button (`Command = ZHBO_CHOOSE_CLASS`), and the engine's
   `processCommandUI` routes `GUI_COMMAND_ZHBO_CHOOSE_CLASS` to `Zhbo_ShowClassPick`. The pick
   screen and the editor now draw and take input from the overhead view too, so a class can be
   chosen from either view, exactly like every other Zero Hour command.

Tests: `cargo test --workspace` — zhbo-core 134 (the overhead pick creates the General's record and
applies the kit; a re-pick replaces the chosen class); clippy clean. The engine's
`ControlBarCommandProcessing.cpp` and `SkirmishGameOptionsMenu.cpp` compile with the new call sites.

## Create-a-class rework: lobby screen, real BO2 art, mouse, full slot management (2026-10-19)

A four-phase rework of the whole feature (the plan lived outside the repo; the work was done by
DeepSeek V4.1 Flash subagents, orchestrated and reviewed). What landed, and the gotchas worth
keeping:

1. **The lobby button now renders.** Root cause of the invisible button: patch 0014's code-built
   gadget was created with `WIN_STATUS_IMAGE` but no images and a null font, and
   `assignDefaultGadgetLook(..., defaultVisual=FALSE)` leaves the text colours undefined — and an
   undefined `WIN_COLOR` is `0x00FFFFFF`, **white with zero alpha**. Both draw paths (image, text)
   painted nothing. The rebuilt button borrows the Start button's font/size, uses the non-image
   draw func, and sets text + fill/border/text colours explicitly. Also fixed there: gadget
   coordinates are parent-relative (the old code added the screen origin — harmless only because
   this root sits at 0,0), and the static button pointer went stale on a second lobby visit (the
   shell destroys layout windows on pop; look the button up by id every `Init`).
2. **The CaC asset pack.** `zhbo-assets` converts the create-a-class art from the player's BO2
   install: 16 perk icons (`code_post_gfx_mp` `perk_*`), 8 wildcard icons (`common_mp`
   `menu_mp_bonuscard_*`; Tactician is BO2's `_two_tacticals`), 25 attachment icons
   (`cac_mods_*`; the short→material table is `cac.rs`, confirmed against BO2's `MPUI_*` string
   table, not guesswork), 15 menu-chrome sprites (`menu_mp_cac_*`), and seven UI sounds from the
   `mpl_code_post_gfx` bank (`cac_enter_cac`, `cac_grid_nav`, `cac_grid_equip_item`,
   `cac_cmn_backout`, `cac_cmn_deny`, `cac_loadout_edit_submenu`, `cac_loadout_edit_sel`). The
   atlas is now **multiple 1024² pages** (each MappedImage names its own texture page) instead of
   one page that could not fit the pack; `hud::VERSION` 39→40. `willy_pete_mp`'s icon is the only
   missing sprite (no usable art in the install; drawn fallback). `ZHBO_BO2_UISelect` had silently
   never had an SFX entry; it does now.
3. **The screens are real.** All class-screen drawing moved to `classui.rs` (BO2 background,
   banner, rows, panels, per-element icons, `WeaponDef`-sourced stat bars, Pick-10 counter, deny
   rim); the pick list is presets + up to 10 profile customs + CREATE A CLASS, **customs
   re-pickable** (they never were — the old screen listed only the five presets). Mouse works end
   to end (the bridge finally fills the `ClientIn` mouse fields; hover/click/wheel through pure
   `classmenu::step_pointer` folds; `menu_open` in `ClientOut` makes the translator swallow
   game-bound input behind the screen and frees the cursor in first person). Keys 7/8/9/0 joined
   the `Key` enum (appended at the end, C++ side mirrored). Profile delete/copy/rename are in, on
   the pick screen and in the editor.
   - **Save is split from pick** (ABI 18): re-picking a class no longer rewrites the profile file.
     The one-shot `save_profile` (+`zhbo_profile_write`) carries the file's *whole text* packed
     four bytes per word — delete/copy have no class payload, so the wire-words save
     (`zhbo_profile_save`, now unused by the bridge) could not serve them.
   - **Pre-existing path bug fixed:** `cargo xtask pack` packs `data/` with paths relative to
     `data/`, so strings/tuning/classes live at `ConquerOps\*.ini` in the BIG — but `names.rs`
     said `Data\ConquerOps\*.ini`, so the shipped class presets, tuning pack and strings were
     **silently never loaded in-game** (built-in defaults masked it; the tuning test passed
     because it compares defaults to the file it reads itself). The weapon table's
     `Data\ConquerOps\weapons.ini` was and is right — the converter writes that path into the
     *other* archive.
   - The client now encodes preset picks 1-based (0 is the one-shot's "no pick"); the bridge
     decodes. The classpick autopilot emits one action per frame with release frames — macOS
     hitches skipped the old 0.1 s windows. SDL key polling now reports Escape (menus need it).
4. **The lobby screen** (ABI 19): patch 0015 retargets the button to a code-built full-screen
   **modal root window** (`winSetModal` + focus — input only within it; the lobby is inert), on
   the engine's `ExtendedMessageBox` pattern. `Shell::doPush` was rejected: it only loads `.wnd`
   files and the mod ships none. The window's draw callback runs `zhbo_menu_frame`
   (`ZhboMenuIn`/`ZhboMenuOut` — free functions, not API-table fields; prims render through the
   stored `ClientApi` callbacks, sounds via `cbSound`, both shell-safe); `menu.rs`'s `MenuState`
   hosts the same `classui` screens with no sim (browse → edit → save/delete/copy → close; no
   pick outside a match). `Zhbo_Reset` destroys the window and `zhbo_reset` drops the state.
   CONTRACT.md has the menu-ABI section; the manual shell check steps are in TESTING.md.

Tests: `cargo test --workspace` 233 green (zhbo-core 156), clippy unchanged, full autotest 13/13
and classpick 5/5 (save, pick by name, profile re-pick, no re-save on pick, no `???`). Still
needs human eyes: the shell screen's look/feel in the lobby (steps in TESTING.md).

Gotcha for future sessions: `zhbo-requirements`' `finds_games_inside_a_prefix_library` test reads
the **real** `ZHBO_ZH_DIR`/`ZHBO_BO2_DIR` env vars and fails when they are set (they are, in the
dev shells here). Run tests with `env -u ZHBO_BO2_DIR -u ZHBO_ZH_DIR`, or fix the test to scrub
them. Pre-existing, deliberately not fixed in this work.

## Create-a-class Phase 5: in-game is pick-only, mouse/digits/list fixes, screenshot harness (2026-10-19)

Playtest-driven fixes (again DeepSeek V4.1 Flash subagents, orchestrated):

1. **In-game is selection only, and never forced over the match.** The old `class_pick_wanted`
   counted "no class chosen yet" as a reason in *every* view, so the screen drew over the RTS view
   from match start while `menu_open` ate every click — unplayable. Now: `ViewMode::Rts` opens
   only on the command-bar force; `ViewMode::Fps` on the force or while unclassed (BO2's
   spawn-time select). Escape closes and keeps the current kit; a `class_pick_dismissed` latch
   holds the offer closed until the next take-control (`start_entering` re-arms it). The in-game
   screen lost the editor, the CREATE A CLASS row and the delete/copy keys (BO2: creating is a
   lobby activity); `Client.class_editor` is gone, the editor lives only in the shell `MenuState`.
   `classui::pick_rows*` took a `manage` parameter so both screens stay single-sourced.
2. **Digits type into class names** (`char_of` covers `Key0..Key9`; BO2's charset is A–Z, 0–9,
   space).
3. **Every clickable surface is hit-tested** (pick rows, slot rows, option rows, name/save rows,
   and the shell footer's delete/copy/back, which are real buttons now); `PointerEvent.moved`
   separates hover from click.
4. **Weapon lists are filtered to real CaC weapons.** The duplicates without icons at the bottom
   were converted alt-mode variants: the filter drops `WeaponDef::alt` entries and anything named
   in a base weapon's `alt_modes`, then de-dupes by display name (`classmenu::cac_entries`).
   Every listed weapon resolves a `ZHBO_BO2_Icon_<base id>`.
5. **The classpick autotest no longer drives an in-game editor** (there isn't one): the script
   seeds `ConquerOps.profile` with a custom class before launch, the autopilot picks it in-game by
   its computed key, then forces and Escapes a second offer and walks — asserting the pick, the
   no-re-save rule, and that play resumes. Editor end-to-end coverage is the headless `menu.rs`
   tests.
6. **Headless screenshot harness**: `cargo run -p zhbo-assets -- screenshot [out-dir]` renders the
   real screens (shell browse, in-game pick, editor) to PNGs with no game launch — prims are
   rasterized in software (atlas sprites read back out of `ConquerOps_BO2.big`, an embedded 5×7
   font for text). It installs the converted weapon table from the archive so the catalog is real.
   It immediately earned its keep, surfacing for the next art pass: the converted menu backdrop is
   a bright snow scene that washes the whole screen out (wrong material choice for a menu);
   `ZHBO_BO2_PanelLine` (a grunge frame) is stretched across panels instead of 9-sliced/tiled; the
   chosen-option `*` overlaps the option name; weapon stat bars only draw when the option list is
   short enough to leave room.

Tests: 247 green workspace-wide, clippy unchanged, full autotest and the reworked classpick run
pass. The screenshots live in `target/cac-shots/` (gitignored).

## Create-a-class Phase 6: unarmed spawn, command-bar class sets, the General-0 pick bug (2026-10-20)

Playtest bugs, two parallel Flash agents + an integrator build (ABI 19 → 20):

1. **The custom-pick kit bug was a wrong General id, not the data path.** A reproduce-first test of
   the profile→client→sim wire path passed on current code; the real break was that
   `Client.general` was only assigned at take-control, so a class picked from the *command bar*
   before first control went out naming **General 0** and `Sim::choose_class` silently returned —
   screen closed, no `ClassChosen`, stock kit kept. `Client` now adopts the sim record's General
   every frame, and the sim's no-record branch creates the record in `Mode::Rts`. Hardened along
   the way: `weapons::catalog::weapon_or_none` — an unknown weapon id is never silently swapped for
   the first catalog entry (the old `weapon()` fallback was exactly the "got an AN-94 instead"
   symptom); oversized custom payloads are denied instead of truncated-undecodable. The classpick
   autotest now seeds an MP7+claymore class and greps the pose line for `MP7 <n>|`, so a
   name-only pass can never ship this again.
2. **Unarmed until classed.** `Loadout::unarmed()` / `Arsenal::unarmed()` (knife only; the knife
   pre-raised, zero offhands, zero ammo, no ADS since it is not a gun) at every record creation;
   `Loadout::default()` untouched (classes' spec base, converter source). Consumers swept: HUD
   hides ammo/offhands/crosshair (`Arsenal::is_unarmed`), fire/swap/reload/alt/offhand are no-ops,
   melee still swings, hammer and field computer work. Escape on the spawn offer now means *stay
   unarmed*; the offer re-arms on the next take-control. RTS: a classless General reports
   `[0,0,0]` ammo and the bridge's existing no-ammo veto (`setPossibleNextShotFrame(frame+2)`)
   holds Zero Hour's weapons — no INI change; fighting resumes with the class kit once one lands.
   The autotest's scripted runs pick a class at 0.3 s (gated on the pick actually being up, so the
   digit can't leak into a weapon swap), and the `ZHBO_AUTOPILOT=rts` run gets a one-shot Assault
   from the bridge since it has no class UI.
3. **The command bar is the class menu for a fresh General.** Selecting a classless General swaps
   the bar to a code-built two-page class set (page 1: Take Control + 5 presets + up to 7 customs
   + Next; page 2: the rest + Back), built from `zhbo_core::classicons` rows served over the new
   `zhbo_class_buttons`/`zhbo_general_has_class` exports (ABI 20; `ZhboClassButton` carries label,
   icon names, perk icons and the custom's wire payload). Labels come from the live class list via
   `strings.rs` (`CONTROLBAR:ZHBO_ClassButton01..15`, duplicates get a ` 2` suffix); the kit icons
   are drawn over the buttons every frame (primary large, offhand minis, perk pips — the
   ammo-overlay pattern) because player-authored customs can never have conversion-time art. The
   bridge routes a press by pointer-matching the command window carrying `WIN_STATE_SELECTED`
   during `processCommandUI`; the pick rides the same `MSG_ZHBO_CHOOSE_CLASS` as the overlay
   (1-based index, or `i32::MIN` + wire words). Once classed, the normal set returns and its
   Create a Class button swaps back into the class set — the command bar no longer opens the
   overlay, which now exists only as the first-person spawn select. CONTRACT.md documents the
   class-set contract.
4. **Test-isolation fixes of the same family:** `classes.rs` and `strings.rs` got
   `profile::lock`-style `#[cfg(test)]` mutexes after parallel runs showed `classicons` tests
   reading the class registry mid-mutation and `strings` tests racing the shipped table. Lock
   order where tests combine: profile → classes → strings.

Tests: 268 green; engine build clean at ABI 20; full autotest 13/13 (now with a class pick before
combat); classpick 6/6 with the MP7 kit proof. Manual checks outstanding: unarmed spawn (knife
only, both views), the paged class bar with kit icons, and a lobby-authored custom applying its
exact kit in-game.

## Playtest fixes: the dropped command-bar pick, the ??? labels, the stitched icons (2026-10-20)

Three user-reported bugs, three Flash agents in parallel, then an integrator build (ABI 20 → 21,
archive VERSION 40 → 41):

1. **Command-bar class picks were dropped 100% of the time.** The button dispatch queued
   `s_out.choose_class` from the UI pump, but `zhbo_client_frame` writes the WHOLE `ZhboClientOut`
   fresh every frame (`*out = result` in ffi.rs) and `applyClientOutput` reads it right after — no
   click ordering survives. Gotcha worth remembering: **one-shots that cross the ABI must be
   ClientIn, never bridge-side ClientOut state.** Now `ZhboClientIn.class_button` (1-based button
   index, appended; `ZhboClientIn` 128 → 136) and the Rust client emits the pick through the same
   `pick_custom_class`/preset path as the overlay, naming the General it adopted from the sim
   record. The class set closes on `zhbo_general_has_class` (a failed pick leaves it open for a
   retry); the stale bridge queue code is gone.
2. **The ??? on every custom label was a wchar_t width mismatch.** The string ABI is UTF-16
   (`const uint16_t *zhbo_string`), but `WideChar` is `wchar_t` — **4 bytes on macOS/Linux** (the
   engine has no `-fshort-wchar`), and `Zhbo_FetchString` just reinterpreted the pointer:
   `wcslen` read two UTF-16 units per WCHAR and ran past the 16-bit NUL into heap, and CoreText
   maps the garbage to `?`. Windows' 16-bit wchar_t made the cast harmless there — which is why it
   was long-standing and Mac-only, and why Rust-drawn overlay text was fine. Fix: widen properly in
   `Zhbo_FetchString` (with surrogate-pair handling; `GameTextManager::fetch` copies immediately,
   so one static buffer suffices, ABI unchanged). Also fixed in the same pass: `zhbo_tactical` was
   never in `strings::lookup`'s weapon family (the Tactical button missed on EVERY platform since
   the buttons landed); served strings are now interned (immortal pointers across label rebuilds,
   and no per-fetch leak of hot labels); `zhbo_string` logs `served`/`MISSED` once per label so
   future misses name themselves.
3. **The "stitched" icons were hand-set base-game coords, not the atlas.** A contact sheet of the
   real dist archive (`zhbo-assets hud-sheet`, new diagnostic) showed all 161 sprites whole and
   the reconversion byte-identical — then proved `ZHBO_Button_ChooseClass`'s rect in
   `ZHBO_A_Buttons.ini` started 10 px inside one `SAUserInterface512_005` cameo cell and ran
   through the gutter into the next (literally the tail of one jet and the head of another), and
   `ZHBO_Button_ClassTile` reused a soldier cameo under the kit overlay. The converter now always
   generates both tiles (plain dark tile; BO2 grid-pattern tile for ChooseClass; VERSION 41), the
   fallback coords point at one whole cell, and a new test (`every_command_button_image_is_a_
   generated_tile`) would have caught it.

Tests: 277 green; engine build clean at ABI 21; full autotest 13/13; classpick 6/6 (first attempt
died to a machine hitch storm inside the 90 s wait — the in-game flow had actually completed;
rerun passed. `Zhbo_Reset` legitimately fires ~4× at startup; `ResetControlBarTrackers` rebuilds
the class pages each time, which is why the log shows repeated "made the class menu" lines).

## Create-a-class parity wave: the 10×(5+5) model and the reference-driven pass (2026-10-20)

The reference environment session delivered a decompiled CaC (`~/Documents/BO2-reference/lua-dump/`,
479/479 scripts) and 295 captures (`~/Documents/BO2-reference/cac/sweep/`); this wave verified and
refined our implementation against it (Flash agents; citations are to those files). The AGENTS.md
addendum permitting Steam-online menu-shell capture is in the same tree.

1. **The model is class sets, not a flat list** (`cacutility.lua`'s caps block; `cacchooseclass.lua`'s
   `SetActiveClassSet`). Profile v2: ten `ClassSet`s of ≤5 customs each, plus an `active_set` the
   game loads with ("You will load into the game with this class set"); v1 profiles migrate into
   sets 1–2. Save/copy/delete/rename/reset are slot-based inside sets; a sixth class in a set is
   refused (was: oldest evicted). **Gotcha found inside the work:** `classes::parse` stripped `;`
   as an inline comment, so every shipped spec silently loaded as its first slot only — the real
   kits never reached the game. Comments are whole-line only now.
2. **Caps and naming.** New hard cap of three bonus cards (`cacutility.lua` `maxBonusCards = 3`;
   a fourth is denied with the deny cue). BO2 never says "Pick 10" — the UI now says "allocation"
   with BO2's segmented 10-block bar and `n/10` counter (`allocationbar.lua`'s `AddBars`,
   `allocationsubtitle.lua`'s format).
3. **Repair, BO2's way** (`classrepair.rs`, mirroring `checkclasses.lua` `CheckItems` /
   `ClearWeapon` / `ClearSlot`): unknown weapons clear with their attachments, wrong-slot offhands
   and repeat/unknown perks drop in slot-walk order, and an over-budget/over-card class is trimmed
   back-to-front so the weapon is the last thing standing — applied on profile load and when a
   stored class opens in the editor.
4. **The premade five are BO2's now** (`editdefaultclasses.lua` + `mp/statstable.csv`'s
   default-class column): Assault=TAR-21+SMAW, SMG=MP7+Five-seven, LMG=MK48+SMAW, CQB=870+SMAW,
   Sniper=SVU+Five-seven, frag/concussion, Lightweight/Toughness/Dexterity, no attachments. The
   built-in stand-in catalog gained those seven guns as name+class stand-ins (schema defaults; the
   converted table replaces them — no numbers hand-copied).
5. **The command bar is one page of the active set**: Take Control + the five premade + up to
   five customs + Back; the two-page machinery and Next glyph are gone (`ZHBO_CLASS_BUTTONS 15` is
   capacity only; rows 10..14 are `present=0` and skipped). Rows re-derive from the active set on
   every reset, so a set switched in the lobby shows up next match.
6. **The screens read like the captures** (bounded visual pass; citations per item in the commit):
   `‹ CLASS SET n ›` pager with the ten-dot marker (`a-chooseclass/001…011`; display-only in-match,
   where the client can't switch sets), the editor as BO2's boxed grid with the orange-outlined
   selected box and its circled X (`R Remove` when the slot holds something), attachment boxes
   under each weapon ("Attachments not available" where none are offered), the weapon picker's
   rotated CATEGORY column (`c-primary/001`, `d-attach/037`) with the four segmented stat bars in
   BO2's order (FIRE RATE, DAMAGE, RANGE, ACCURACY), perk order confirmed against `e-perks`, bonus
   cards reordered to `f-wildcards/001` (Secondary Gunfighter before Primary Gunfighter), and the
   choose-class footers with `S Class Set Options` / `O Class Options` dialogs (`a-chooseclass/021,
   023`, `j-dialogs/001`). Deliberate divergences: no camo/personalize, killstreak slots, unlock
   tokens/padlocks, level indicator, weapon descriptions (no data source); the set dialog ships
   RENAME + RESET SET (set COPY deferred — profile has no set-copy op).
7. **Autotest follows the premades:** the standard run seeds a `GL Test` custom (an94_mp+gl — no
   premade carries an attachment, so the alt-mode check needs it) and picks it by its derived row
   key; `--rts` checks follow the Assault SMAW. **Open:** `scripts/autotest.ps1` (Windows) doesn't
   seed the GL custom — port when a Windows run is next available.

Gotchas for the art pass: the converted CaC chrome sprites are whole framed tiles and smear when
stretched (the screens draw their own chrome now; bring the art back via nine-slicing in
`hud::render`); the converted menu background is a bright snow scene, so the screens scrim it dark
to match BO2's near-black; `zhbo_menu_frame` must draw `menu.dialog()` after `draw_screen` or the
dialogs never reach the shell. `classui.rs` is ~2,900 lines — past the AGENTS.md smell line; split
layout/draw vs. input folds vs. dialogs next time it is touched.

Tests: workspace green (zhbo-core 234); engine build clean; full autotest 13/13 (with the GL
custom pick), classpick 6/6.

## v1.0 parity, Track 0: the shared BO2 UI layer (2026-10-21)

The v1.0 plan (`docs/V1-PARITY-PLAN.md`) replaces guessed BO2 look and behaviour with ground truth
from the reference environment (`~/Documents/BO2-reference/`). Track 0 builds what the HUD and
class-screen ports share:

1. **Splits.** `classui.rs` (2,920 lines) became `classui/` and `hud.rs` became `hud/`, each
   split along its existing section seams. The move changes no behaviour.
2. **LUI layout** (`zhbo-core/src/lui/mod.rs`). Elements are four edge offsets plus four anchor
   flags, resolved per axis: both anchors stretch, one anchor offsets from that edge, neither
   offsets from the centre. The root is `LUI.UIHeight` = 720 units tall
   (`patch_mp/ui/lui/lui.lua:4`) and as wide as the aspect allows. `lui.lua`'s `UIWidth = 960`
   is the 4:3 base, not the root width. G9 confirms the engine centres a root 720 high and
   720·aspect wide, and that PC's safe area is the whole root.
3. **Fonts.** BO2's UI fonts are `fonts/720/{small,normal,big,extraBig,extraSmall,italic,
   smallItalic}Font` in **`zone/english/en_code_post_gfx_mp.ff`**. `zone/all` has only the
   debug fonts. All of them share one 512×1024 alpha atlas, `gamefonts_pc_720`, streamed from
   **`zone/english/en_base.ipak`**. The converter used to open only `zone/all` packs. The atlas
   is IWi **format 5, alpha-only**, which `iwi::Format::A8` now decodes as white coverage. The
   `CoD.fonts` → file mapping and the English `CoD.textSize` values come from
   `patch_mp/ui/t6/codbase.lua:147-350`. Every size equals its font's pixel height (Default 25,
   Condensed 30, Big 48, Morris 60), so LUI draws text at the font's native size; the ammo label's
   48-unit box in `hud/ammocounter.lua` is `bigFont`. Each glyph becomes a MappedImage, and the
   metrics travel as `Data\ConquerOps\bo2_fonts.txt` → `zhbo_load_fonts`. Without the archive,
   text falls back to the engine font. G9 confirms the engine's placement: scale = box
   height ÷ pixel height, the pen line at the box bottom, glyphs at `bottom + y0·scale`.
4. **Menu sounds.** `sounds::UiCue` lists every alias the MP UI Lua plays (63 after removing
   `cac_restricted`, a padlock material, and `cac_closed`, an event name). The `cac_*` aliases
   live in `code_post_gfx_mp`'s `mpl_code_post_gfx` bank. A few `uin_*` aliases are in
   `common_mp`, `patch_mp` or `patch_ui_mp`. Most of the shared widgets' `uin_navigation_*` and
   `uin_slider_*` aliases exist **only in the single-player bank** (`spl_code_post_gfx`), so BO2's
   MP menus are silent for them, and the converter leaves them out the same way. The real install
   yields 37 menu events. The seven old CaC cue constants now point at their aliases' events
   until Track C wires each Lua call site to its exact cue.
5. **Animation states** (`lui/anim.rs`): `Tween` is one `animateToState` move. An interrupted
   move starts from the current value, and a zero duration snaps. The easing is the engine's (G8,
   tick `0x00932f80`): linear, with `(1 − cos πt)/2` on the flagged half (easeIn: t < 0.5,
   easeOut: t > 0.5).
6. **Materials by BO2 name.** `lui::materials::MATERIALS` lists the materials the ports draw. Each
   converts to `ZHBO_Mat_<material>`, searched across the MP zones, now including `patch_ui_mp`.
7. **A dead end, recorded so nobody repeats it:** image `GfxImageLoadDef`s are loaded into
   `XFILE_BLOCK_TEMP`, which our loader recycles on every pop. A generic keep-what-points-into-temp
   capture was tried for the font atlas, but the atlas is streamed (one part) after all, so it was
   reverted. MODLOG backlog #3 (reflection probes) will need that capture.

Archive `hud::VERSION` 41 → 42.

## v1.0 Track O: command-view orders and the General's class guns overhead (2026-10-21)

Scope, from the user: first-person actions should be commandable from the overhead view as **RTS
orders the General carries out himself**. The primary/secondary swap stays a direct toggle; the
one-off actions (lethal, tactical, knife) get their own targeted flows. The contract is in
CONTRACT.md "The General under RTS control: his weapons and orders".

1. **Gap found first: the overhead guns were never the class's.** `ZHBO_GeneralRifle`/`_Pistol`
   were written once by the converter from `Loadout::default()`. An MP7 or SVU class therefore
   fought overhead with the default rifle's fire rate, magazine, reload, reach and sound; only the
   ammo count was the class's.
   - The converter now writes one Zero Hour weapon per BO2 player weapon
     (`ZHBO_GeneralWeapon_<id>`, `weapons::rts_weapons_ini`).
   - The core names, every frame, the one each slot must hold (`RtsWeapons::templates`).
   - The bridge swaps it in with a new engine call, `Object::zhboSetSlotWeapon` (patch 0016).
   - The swap survives save/load because `Weapon::xfer` stores the template by name.
2. **Orders ride the third slot.** Zero Hour has three weapon slots and `FIRE_WEAPON` buttons
   already do "walk into range, fire once at a point/unit", so Lethal, Tactical and Knife are
   `FIRE_WEAPON TERTIARY MaxShotsToFire=1`. The slot holds the ordered item's Zero Hour weapon,
   whose `AttackRange` is the order's reach:
   - a throw: v²/g at the core's own throw speed, never inside its blast (`MinimumAttackRange`);
   - a plant: `PLANT_DISTANCE`;
   - the knife: `MELEE_RANGE`.
   When it fires, the core does what first person does (`WeaponAction::Plant`, the throw arc,
   `Sim::melee_strike`, extracted from the first-person melee). Gotchas found on the way:
   - **A temporary weapon lock can't override a permanent one** (`WeaponSet::setWeaponLock`). The
     General is usually permanently locked to a gun (the switch buttons and the core's own
     after-order lock both lock permanently), so the order message's handler releases the
     permanent lock first; the core locks him back to the gun once the order is carried out.
   - **The item's weapon has to be in the slot before Zero Hour reads the order.** The button's
     action message (`Zhbo_OnFireWeaponCommand`, patch 0016's hook in `issueFireWeaponCommand`)
     comes just ahead of Zero Hour's own weapon message, and its logic handler swaps the weapon in
     immediately (`applyRtsOrder`), not at the next `rtsWeapons`. Otherwise Zero Hour would aim
     the order with the previous item's reach.
   - **Zero Hour's attack range is edge to edge** (`Weapon::getAttackDistance` adds both bounding
     radii, and the General's is 10 units, 60 BO2 inches). First person's knife check is centre
     distance against the BO2 body, so a General stopped by Zero Hour at knife range was always
     too far for it. The RTS strike counts both footprints (`MeleeReach::Footprint`).
   - **Events emitted inside `rts_update` were never logged.** `zhbo_logic_step` takes its event
     baseline after `rtsWeapons` has run, so the old overhead throws never appeared in the log
     either. The ffi now logs `rts_update`'s own events (`log_events`).
3. **Reload** is a bridge command (`Command_ZHBO_Reload`, action 4): the core tops the gun in hand
   up from its spare rounds, and the bridge starts `Weapon::reloadAmmo` for the time.
4. **The bar shows his kit, not the default loadout's.**
   - The four loadout button tiles and the weapon labels ("&AN-94") were baked from
     `Loadout::default()`.
   - Now the gun and order buttons are plain tiles, and the bridge draws the live kit's BO2 icons
     over them (`zhbo_client_rts_icons`) with what is left, the class-button pattern.
   - The labels are BO2's slot names (Primary, Secondary, Lethal, Tactical) plus Knife and Reload.
   - Reload gets a generated magazine tile.
   - The set is 3 Primary over 4 Reload, 5 Secondary over 6 Knife, 7 Lethal over 8 Tactical.
5. **Cleanups along the way:**
   - the dead copies the module split left in `sim/build.rs` (hitscan helpers) and `sim/fire.rs`
     (`HostWorld`, `placement`) are deleted;
   - the pre-existing clippy warnings in every file touched are fixed (ffi docs, `as_chunks`, a
     `const` assert, a `Files` alias).

Decisions made without the user (asleep; revisit if wrong):
- The Black Hat is never an overhead order: it needs its target held in view.
- C4 with none left in hand has no overhead "detonate" button yet.
- Attachment variants share the base weapon's RTS weapon. An extended magazine's bigger clip is
  still counted by the core, but Zero Hour reloads at the base clip.
- The throw cursor's ring is the item's blast (`ScatterRadius`). It is read from whatever item the
  slot holds when the button is pressed, so it shows the previous order's item until the order is
  given.
- A cancelled order, or one never carried out, leaves him without his permanent gun lock; he
  falls back to the primary.

Tests: `cargo test --workspace` is green (zhbo-core 245, of which 11 are new in `sim/rts.rs`;
zhbo-assets 36, of which 2 are new). The ignored real-install conversion passes with every player
weapon's RTS weapon present and within the ABI's name cap. Patch 0016 applies on top of 0001–0015,
and all touched C++ passes `-fsyntax-only` against the real compile commands with the patched
headers overlaid. **Not yet run in game.** Integration needs:
- ABI bump 21 → 22 (`ZhboRtsWeapons` 36 → 192 bytes, `ZhboRtsIcons` 240 bytes,
  `zhbo_logic_rts_reload`);
- archive VERSION 42 (reconvert);
- `scripts/autotest-mac.sh --rts` (new checks) and `--rts-dry`.

## Track R, milestone 1: the native BO2 viewmodel path (2026-10-21, branch `v1/track-r`)

The v1.0 plan's Track R replaces the W3D conversion of BO2 art (the root of the transparent
patches, attachment misalignment and wrong animation variants) with BO2 models drawn by the
renderer itself. This milestone builds the whole path end to end behind `ZHBO_NATIVE_VIEWMODEL=1`;
W3D stays the default until the integrator verifies it in game.

1. **`zhbo-native`** (new crate, no deps): the mod's own `.zbm`/`.zba`/`.zbv` blobs and BO2's pose
   math. Composition and sampling mirror `character::Rig`/`animate`, which were verified in game:
   a later part hangs from an earlier bone by name; rotations replace; **translations add to the
   bone's local offset** (zero for roots). BO2's `hideTags` collapse the tag and its children to a
   zero matrix, as BO2 hides parts. Readers are bounds-checked to the byte (every prefix of a blob
   is refused).
2. **Converter**: `native.rs` writes the blobs beside the W3D output for every converted weapon:
   the arms per faction, gun, magazine plus its rigid `tag_clip1` copy, attachments on their
   `viewModelTag` at the attachment's view offset (`WeaponAttachmentUnique`), and the combination's
   animations (the variant table the W3D path already picks). The aim marker `tag_zhbo_aim` uses
   the W3D path's rule. Real-install test: 60 viewmodels × 3 factions compose with no missing tag;
   1,578 files, 31.3 MB, all f32. Archive VERSION 42.
   - **Gotcha:** a few BO2 viewhands (e.g. `c_chn_mp_pla_longsleeve_viewhands`) decode with
     non-finite half-float weights and unit vectors. The W3D path never noticed. The blob reader
     refuses non-finite data, so the converter neutralizes those vertices.
3. **Renderer** (`zhbo-gfx` `runtime/native.rs` + `native.wgsl`): GPU skinning (256-bone palette),
   `bo2Shade` (patch 0009) ported to WGSL in linear light with sRGB colour maps, BO2's GE128 alpha
   test, and blend from `GfxStateBits` (#73). Its own depth buffer, so the world never cuts the gun.
   It draws into the scene view just before the composite (`kBgfxEngineView` 1 /
   `kBgfxSceneCompositeView` 9, BgfxBackend.cpp), so bloom and tonemap apply.
   - **Parity decision:** the native draw uses the scene's own projection. G4
     (`~/Documents/BO2-reference/ghidra/answers/G4.md`: `CG_GetViewFov` `FUN_004a0920`, `cg_fov`
     65 × `cg_fovScale`) shows t6mp has no viewmodel FOV; the `cg_gun_*` dvars all default to 0.
4. **Core** (`nativevm/`): asks the bridge for archive files (it can't open the BIG itself), loads
   and composes, plays the clip with a playback-rate hook (G3's state-duration multipliers), and
   places the gun. Placement is the eye frame × `GunPose` × ADS slide (aim marker onto the view axis)
   × art scale. `GunPose` is the hook for the gun-position track's `weapons::gunpos`; bob and sway
   are not implemented here. Uploads mesh and textures and submits the palette. Logs and file
   requests go through the bridge.
5. **Bridge** (`ZhboViewmodel.cpp`): calls `zhbo_native_viewmodel` before loading the W3D model,
   serves file requests and log lines, and plays the muzzle/brass FXLists at
   `zhbo_native_bone`. Sound notetracks are unchanged. The scene lights come from GlobalData; ZH's
   `lightPos` is the direction light *travels*, so the light is `-lightPos` (`W3DShadow.cpp`
   `lightRay`).
6. **Single sources**: `ART_SCALE` (11/72) and `AIM_TAG` live in `zhbo-native`, and
   `character::SCALE` reads it. Open finding for whoever owns units: the art scale (11/72 units per
   inch) and the sim's `units::ZH_PER_INCH` (1/6) differ. The viewmodel is unaffected (scaling about
   the eye leaves the image unchanged), but the two should be reconciled.

Tests: zhbo-native 17; zhbo-gfx `tests/native.rs` 5 (headless Metal: skinning, the sRGB round
trip, light, view transform, view order, refusals); core `nativevm` 4, plus 1 end to end into a
headless renderer with `--features renderer`; workspace green; real-install conversion green. The
bridge file compiles against the macOS engine build's flags. **Not run in game** (integrator).

Still to do in Track R:
- R4: surface back-face culling (currently none) and BO2's reflection probes.
- R5: the attachment combination's own variant clip set is already in the manifest; verify in game.
- R6: ADS optic eye relief from the weapon def (today: the W3D marker rule).
- R7: third person, the General's body and weapon on the native path with BO2 MP body animations.
- R8: FxEffectDef effects on tags.
- Tablet and hammer viewmodels: still W3D.
- Retire the W3D viewmodel once verified.
- Quantize the blobs.
- Share textures across viewmodels (each weapon switch re-uploads the arms' textures).

## Integrating Track R: a config bug that hid every ZHBO_* variable from the core (2026-10-21)

The first `ZHBO_NATIVE_VIEWMODEL=1` run drew W3D as if the flag were unset. `config::get()` was
`CONFIG.get_or_init(Config::default)`, and the bridge's loaders (weapon table, strings, classes)
run before `zhbo_init`. The first early read therefore locked the defaults in, and `init`'s
`set` was silently dropped. The autopilot and loadout only kept working because the C++ side
parses those variables itself. `get()` now falls back to a static default without filling the
cell; the new test `a_read_before_init_does_not_lock_in_the_defaults` covers it. With the fix the
native path runs: autotest 13/13, every weapon composed (e.g. `ZV12U composed: 86 bones, 14410
vertices, 16 surfaces`), and zhbo-gfx refused nothing. **Open (next Track R round):** next to the
W3D render of the same frame, the native colours wash out to grey (the AN-94's olive finish and the
dark gloves render light grey), and the gun sits a little higher. The colour map's sampling or the
`bo2Shade` light balance is the first suspect.

## v1.0 parity, Track H: BO2's HUD from its own Lua (2026-10-21)

The first-person HUD is now a port of BO2's MP HUD Lua
(`lua-dump/decompiled/patch_mp/ui_mp/t6/hud/`), built on Track 0's LUI layer. Every box keeps the
Lua's numbers and anchors, every image is the material the Lua registers (converted as
`ZHBO_Mat_<name>`, listed in `lui::materials::MATERIALS`), and the code-drawn parts follow Ghidra
G6/G7. Element by element:

1. **Ammo area** (`hud/ammo.rs`; `ammoarea.lua`, `offhandicons.lua`, `huddigit.lua`):
   - The panel art (300 × 150), the offhand counters (24 px icons stacked left, alpha 0.75^n),
     the fire-rate icon, and the alt-fire icon with its key.
   - **HUD digits**: the `_white` art behind at offWhite α 0.6 and the plain art in front. The
     background/foreground binding came from `setDigit`'s upvalues 3 and 4 in the bytecode.
     Reserve digits are drawn small (×0.75) and the clip big, with a pen starting at
     Right − 20 = −48. A low clip pulses red every 500 ms.
   - **Weapon label**: the three-piece gun title in `SmallItalic`, black at α 0.5. Its width is
     the text width − 18 + 128 and its visible width the text width − 18 + 30. Attachment labels
     show for 2000 ms; with two or more, `CUSTOMIZED` replaces them, and the fire-rate icon fades
     in over 200 ms with easeIn.
2. **Score panel** (`hud/score.rs`; `scorebottomleft.lua`).
   - **Gotcha:** unluac names every upvalue `arg`, so the numbers were recovered from the
     bytecode (`build/t6lua.py disasm`). The `new` closure captures 28, 110, −87, 107, −48.
   - The panel's left inset is 28; the decompiled "110" was a mislabel. The score sits at
     x 28 + 110 = 138, y −87, and the digits pack from the first slot (leading zeros take no
     space).
   - **Divergence:** the Zero Hour faction logo, the army's radar colour on the backing, and the
     player's funds as the number. There is no enemy score, timer or win status.
3. **Streak column** (`hud/rewards.rs`; `rewardselection.lua`):
   - The vertical list's slot tops are −302, −254, −218 and −182, with slots 64 × 64 and 24 in
     from the right edge.
   - **Divergence:** the bottom slot is the field computer (always lit) and the next one the
     build menu.
   - A slot's name shows for 1000 ms and fades over 250 ms.
4. **Centre** (`hud/center.rs`):
   - **Crosshair (G6):** the offset is `tan(spread)·240/tan(fovY/2)` in the 480-line space;
     alpha is `1 − aimSpreadScale`, floored at 0.5.
   - **Hit marker (G6):** `damage_feedback` 24 × 48 at (−12, −12) in the 480-line space, fading
     over 1 s (`HITMARKER_TIME` 0.25 → 1.0). The red kill tint is gone, since BO2 has no kill
     marker.
   - **Score popup:** "+N" in `Condensed`, 40 right of and 75 above the centre, for 1000 ms
     (`SCORE_POPUP_TIME` 1.4 → 1.0).
5. **Damage** (`hud/damage.rs`, `hud/state.rs`; G7):
   - **Low-health overlay:** the `overlay_low_health` state machine. Pulsing starts below 0.66
     health and fades out above 0.9, with phases of 150, 320 and 400 ms and a 700 ms fade-out.
     The old corner blood is gone.
   - **Direction indicators:** 8 slots of 2 s each, opaque for the first half; each is a
     128 × 64 band 64–128 units from the centre, drawn through the new `Prim::Quad` /
     `ClientHost::image_quad`.
6. **Minimap and compass** (`hud/map.rs`; `compass.lua:70-242`):
   - The container is 13–163 × 7–187, with a 150 × 150 map.
   - The strip sits 32–6 above the container's bottom on a black α 0.25 plate with a white α 0.25
     border, showing `shaderVector0.x` 0.673 of a turn.
7. **Scoreboard on Tab** (`hud/scoreboard.rs`):
   - The columns are TDM's: SCORE, KILLS, DEATHS, ASSISTS, resolved from `en_code_post_gfx_mp`
     (`MENU_SCORE_CAPS`, `MENU_KILLS_CAPS`, `MENU_DEATHS_CAPS`, `MPUI_ASSISTS_CAPS`).
   - BO2 hides its HUD under the board (`BIT_SCOREBOARD_OPEN`), and so does the port.
   - The tallies are client-side (`HudLui::kills/deaths`), and only the local player's row shows.
     The widths come from the file's constants but their binding to each box is not fully
     decoded: **approximate**.
   - **The full map moves to M** (`Bindings::map`, default `Key::M`; `Bindings::scoreboard`
     `Key::Tab`). The new key is a default for the user to review.

**Pending:**
- **Weapon table (Track W):** `reticleSide`, `iReticleSideSize`, `iReticleMinOfs` and
  `crosshairColorChange`; BO2's fire type (the pump and bolt icons); the low-ammo warning
  threshold (a third of the magazine is used meanwhile).
- **Engine:** an `image_quad` C callback and the damage direction (CONTRACT.md). Without them
  the indicators draw outlines and no slot fills. The view-damage HUD shake (`hudshaker.lua`)
  also needs that direction.
- **Not ported**, with no sim source: `scorefeed` rows (BO2 adds none for kills), the
  obituary window, `notificationpopups`, `grenadeeffect` (proximity-grenade droplets), the
  game timer, and the score popup's additive glow.

## Integrating Track H: HUD materials loaded inline inside other assets (2026-10-21)

The ported HUD asked for `hud_mp_firerate_*` and `overlay_low_health`. The launcher reported them
"not in this install", although the reference catalog lists them in `common_mp`. They are not
`common_mp` assets: they are `Material`s loaded **inline inside other assets** (the weapons), and
`find_material` only looked at top-level assets. Searching for the name pointer alone matches
reference stubs and other structs' name fields too, so `xmodel::find_inline_material` takes the
name string, finds the fields pointing at it, and accepts only a struct that reads as a real
`Material`: 1–32 textures, all with sized images. The real install then gives 64×64
`hud_mp_firerate_auto` (`mp.ipak`) and 512×512 `overlay_low_health` (`base.ipak`). The scan is
slow, so the archive build checks every zone's top-level assets before scanning any zone
(`find_asset_material` first). The whole real-install archive build takes about 40 s in
release. In game: the FULL-AUTO label and the red low-clip digits draw from BO2's art, and the
autotest passes 13/13. Seen in the screenshots and **open**: the streak column shows only BUILD,
without the field computer's slot, and a stray yellow "3" sits by the offhand icons.

## v1.0 Track PW: perks as specialties, BO2 movement, weapon timing, recoil, gun position (2026-10-21)

Everything here comes from the Ghidra answers in `~/Documents/BO2-reference/ghidra/answers/`
(G1–G7) and BO2's own data. Before this, the perk effects and movement numbers were guesses.

1. **Perks are specialty sets** (G1). The engine never sees perk names. Each consumer tests one
   of 62 specialty bits (the index table at `t6mp` `0x00f6b918`), and a player has a bit when
   the class, the current weapon variant (`WeaponVariantDef+0x248`) or one of its attachments
   (`WeaponAttachment+0xfc`) grants it. The converter reads the perk rows from the player's
   `patch_mp` `mp/statstable.csv` (a new StringTable reader); an install test proves all 16
   equal the built-in copies (`statstable.csv:151-166`). **The old Dexterity was wrong**: it
   was a guessed "aim faster after sprinting" ×0.5, but BO2's Dexterity is
   fastmantle/fastladderclimb/sprintrecovery/fastmeleerecovery. Dead Silence's stats row is
   `PERKS_NINJA`.
   **Gotcha:** on this install only **FMJ** grants bits through the data (`armorpiercing`,
   `bulletpenetration`). No weapon variant does, and fast mag and quickdraw work through their
   reload/ADS time scales, not `fastreload`/`fastads` bits. G1's README examples describe the
   mechanism, not this data.
2. **The tuning pack is in BO2 units under dvar names**, one field table in `tuning.rs`, with a
   test holding `tuning.ini` equal to the defaults. Fixed in passing: the old ini said
   `WalkSpeed = 380` while the default was `inches(190)` = 31.7 units/s.
3. **Movement** (G2). Lightweight is `(int)(190 × 1.07)` = 203 in/s, the engine's truncation.
   Strafe ×0.8, back ×0.7, strafing while sprinting ×0.667. BO2's sprint meter: the length is
   base × the weapon's `sprintDurationScale` (×2 Extreme Conditioning, cap 16.383 s); it
   recharges 1 s per second after a sprint; a start needs more than 1 s; holding the button
   through the end needs a release. Going prone blends the speed scale 0.65→0.15 over 600 ms,
   and getting up blends back over 400 ms.
4. **Weapon timing** (G3, `FUN_008e0780`). Perks don't shorten data; the state's countdown runs
   at 1/multiplier. The arsenal's timers run in weapon-data time, so the viewmodel's clip
   times, read from them, already play at `rate / mult` as the client does (`FUN_0052b1d0`).
   `Arsenal::anim_rate` exposes the factor for anything wall-clocked. Fast Hands now runs the
   normal raise/drop at ×2 instead of swapping in the quick animations. The grenade cook fuse
   stays real time. Quickdraw divides the ADS rate by 0.5.
5. **Recoil** (G5): the hip/aimed view-kick selection (aimed only at ADS == 1), pitch always
   up, roll −0.5·yaw, the minimum magnitude, the grip's accumulator with
   `fAdsRecoilReturnRate`, and reduced kick. The gun-kick impulse plus `GunKickSpring`
   (5 ms steps) are ready for the viewmodel. **Flinch** (G7): damage × 0.175, ×0.25
   Toughness, clamped 0.5–90°, as a new `Flinch` event. The sim has no damage direction, so
   it takes the engine's pitch-only branch. **Spread** (G6): stance min/max blended by eye
   height, ×0.65 with bulletaccuracy (`Arsenal::spread_cone`).
6. **Gun position** (G4): `weapons/gunpos.rs` is a pure client-side module covering the
   stance/move/strafe/sprint offsets at the weapon's blend rates, the fade by half-ADS, the
   wide-FOV pull-back, the landing dip, BO2's bob waveform with ADS damping, sway and the kick
   spring. Track R consumes it. The weapon table carries every field it needs: 17 new keys,
   now 64.
7. **Other perks.** Flak Jacket: `perk_flakJacket` 35% of the thrower's own blast
   (`_class.gsc:1023-1032`). Tactical Mask: the thrower's own concussion ×0.1
   (`_weapons.gsc:1375-1376`, a script constant, `StunProtectionScale`). Scavenger and Hacker
   are tested by specialty. **No system in the sim:** Ghost, Blind Eye, Hardline, Hard Wired,
   Cold Blooded and Awareness (no UAV, airstreaks, momentum, EMP or enemy footsteps for them
   to act on), Dead Silence (the General's footsteps are his own 2D cues), fast mantle and
   ladder climb (no mantling or ladders on Zero Hour maps). Their bits are carried and tested
   nowhere yet.

**Pending** (left as before, cited): the fall-damage curve and `fallheight` (G2 unresolved);
proof that the sprint base is `player_sprintTime` (G2 medium); how the engine converts and
recentres view kick (G5 unresolved, so `KICK_TO_DEGREES` and the client's settle remain);
`aimSpreadScale` dynamics (the bloom model remains); the ADS move base (the converter's
`ADS_MOVE_BASE` remains); the two `playerAnimType` values exempt from quickdraw.

## Integrating Track PW: the autotest single-instance race (2026-10-21)

The merge conflicted only in MODLOG.md (both sides appended); both kept. Archive VERSION 45 → 46
for the converted weapon table (17 new keys, now 64). Verified: cargo test --workspace green
(zhbo-core 298; zhbo-assets 39 + 7 on the real install), `cargo xtask build` clean, in-game
autotest 13/13 default and 14/14 `--rts`.

Gotcha: two autotest runs back to back can fail the second. The first game's process has not
fully exited when the next launcher checks the single-instance lock, so it bails with
"Generals is already running...Bail!" on stdout; the script's `rm -f` of the userdata log then
leaves `wait_for_log` watching a log that never appears ("the script never reached first
person"). Re-run the failed mode standalone.
## Track R, round 2: the grey wash-out was linear light (2026-10-21, branch `v1/track-r2`)

Root cause of finding #1 above: `native.wgsl` had ported `bo2Shade` to *linear* light — colour
maps sampled through sRGB views, lights and ambient sRGB-decoded, the result re-encoded for the
scene target. But bo2Shade was tuned in game in display (gamma) space, and in linear light its
environment term (a sky/ground reflection scaled from the ambient and sun levels) keeps roughly
its gamma-space strength while a dark albedo's diffuse collapses (42/255 ≈ 0.165 display → 0.024
linear): the grey sky reflection swamps the finish's hue. It only bites under a bright map
ambient (Zero Hour maps go well past the GameData defaults — the wash-out map's ambient is past
1), which is why checking the day lights (all ≤ 1) found nothing. Fix: shade in display space,
exactly like bo2Shade and like the scene the pass draws into. Colour maps upload raw (the core's
`gpu.rs` and the renderer's `white` fallback drop their sRGB views), lights and ambient are Zero
Hour's display-encoded values used as-is, and the result is written straight; only when the target
view is itself an sRGB format is it decoded first, since the hardware re-encodes on write
(`encode_srgb` inverted to `decode_output`).

Validated headless on Metal: the new `a_dark_finish_stays_dark_under_a_bright_ambient` test (the
AN-94's average finish 42/39/35 with its packed maps, ambient 1.2) renders **RGB 134/133/132 —
exactly the reported light grey — against the old shader**, and RGB 93/90/86 with its hue on the
new one; `a_dark_finish_keeps_its_hue_under_zero_hour_light` (olive 70/80/40, the day lights)
passes on the new shader (85/94/56). With the real converted AN-94 maps
(`nativevm::tests::diag_real_maps`, `#[ignore]`d) and the full viewmodel from the archive
(`diag_real_viewmodel`, ambient 1.2) the gun renders its dark olive/metal finish, not grey.

Also this round, chasing the other half of the finding ("the gun sits a little higher"):
- `native_pose_matches_the_w3d_rig` (zhbo-assets, `#[ignore]`d, needs the BO2 install): the
  native pose and the verified W3D rig must agree bone by bone (< 0.01 units) across weapons and
  frames. **Not run here — no BO2 install on this machine; integrator.**
- `ZHBO_NATIVE_COMPARE=1` (`ZhboViewmodel.cpp`, with `ZHBO_NATIVE_VIEWMODEL=1`): poses the W3D
  model hidden alongside the native one and logs both paths' `tag_weapon`/`tag_flash` once a
  second. **Not compile-checked here (no game build on the branch); integrator.**
- The diag tests take `ZHBO_DIAG_DIR` (extracted `an94_col.tga`/`an94_nsg.tga`),
  `ZHBO_DIAG_BIG`/`ZHBO_DIAG_OUT` (`ConquerOps_BO2.big` → PPM; `ZHBO_DIAG_AMB` scales the
  ambient).

## Track R, round 2b: R4's back-face culling (2026-10-21, branch `v1/track-r2`)

Every surface now culls the faces its material's `cullFace` says to, end to end:
`zhbo_formats::xmodel::cull_of` reads `GfxStateBits::loadBits[0]` bits 14-15 (`GFXS0_CULL_*`: 1
none, 2 back, 3 front, 0 undefined → none; values from OAT's T6 `GfxStateBitsEnum`,
git.alterware.dev/zone/OpenAssetTools commit 28011d6b64 — the same source `t6zone/layouts.rs` was
generated from), the converter writes it into the `.zbm` materials (blob **version 2**), the core
carries it through `SurfaceSpec`, and the renderer keys pipelines on it.

- **Winding gotcha:** BO2 and the engine use opposite front-face winding (verified in game back
  on the W3D conversion, `character.rs`: unswapped BO2 order "culls the outside of the model").
  The native blob keeps BO2's own index order, so the pass's pipelines use `FrontFace::Cw` —
  confirmed headless against the real AN-94: with a v1→v2-upgraded archive (cull = back on the
  opaque surfaces) `Cw` renders the receiver's dark exterior correctly while `Ccw` shows the
  chalky inside. The headless quads were CCW (engine-convention); they are re-wound to BO2's
  order so the lit tests still exercise front faces.
- `culls_the_faces_the_surface_says` (headless): back/front/none on both windings.
- **The archive VERSION (`hud::VERSION`) is deliberately not bumped on this branch** (integrator
  serializes version bumps): a v1 archive's blobs are refused with "reconvert the BO2 art" and
  the viewmodel falls back to W3D until the integrator reconverts. Verified against the local v1
  archive: clean refusal, no draws, no wgpu errors.
- Reflection probes (the rest of R4) stay out: they need the zone loader to keep
  `GfxImageLoadDef` data (`XFILE_BLOCK_TEMP`), the dead end already recorded above ("BO2's own
  reflection probes" backlog item).

## Integrating Track R round 2: display-space shading verified in game (2026-10-21)

Merged `v1/track-r2` (3 commits): the grey wash-out fix (bo2Shade had been ported to linear
light but was tuned in Zero Hour's display (gamma) space; under a bright map ambient the
environment term kept its gamma strength while a dark albedo's diffuse collapsed, swamping the
hue) and R4's back-face culling (`cullFace` from the material state bits, zbm blob v2,
`FrontFace::Cw` anchored three ways: the in-game-verified opposite winding, the engine path's
own setting, a headless A/B on the real AN-94). MODLOG.md was the only conflict; both sides
kept. Archive VERSION 46 → 47 (zbm v2; old archives refuse cleanly and fall back to W3D).

Verified: cargo test --workspace green; clippy clean in every Track R file (30 pre-existing
warnings remain in unowned files); `cargo xtask build` clean (first compile of the round-2
bridge changes); in-game autotest 13/13 on the W3D path and 13/13 with
`ZHBO_NATIVE_VIEWMODEL=1 ZHBO_NATIVE_COMPARE=1`. The compare log shows the native and W3D rigs
agreeing on `tag_weapon`/`tag_flash` to ≤ 0.006 units across clips and the whole ADS blend.
Screenshots: the AN-94 and TAC-45 render dark gunmetal/olive again — no grey wash-out — and no
inside-out surfaces from the culling.

Residual, for the next Track R round:
- Art scale is still unreconciled: `zhbo_native::ART_SCALE = 11/72` vs the sim's
  `ZH_PER_INCH = 1/6` (12/72) — 9% off. Invisible in first person; must be settled before R7
  (third-person/world placement).
- One frame under enemy fire showed a magenta blob at the screen's right edge — world-side FX
  (the native pass has no magenta placeholder); watch it in the next playtest.
- Reflection probes (the rest of R4) stay skipped: the `XFILE_BLOCK_TEMP` dead end stands.
- `native_pose_matches_the_w3d_rig` still needs the BO2 install to run ("the gun sits a little
  higher"); the compare mode's two-tag log is the in-game substitute so far.
## v1.0 Track C, first wave: the choose-class screens on BO2's own geometry (2026-10-21)

The pick screens were invented: a self-drawn banner/pager/details panel, presets first, a CREATE A
CLASS row the real game never had. This wave ports them to the Lua (`classui/bo2.rs`, `pick.rs`,
`preview.rs`), in LUI units resolved through Track 0's `lui` layer:

1. **The lobby is `CACChooseClass`** (`decompiled/patch_mp/ui_mp/t6/menus/cacchooseclass.lua`):
   the CHOOSE CLASS title (`addTitle`, Big), the `‹ CLASS SET n ›` header (35 tall at
   `TitleHeight + 10`, ExtraSmall name over `menu_vis_carousel_pip[_fill]` pips), the active set's
   **five custom slots and nothing else** (`UpdateButtonList`: 30-tall CoD9Buttons,
   `ButtonSpacing` 2, list width `width / 3 - 50 - 20 = 218`), BO2's `MENU_CLASS_SETS_HINT` under
   the list (`hinttext.lua`'s `ui_arrow_right` + wrapped Default), and the `ClassPreview` from
   `width / 3 - 50` to the right edge. Untouched slots are BO2's `Custom 1`..`Custom 5`
   (`a-chooseclass/002`: set 2's five empty slots; `reset_classes.cfg` names them `CLASS_SLOTn`),
   so a profile blank name now saves as `Custom n`, not `Custom Class n` — the profile, the pick
   rows and the command-bar labels share the one namer (`classui::empty_slot_name`).
2. **The match is `changeclass`** (`decompiled/patch_mp/ui_mp/t6/hud/changeclass.lua`): the same
   title, a 250-wide list at `TitleHeight` holding the set's name (Condensed), a half-button
   spacer, the five slots, another spacer and the default classes in BO2's order — `class_smg` 10
   through `class_sniper` 14 (`AddDefaultClassButtons`) — and a `Menu.Width - DefaultWidth * 1.5`
   preview. An untouched slot is inert in a match (BO2's empty slots would spawn an empty class;
   the mod's profile only stores saved customs, so a no-op beats a silent knife-only spawn).
3. **The preview is `ClassPreview`** (`patch_mp/ui/t6/classpreview.lua`): the `n/10` counter
   (Condensed, `CoD.orange`, right-aligned 100 wide) over the ten-bar `AllocationBar`, then the
   columns — left `(W - 4) * 0.53` (primary block half its width tall, its attachment row, the
   secondary, its row, 71 tall), right `(W - 4) * 0.47` (three perk rows, LETHAL, TACTICAL), then
   WILDCARDS across. Black 0.1 plates, slot names ExtraSmall at 0.5. The focused list button
   pulses `BOIIOrange` 1 → 0.5 and back every 650 ms (`cod9button.lua` `PulseDuration`, eased
   through Track 0's `lui::anim`); the hosts feed it their UI clocks (`Client::ui_clock` from
   render dt, the shell menu a 60 Hz step — UI only, never the sim).
4. **Chrome is BO2's materials now** (`lui::materials::MATERIALS`): `menu_mp_cac_backing` (8
   tiles, alpha 0.7) and the two `menu_mp_backing_frame` rules of `addLargePopupBackground`
   (alpha 0.65, 16 tall around a 605 band lifted 19). The bottom rule is the top one drawn with
   its LUI top below its bottom — a vertical mirror — so the converter writes a `_flipv` copy
   (`hud.rs`'s `flip_vertical`); a `FLIP_V` name suffix is the convention.
5. **CaC weapon pictures** come from the player's `mp/statstable.csv` (patch_mp.ff): column 4 is
   the reference (`mp7`), column 6 the image (`menu_mp_weapons_mp7`); `classpreview.lua` draws
   `<image>_big`. New `zhbo-formats::stringtable` parses the zone's `StringTable` asset; the
   converter emits `ZHBO_CaC_Weapon_<base>` (`names::cac_weapon_image`) from the `_big` material.
6. **The screenshot harness** installs the archive's BO2 font metrics (`names::FONTS_PATH`) like
   the bridge does at init, reads every MappedImage INI in the archive (the fonts' glyphs live in
   their own), takes `--size WxH` for the captures' 3024×1890, and the new `build-archive` dev
   subcommand writes the launcher's BO2 archive to a chosen path so the harness can be iterated
   without launching the game.

Gotchas:
- **The digit keys follow the rows.** The match list puts the set's slots first, so key 1 is the
  first custom now (was the Assault preset). Every scripted pick derives its key from the live
  list (`autopilot::custom_class_key`: the first custom, else the first default class — never a
  hard-coded index). The classpick autotest's "5 premade + the custom" doc note is updated.
- **`map_or(true, …)` → `is_none_or`, `chunks_exact_mut(4)` → `as_chunks_mut::<4>().0`** (clippy
  1.99 in the touched files).

Tests: `cargo test -p zhbo-core -p zhbo-assets` green (258 + 37), clippy clean in every touched
file. **Not yet run in game** (the integrator runs the game): the classpick autotest's
expectations changed with the row order. Integration needs: archive VERSION bump (new CaC
materials/images in `ConquerOps_BO2.big`).

Remaining Track C scope: the editor/picker screens on the same LUI geometry (they still draw the
old self-drawn chrome), the cue map wired to the exact Lua call sites (`cac_main_nav`,
`cac_main_edit_loadout`, `cac_safearea`, `cac_main_exit_cac`, … — the aliases exist in
`sounds::UiCue`), the LUI transitions (the 250 ms slide/fade of `SetActiveClassSet`), the pixel
pass against `cac/sweep/`, and the in-match class panel per `hud/class.lua`.

## v1.0 Track C, second wave: the editor and the pickers on LUI geometry (2026-10-21)

`screens.rs` had grown past the ~1,500-line hygiene limit and the editor was still drawing the
old self-drawn chrome at engine font sizes while the pick screens had moved to `bo2.rs`'s LUI
frame. This splits it and puts the editor and both pickers on the same geometry:

1. **`classui/editor.rs` is the editor** (`CoD.CustomClass`, `patch_mp/ui/t6/customclass.lua`):
   the class name as `addTitle`'s heading, the allocation bar under it (element top 3, 10 tall,
   the menu's width), the left column PRIMARY (151) / ATTACHMENTS / SECONDARY / ATTACHMENTS and
   the right one PERK 1-3 / LETHAL / TACTICAL (`ButtonHeight` 71, `MajorGridSpacing` 9,
   `LeftSideButtonWidth` 500 at 0, `RightSideButtonWidth` 355 at 509), WILDCARDS across, the
   mod's SAVE CLASS under them. Slot plates are `CoD.SlotList`'s (`SlotWidth` 64 × 1.4, `Spacing`
   15): **three on a primary/secondary row, two on everything else** (`slotlistgridbutton.lua`;
   the count is the button's kind, not the number of filled slots). `screens.rs` is down to the
   host view (`ScreenView`/`draw_screen`); `layout.rs` is geometry only; `chrome.rs` keeps the
   shared bits (prompts, deny rim, `allocation_lit`, element names, weapon stats).
2. **The pickers are BO2's carousel and grids** (`cacweaponsp.lua`, `menus/cacattachmentsmenu.lua`,
   `cacperksp.lua`): the highlighted gun is the grown card (`HighlightedItemWidth` 345.6 ×
   `HighlightedItemHeight` 217.8) and its successors stream right as `ItemWidth` 192 × 146 tiles,
   clipped at the menu's edge; the option grids are 6 columns of `GridItemWidth` × `64 + Title +
   10` for attachments and 3 columns of `96 + Title + 10` for the rest, `GridItemSpacing` 8.
   `picker_window`/`picker_option_at` are the one window both the draw and the hit-test read, so
   a click can never land on a different cell than the one drawn.
3. Two fixes came with it: the option grid's draw loop added the window's first index
   twice (labels and the orange highlight walked off the visible page — invisible today only
   because no list is longer than its page at 864×648, but wrong the moment one is), and
   `editor_frame` built the option list every frame and threw it away.

Gotchas:
- **`editor_layout` takes no scale now.** It resolves the LUI frame (`bo2::Frame::new`) and every
  box is in root units; `Frame::px` is the only place units turn into pixels. Anything holding a
  screen-space rect across a resize is a bug (the old layout took `s` and pre-multiplied).
- The grid's `GridTop` is 80 in the captures (`d-attach/002`) against the Lua's 60: the captures
  are what the pixel pass matches.

Tests: `cargo test -p zhbo-core -p zhbo-assets` green (261 + 37), clippy clean in every touched
file. New: the carousel and the option grid hit-test every cell they draw, and a drawn option
grid is asserted to label the window's options with exactly one `BOIIOrange` highlight.

Remaining Track C scope: the sound cues wired to the exact Lua call sites (`cac_main_nav`,
`cac_main_edit_loadout`, `cac_safearea`, `cac_main_exit_cac`, `cac_slide_*`, `cac_wildcard_*` —
the aliases exist in `sounds::UiCue`, the screens still play the five coarse ones), the LUI
transitions (the 250 ms slide/fade of `SetActiveClassSet`, the screen fade), the pixel pass
against `cac/sweep/`, and the in-match class panel per `hud/class.lua`.

## v1.0 Track C, third wave: the screens play the Lua's own sound cues (2026-10-21)

The screens had a handful of coarse cues (`UI_OPEN`/`UI_NAVIGATE`/`UI_SELECT`/`UI_BACK`/`UI_DENY`) played
by rules the hosts invented. BO2's menus do not work that way: every widget and handler names the
exact alias, and the same alias is reused across screens. `classui/cues.rs` now names them,
`classui/input.rs`'s folds decide which one a frame calls for, and the hosts only play what they
are handed:

- **The lobby** (`cacchooseclass.lua`): a row gaining focus is `cac_main_nav`
  (`classButtonList.gainFocusSFX`); choosing a class to edit is `cac_main_edit_loadout`
  (`ChooseClass`); the screen's back-out is `cac_main_exit_cac` (`setBackOutSFX`); the set pager is
  `cac_safearea` (`Prev`/`NextClassSet` — the alias is not a safe-area sound); entering create-a-
  class from the lobby is `cac_enter_cac` (`gamelobby.lua`); a dialog opening is
  `cac_loadout_edit_sel` (`popup.lua`'s `OpenPopup`).
- **The editor** (`caceditclass.lua`, `slotlistgridbutton.lua`, `attachmentgridbutton.lua`,
  `bonuscardbutton.lua`, `cardcarousellist.lua`): opening the weapon picker is
  `cac_loadout_edit_sel` (`WeaponSelectSFX`), any other submenu `cac_loadout_edit_submenu`
  (`SubmenuSelectSFX`); a gun slot's focus is `cac_loadout_nav` and the bonus-card strip's
  `cac_loadout_wildcard_nav`; a carousel move is `cac_slide_nav_lr`/`_up`/`_down` and its equip
  `cac_slide_equip_item`; a grid's move and equip are `cac_grid_nav`/`cac_grid_equip_item`; an
  attachment is `cac_equipment_add`; a bonus card is its own `cac_wildcard_equip_1..7` (Tactician,
  BO2's `bonuscard_two_tacticals`, has no alias and falls back to the grid action, as the Lua's
  table lookup does); `R`/the `X` is `cac_cmn_item_remove`; backing out of a picker or the editor
  is `cac_cmn_backout`; a refused choice is `cac_cmn_deny` **alone** (the Lua's handlers play
  `denySFX` instead of the action sound, so the fold returns no second cue).
- **In a match** (`hud/class.lua`, `hud/changeclass.lua`): the panel opens with `uin_main_pause`,
  rows use the grid's nav/equip, and leaving it is `cac_cmn_backout`. The `ClassChosen` sim event no
  longer plays a cue: BO2 applies a class change silently on spawn, and the pick already played the
  equip.

Gotchas:
- **`sounds::UI_*` is gone.** The alias-of-alias constants hid which Lua site a sound came from.
  Use `classui::cues::*`; the converted event name is still `ZHBO_UI_<alias>`, so no archive change.
- **Cues are decisions, not observations.** A denied action returns `cue: None`; a host that plays
  `frame.cue` and its own deny sound must not also play a back-out for the same frame.
- **Content grids walk Up/Down only.** The pure editor has no per-cell left/right links yet, so
  BO2's 3-wide grids (a Right that stays in the row, a page step at the edges) are still to come;
  the carousel is the only 2-D picker today.

Tests: `cargo test -p zhbo-core -p zhbo-assets` green (267 + 37), clippy clean in every touched
file. New: the cue table is asserted to be BO2 aliases and distinct per action, and the editor's
walk is asserted to play the Lua's cue at each site (weapon select, slides, grid nav/equip,
equipment add, the card's own alias, remove, back-out, and the deny alone).

Remaining Track C scope: the LUI transitions (`SetActiveClassSet`'s 250 ms slide, the screen's
fade, the highlight tweens — only the focused row's 650 ms pulse is in), the content grids'
per-cell left/right links, the pixel pass against `cac/sweep/`, and the in-match class panel per
`hud/class.lua` (the change applying on the next spawn is the sim's already; the panel that shows
the active class in a match is not ported).

## v1.0 Track C, fourth wave: the class-set switch moves with the Lua's 250 ms (2026-10-21)

`SetActiveClassSet` (`cacchooseclass.lua`:500) is the one transition the choose-class screen has,
and the port drew the new set as a hard cut. It now runs the Lua's two animations:

- **The list slides in** from `-320 - buttonListWidth - 20` to its resting place with
  `beginAnimation("onscreen", 250, false, true)` — an ease-out over the list's width plus 20 — and
  is clipped to the menu while it travels so it cannot paint over the backing frame.
- **The preview fades in** over the same 250 ms (`fade_in`, `250, true, false` — an ease-in) by
  one alpha multiplier over the whole block, the way LUI's `alphaMultiplier` works; `bo2::fade`
  scales a prim group (image fallbacks included).

The hosts own the clock: `ScreenView` carries `set_change: Option<f32>` (seconds since the
switch), `MenuState` starts it on `cycle_active` and clears it once 250 ms have run at its 60 Hz
step, and the in-match pick reports `None` (a match never switches sets). `draw_pick` takes the
`ScreenView` now instead of seven loose arguments, so the motion cannot be half-wired.

Gotchas:
- **A fade is a group, not a colour.** Prims carry their own alpha; fading an element means
  drawing it into its own list and scaling that list, not passing an alpha down the call chain.
- **Settled means byte-identical.** Outside a switch the draw must be exactly the static layout
  (no clip prim, no fade list), or the screenshot harness's captures stop matching; the test
  asserts both.

Tests: `cargo test -p zhbo-core -p zhbo-assets` green (269 + 37), clippy clean in every touched
file. New: at settle 0 the list's first row sits a list-width plus 20 to the left of its resting
pen and the preview starts at alpha 0, at settle 1 both are static and unclipped, and the menu
starts/clears the transition on a set switch but not on a row move.

Remaining Track C motion: the picker/submenu open and close (BO2 fades a menu in on `openMenu`),
the screen's own entrance, and eased highlight moves between rows (only the focused row's 650 ms
`BOIIOrange` pulse is in). Remaining Track C scope otherwise: the pixel pass against `cac/sweep/`
and the in-match class panel per `hud/class.lua`.

## v1.0 Track C, fifth wave: the in-match panel opens on the class you wear (2026-10-21)

BO2's in-game class panel is the pause menu's Class tab (`hud/class.lua`) and its changeclass
screen focuses the class the player is wearing — `Button_RowGainFocus` runs on the active class,
and the preview starts on it. The mod's in-match pick always opened on row 0, so a classed player
pressing the command bar's entry saw the first slot focused and could not tell what they wore.

`Client` now derives the row from the sim's class id when the panel opens
(`current_class_row`: a shipped class by registry id, a custom by `custom:<name>` against the
active set's profile list) and focuses it. The sim's `choose_class` is unchanged.

On "applies on next spawn": the sim already applies a pick as it lands, which is right for the
screen's real use — BO2's `changeclass` IS the spawn screen, and the mod's unclassed spawn offer
is that screen. The command bar's mid-life entry is a mod affordance BO2 has no equivalent for, so
its pick applies at once (with the kit's first raise) rather than waiting for a respawn; a
`PlayerSim::pending_class` applied by `take_control` would be the way to get the pause-menu
version, and it would change the classpick autotest's mid-life expectations. Left to the
integrator as a design call.

Tests: `cargo test -p zhbo-core -p zhbo-assets` green (270 + 37), clippy clean in the touched
files. New: a two-custom profile, the second picked by its row key, the panel closed and forced
open again — the highlight is the worn class's row.

## Integrating Track C: the class screens on BO2's geometry, art and cues (2026-10-21)

Merged `v1/track-c` (6 commits): the choose-class screens and the Pick-10 editor on BO2's own
LUI geometry, materials and art; the editor split out of the oversized screens.rs into
`classui/editor.rs`; the Lua's exact sound cue per call site (cac_* / uin_*); the 250 ms
class-set slide + fade; the in-match panel opening on the class the General wears.

Conflicts: MODLOG.md (both kept); `lui/materials.rs` (H's HUD entries + C's CaC entries, one
array, C's `FLIP_V` kept); `zhbo-assets/hud.rs` (H's inline-material scan with C's zone order,
adding `ui_mp`); `hud/tests.rs` (H's looser crosshair assertion kept). `stringtable.rs` was an
add/add — two independent StringTable readers, PW's null-guarded `rows()` and C's richer
`Table`/`find`/`rows_where`; both kept verbatim, to be unified later with the real install as
arbiter. The compiler then caught one semantic conflict: C's `bo2::fade` pre-dated H's
`Prim::Quad`; it now scales Quad's alpha and recurses its fallback like Image. Archive VERSION
47 → 48 for the new CaC materials and `ZHBO_CaC_Weapon_*` images.

Verified: cargo test --workspace green; the real-install suite green EXCEPT the pre-existing
`xmodel::tests::reads_an_mp_character` failure ("weights sum to 1", broken before this merge —
only `stringtable.rs` differs from HEAD in that crate; fix pending on branch
`fix/xmodel-weights`); clippy clean in the touched crates; `cargo xtask build` clean; in-game
autotest 13/13 default, `--classpick` all checks (the seeded custom picked, its MP7 kit
applied, the pick not re-saved), `--rts` 14/14.

Watch: cues are now per call site, so menus sound different (the lobby's Escape is
`cac_main_exit_cac`, a set step `cac_safearea`, the in-match panel's entrance `uin_main_pause`).
`GridTop` is 80 in the port vs 60 in `cacattachmentsmenu.lua:8` — first suspect if the reference
pixel pass disagrees. `sim::choose_class` still applies a mid-match pick immediately; BO2's
pause-menu "next spawn" semantics would need a `PlayerSim::pending_class` (deferred, see the
Track C section above).

## Fix: XModel blend weights are u16 fixed point, not half floats (2026-10-21, branch `fix/xmodel-weights`)

The ignored real-install test `xmodel::tests::reads_an_mp_character` failed at "weights sum to 1"
on `c_usa_mp_fbi_assault_fb`. Bracketed with the player's install: green at 27f274b (the reader's
introduction), red from **48ad512** on — that commit ("half-float blend weights per OAT") switched
the secondary weights of `XSurfaceVertexInfo::vertsBlend` from `val/65535` to `half_to_f32`,
misreading OAT: its `BoneWeight16` (`ObjWriting/XModel/XModelToCommonConverter.cpp.template`) is
`value / std::numeric_limits<uint16_t>::max()`. The prime suspect e60b814 (raw offsets → generated
layout constants) was innocent: every constant it adopted in xmodel.rs, xanim.rs and sndalias.rs
matches the old hand-maintained offset exactly. Against the real bytes the u16 reading is exact
(bone slots all multiples of 64 and in range, secondaries ≤ 0.5, per-vertex secondary sums ≤ 0.68);
the half-float reading landed hundreds of vertices per surface past a 1.01 secondary sum (max: inf),
the implicit first weight clamped to 0 and the total blew past 1.01. The half-float companion
asserts 48ad512 added had never passed — it landed red. Fix: read `val as f32 / 65535.0` again; the
secondary-range check now spans the whole LOD because some surfaces are rigid gear with no blend
stream. **Half floats in `GfxPackedVertex` are only the packed texcoords.** This also retires the
Track R milestone-1 gotcha above about viewhands' "non-finite half-float weights": weights never
decode non-finite; `native.rs`'s neutralizing filter stays as a defense but no real vertex hits it
for weights.

## Integrating fix/xmodel-weights: the skin weights were u16 fixed-point, not half floats (2026-10-21)

`48ad512` (the format-reader hardening) read `XSurfaceVertexInfo::vertsBlend`'s secondary-weight
u16 slots through `half_to_f32`, citing OAT — but OAT's `BoneWeight16` is `value / u16::MAX`,
plain fixed point. The misread pushed hundreds of vertices per surface past a 1.01 weight sum
(0x5600 reads as 96.0, 0xFFFF as -inf) and `reads_an_mp_character` failed from that commit on;
its companion asserts had never passed (rigid gear surfaces have no blend stream). The fix
(`fix/xmodel-weights`, `4075a9a`) reads u16/65535 again, corrects the comments, re-encodes the
synthetic test to exact fixed-point values, and aggregates the secondary-range assert across
the LOD. `e60b814`'s generated-layout adoption was exonerated field by field (xmodel, xanim,
sndalias all match the old hand offsets).

The converted native blobs' skin weights change (the non-finite filter used to neutralise these
vertices): archive VERSION 48 → 49.

Verified: cargo test --workspace green; the FULL real-install suite green (zhbo-formats 8,
zhbo-assets 7); clippy clean in the touched crates; `cargo xtask build` clean; in-game autotest
13/13 default, 13/13 with `ZHBO_NATIVE_VIEWMODEL=1 ZHBO_NATIVE_COMPARE=1`, 14/14 `--rts`.

## Track R6: the sights sit on the shot's own axis (2026-10-21, branch `v1/track-r6`)

`nativevm::placement` slid the aim marker onto the view axis, but the gun's offset and angles came
from `viewmodel::Motion`'s own guesses (`BOB_SIDE`, `KICK_BACK`, `SWAY_*`, `KICK_UP`…), so the
"hip pose" and the ADS damping were not BO2's. R6 makes the weapon def the source: `Motion` now
runs `weapons::gunpos::GunPos::step` each frame with the local player's stance, speed, strafe, aim,
view angles, bob cycle and sprint weight, so the hip pose is the def's `vStandMove`/`vDuckedOfs`/
`vStrafeMove`/`vSprintOfs` (and their rotations), sway and the gun kick spring are the def's own
(`sway*`/`adsSway*`, `f{Ads,Hip}GunKick*`), and `fAdsAimPitch`/`fAdsBobFactor` apply. The shot's gun
kick impulse — `EventKind::Recoil.gun_kick`, which the client used to discard — now feeds G5's
spring. Presentation only: no sim, ABI or asset change.

The placement itself moved to `nativevm::place` (`aligned`/`root`/`sight`), unchanged in intent: the
eye frame is the sim's own (`Body::eye()` + the view angles `dir_from_angles`, the axis `sim::fire`
traces along), and the aim fraction (BO2's `fWeaponPosFrac`, which the arsenal moves at the def's
ADS in/out times) slides `tag_sights`/the optic's window/the front sight onto that axis, its forward
distance kept. The unit test builds the eye and direction the way `sim::fire` and `Body::eye()` do
and asserts the sight's distance to the *ray* at ads 0 / 0.5 / 1 (2" of hip offset → 1" → 0) and
that its along-ray distance never changes.

Gotchas found on the way:

* `gunpos`'s output is (pitch, yaw, roll) **degrees** and (forward, **right**, up) inches; the
  renderer's `GunPose` is (yaw, pitch, roll) **radians** and (forward, **left**, up) art units.
  Both conversions live in `viewmodel::Motion::update` — swapping either mirrors the whole pose.
* `GunPos::sway`'s rotation had the opposite sign from its offset: the gun *led* the view while its
  position hung behind it. An arm cannot turn faster than the camera, so the sway angle now takes
  the same negated delta as the offset (test: `turning_sways_the_gun_behind_the_view`).
* Viewmodel offsets are in the converter's art scale (`zhbo_native::ART_SCALE`, 11/72 per inch),
  not the sim's 1/6 ZH units per inch. `viewmodel::INCH` reads that constant now.
* BO2's own ADS transition is an animation: `weapAnimFiles_t` 0x55/0x56 (`ads_up`/`ads_down`) are
  already converted into the native manifests and the W3D clips, but the clip state machine never
  asks for them. The placement slide stands in for them; when the state machine plays them (Track
  R3/R5 follow-up) the slide's visible work shrinks rather than fighting them.
* `fAdsAimPitch` pitches the gun at full ADS *while* the sight is held on the axis, so the muzzle
  sits a few degrees off the sight line — G4's own rule (`FUN_008ef760`), not a placement error.
* The camera carries the cosmetic view kick (`hud.view_kick_*`) and the sim's shot axis does not.
  The crosshair is drawn on the camera axis, so the sights follow the camera and the crosshair; the
  placement test builds the ray from the fire path's own values to keep that honest.
* Landing still uses the client's own dip: G4's `landChange` (the signed fall delta `FUN_00530990`
  takes) is not in the sim's state, so `GunInput.land_change` goes in as 0.
* G4's ADS view-origin delta (`FUN_00530af0`: `(viewOrigin - eyePos) * adsFrac` added to the gun)
  has nothing to do here: the mod's first-person camera *is* the sim's `Body::eye()`, so the delta
  is zero by construction. It would come back only if a separate ADS view origin were introduced
  (e.g. a peek/lean camera).

Verified: `cargo test -p zhbo-core -p zhbo-native` green (320 + native), clippy clean in the touched
files. In-game `ZHBO_NATIVE_VIEWMODEL=1` / `ZHBO_NATIVE_COMPARE=1` eyeball pass left to the
integrator (see the track's handoff note).

## Integrating Track R6: the ADS sight picture on the shot's own axis (2026-10-21)

Merged `v1/track-r6` (1 commit): `nativevm/place.rs` slides the gun in the eye frame so the aim
marker's lateral/vertical offsets shrink to zero by ads = 1 (BO2's `fWeaponPosFrac` blend; the
fraction moves at the def's `fAdsTransInTime`/`fAdsTransOutTime`), the marker being the
converter's `AIM_TAG` (`tag_sights`, or the optic's window when one is mounted — same path).
The axis is the sim's own (`Body::eye()` + `dir_from_angles` — the ray `sim::fire` traces).
`viewmodel::Motion` now runs `weapons::gunpos` every frame (stance/strafe/sprint offsets, sway,
ADS bob damping, the gun-kick spring fed by the recoil event's previously-discarded `gun_kick`),
and a sway rotation-sign bug is fixed. Clean merge (fast-descendant), no ABI/VERSION change.

Verified: cargo test --workspace green; real-install suite green; clippy clean; `cargo xtask
build` clean; in-game autotest 13/13 default, 13/13 native+compare, 14/14 `--rts`. Compare log:
≤ 0.013 at full ADS and 0.000 at neutral hip; ONE sample pair diverges 0.83 at ads 0 mid-throw
— the native rig plays BO2's real Motion offsets where the W3D rig has none. That is R6's
intended difference: **the compare tolerance only binds at neutral Motion and at ADS from now
on.** Eyeball: the throw and hip poses read naturally in the native frames. The arms swinging
through the ADS transition stands in for the `ads_up`/`ads_down` clips (converted into the
manifests, never yet requested — a Track R3/R5 follow-up).
## Reconciling the art scale with the sim's units (2026-10-21, v1/art-scale)

The two scales left open after Track R2 are one scale after all. `units::ZH_PER_INCH` (1/6) is the
sim's BO2-inches → ZH-world-units factor, fixed by CONTRACT's "1 unit = 6 inches; `GeometryHeight`
12 ≙ a 72-inch player". `zhbo_native::ART_SCALE` (11/72) was a hand-copied *art* factor introduced
with the W3D character conversion and then reused verbatim by the native viewmodel and
`viewmodel::INCH`. It is not a second unit; it was 9% short. Evidence from the two installs:

- BO2: `zhbo-assets model zone/all/faction_seals_mp.ff c_usa_mp_seal6_assault_fb` reports bounds
  z -0.12"..72.15" — the player body is exactly 72 inches.
- ZH: stock infantry `GeometryHeight` is 12–13 (Ranger 13, Burton 12, GLA Rebel 12), per INIZH.big;
  their *mesh* heads sit at 14.3–16.0 units (measured from each model's hierarchy bind transforms
  and vertices in W3D.big/W3DZH.big — the skinned mesh is bone-relative, and the repro matches each
  mesh header's Min/Max exactly). The collision cylinder, not the stylised head height, is the
  documented body size.
- The mod's own eye: `inches(60) × GENERAL_SCALE` = 11.5 units. At 11/72 the converted body's
  60-inch eye is 10.54 units, so the camera floated 0.96 units (5.7") above the body's eye line —
  invisible in first person, wrong the moment R7 puts the body in the world. At 1/6 both are 11.5.
- `GENERAL_SCALE` 1.15 × 12.02 units = 13.8, next to the General's `GeometryHeight = 14`; at 11/72
  it was 12.7, a unit below its own collision cylinder.

Fix (one source, no ABI change): `zhbo_native::ZH_PER_INCH = 1/6` is now the definition; `ART_SCALE`
is an alias of it. `units::ZH_PER_INCH`, `nativevm::ART_SCALE`, `character::SCALE`,
`viewmodel::INCH`, and the effect gravity constant read it instead of copying a number; footstep
spacing keeps its tuned 9 and 11.5 units, now expressed through `units::inches` (54", 69"). Tests pin the
factor and both boundaries (`zhbo-native`, `units`, `character`, `nativevm`). CONTRACT ("Units") and
ARCHITECTURE (zhbo-native) say where the number lives.

**Consequences for the integrator:** the W3D conversion bakes the scale, so existing
`ConquerOps_BO2.big` files must be rebuilt — `zhbo_assets::hud::VERSION` (49) needs a bump, left
unset here because this task forbids VERSION bumps; without it the launcher keeps the old 11/72
W3D art. The native blobs store inches, so they are unaffected by reconversion. First person
changes slightly by design: gun geometry, bob/recoil offsets and the viewmodel pose all use the
corrected factor (9% bigger). Not run in game here (integrator runs the build/autotest).

Verified: `env -u ZHBO_BO2_DIR -u ZHBO_ZH_DIR cargo test -p zhbo-native -p zhbo-core -p zhbo-assets`
green (native 18, core 317, assets 41 + 8 ignored); the real-install conversion
(`generals::tests::converts_from_real_install`, `ZHBO_BO2_DIR`) green, 1,578 native files;
`cargo check -p zhbo-core --features renderer` green; clippy has no warnings in the changed lines
(one pre-existing `chunks_exact` warning in `fx.rs` remains, unrelated).

## Integrating v1/art-scale: one ZH-units-per-BO2-inch factor, 1/6 (2026-10-21)

Merged `v1/art-scale` (1 commit). `zhbo_native::ZH_PER_INCH = 1/6` is now the single definition;
`ART_SCALE`, `units::ZH_PER_INCH`, `character::SCALE`, `viewmodel::INCH`, effect gravity and
footstep distances all read it. The old 11/72 was an eyeballed "72″ soldier ≈ 11 units" that
under-measured the stock body — at 11/72 the converted body's 60″ eye sat 0.96 units below the
camera, the exact defect R7 would have exposed. Ground truth: BO2's player is 72″ (converter
bounds read), ZH's body convention is 6″/unit (CONTRACT; INIZH.big's cylinders), the engine
maps W3D units 1:1 to world units (`Drawable::getScale`). Conflicts: `viewmodel.rs` (kept R6's
`INCH = zhbo_native::ART_SCALE`, now the alias of the single source; the branch's stale
SWAY/BOB constants dropped) and MODLOG.md (both kept). Archive VERSION 49 → 50 (the W3D
conversion bakes the scale; native blobs store inches and need no reconversion).

Verified: cargo test --workspace green; real-install suite green; clippy clean; `cargo xtask
build` clean; autotest 13/13 default, 13/13 native+compare, 14/14 `--rts`. Compare log: the
same lone mid-throw Motion transient, 0.904 (was 0.829 — 9% larger with the corrected scale,
as expected); all other samples ≤ 0.013. First-person art is 8.3% larger by design.

Watch item (was "magenta blob at the screen edge"): it recurs deterministically in the
pistol-draw frames (a magenta patch with a white dot plus yellow/black hazard-striped patches
at the frame corners), on both the R2-era and current native runs. Looks like specific
viewhands sub-surface art (a wrist device?) rather than a transient; audit the viewhands'
materials in a future polish round. Not a correctness failure.
## v1.0 Track C leftovers: the content grids walk in two dimensions (2026-10-21)

The pure editor's `step_picking` walked the option list as a single line: Up/Down visited the next
and previous flat option and Left/Right were not read at all, so BO2's 3-wide perk/wildcard grid
(and the 6-wide attachment grid) could only be walked with Up/Down. The carousel was the only
two-dimensional picker.

It now uses BO2's own `ContentGrid` links (`patch_mp/ui/t6/contentgrid.lua:91-122`, through
`SetNavigation` at `:137-144`), which the CAC screens really do use:
`cacattachmentsmenu.lua:178-189` builds `CoD.ContentGrid.new(CoD.CACAttachments.GridColumns, …)`
with `GridColumns = 6` (`:9`), `cacperksp.lua:194-199` the 3-wide perk grid (`:7`), and
`cacbonuscards.lua:185-190` the 3-wide wildcard grid (`:7`). The links are:

- **Left/Right wrap inside the row.** A new cell links to the cell before it (same row) and its
  `right` to the row's *first* cell; the first cell's `left` is back-linked to the row's last. So
  Left off a row's left edge lands on its rightmost cell, and Right off the right edge on its
  leftmost. A one-cell row has neither link (BO2's `SetNavigation` skips a target that is the
  button itself), which is a no-op move here.
- **Up/Down wrap inside the column**, not the flat list: each row links to the row above/below in
  the same column and back, and the first row's `up` is re-linked by the last row that holds the
  column. A short last row therefore has no cell in the columns it does not reach, and those
  columns wrap between the first and last rows that do have them.
- **There is no page step on Left/Right.** The "row-edge step the page" in the task's premise is
  not in this Lua: `ContentGrid` is one un-paged grid, and the only paging in the dump is
  `ScrollableContentGrid` (`scrollablecontentgrid.lua:559-745`, Up/Down page by a gridful), which
  none of the CAC screens build. The port wraps, as the Lua does.

The column count is now a property of the pure slot model (`Slot::option_columns`: 6 for the
attachment rows, 3 for every other content grid, `None` for the carousel and the non-list rows),
and `classui::grid_columns` (the drawing window) delegates to it, so the picture and the arrows
cannot disagree. The weapon carousel keeps its category walk: `classui::editor_frame` consumes its
four arrows before the pure editor sees them, and `step_picking`'s linear branch remains only for
that path.

Tests: `cargo test -p zhbo-core` green (39 classui + 37 classmenu among them), clippy clean in the
touched files. New: the link table itself on a 7-cell/3-wide grid, a full 6-wide attachment grid,
a one-row grid and a short last row; and the shared input fold walking a perk grid with all four
arrows, asserting `cac_grid_nav` on the move and that Left/Right wrap the row while Down/Up move a
row.

## v1.0 Track C leftovers: the menus fade in on the Lua's 250 ms (2026-10-21)

The class screens had the focused row's 650 ms pulse and the class-set switch's 250 ms
slide/fade, but a menu that opened simply appeared. BO2's menu-open animation is
`CoD.Menu.animateIn` (`patch_mp/ui/t6/codmenu.lua:2300-2326`): `setAlpha(0)`, `setZoom(600)`,
`beginAnimation("animate_popup_in", 250, false, true)`, then `setAlpha(1)`, `setZoom(0)` — 250 ms,
no ease-in, an ease-out, from alpha 0 to 1. `openPopup` runs it for every popup it creates
(`codmenu.lua:1985-1990`), `openPopupOnDrc` at `:2040-2042`, and the lobby opens
`CACChooseClass` through `openPopup` (`partylobby.lua:429`), so that is the choose-class screen's
entrance. G8 (`ghidra/answers/G8.md`) gives the curve — the engine's cosine ease on the flagged
half — and the existing `lui::anim::ease` is that curve; `bo2::fade` is the `alphaMultiplier` over
one prim group.

Read exactly, **`openMenu` does not call `animateIn`**: BO2's `CoD.Menu.openMenu`
(`codmenu.lua:1937-1963`) creates the menu and adds it with `addElementAfter`, a cut; the CAC
editors and pickers are opened that way (`caceditclass.lua:276`, `:309`, `:342`, `:370`), so their
entrance is a cut in the Lua. The port collapses BO2's menu stack into one screen, so it runs the
same menu-open animation on its in-place screen swaps (the editor and its picker) as well; that is
the port's choice, not an invented duration. The **close** direction is the Lua's
`animateOutAndGoBack` (`codmenu.lua:2254-2295`, 150 ms, out only, reached through `goBack` at
`:279-299` when the menu occludes another) and is **not** ported, so backing out of a picker is
still a cut. The menu-level zoom 600 → 0 that travels with the fade is not drawn either (the prim
path has no element zoom); the alpha is the motion the captures show.

Implementation: `ScreenView` carries `open: Option<f32>` (seconds since the shown menu opened).
`classui::draw_screen` draws the static body into one group and fades it with `animateIn`'s
ease-out alpha; with `open` `None` (or once the duration has run) it draws the body directly, so a
settled screen is byte-identical to the old static draw. The hosts own the clock: `MenuState`
starts `menu_open` at `new()` (the screen's entrance), on `open_editor`, on a picker opening, and
when the editor closes back to the list, and clears it after 250 ms of its 60 Hz steps. The
in-match `Client` starts `class_pick_since` on a false → true `class_pick_open` edge (the spawn
offer or the command bar's force) and drops it from the view after 250 ms. The `S`/`O` dialogs are
popups too: `MenuState::open_dialog` starts `dialog_open`, and `draw_dialog` takes the same age
(`None` from the screenshot harness, so the captures stay settled).

Tests: `cargo test -p zhbo-core -p zhbo-assets` green (322 + 40), clippy clean in every touched
file. New: the draw starts at alpha 0, is 127 at the ease-out midpoint, lands at the static screen
(and a full duration draws exactly that static list), for the pick list, the editor and a picker;
`MenuState` starts the fade on open, the editor, the picker and the editor's close, but not on a
picker close; the `S` dialog starts and lands its own fade; and the in-match panel starts one per
open and starts over after Escape closes it.

Remaining Track C motion: the menu's 150 ms close fade and the eased highlight moves between rows
(only the focused row's pulse is in). Remaining Track C scope otherwise: the pixel pass against
`cac/sweep/` and the in-match class panel per `hud/class.lua` (the panel's entrance is now in; the
class change applying on the next spawn is still the integrator's design call).

## Integrating Track C leftovers: 2-D grid navigation and the menu-open fade (2026-10-21)

Merged `v1/track-c-leftovers` (2 commits). The content grids walk in two dimensions, wrapping
inside the row and column exactly as `contentgrid.lua:91-144` links the cells — **there is no
row-edge page step in BO2's `ContentGrid`** (the paged `ScrollableContentGrid` is unused by any
CAC screen); the task premise's page step was wrong. The menus fade in on `codmenu.lua`'s
`animateIn` (250 ms, ease-out only, G8's curve); BO2's `openMenu` itself cuts, so the fade on
in-place editor/picker swaps is a documented port choice. Not ported: the 150 ms close fade and
the menu-level zoom (no element zoom in the prim path). MODLOG.md was the only conflict; both
kept.

**Design decision (user, 2026-10-21): mid-match class picks stay immediate.** BO2 applies a
mid-match class change on next spawn, but the command-bar pick has no BO2 equivalent and keeps
applying the kit at once to the overhead General — a deliberate mod affordance, not a parity
break. The `PlayerSim::pending_class` sketch in the Track C section is retired.

Verified: cargo test --workspace green; real-install suite green; clippy clean; `cargo xtask
build` clean; autotest 13/13 default, `--classpick` all checks, 14/14 `--rts`.

## Track R8: the weapon's own effects on its tags (2026-10-21, branch `v1/track-r8`)

Muzzle flash and shell ejection now come from the weapon's own `FxEffectDef`s, not a single
generic flash. The converter reads `viewFlashEffect`, `viewShellEjectEffect` and
`viewLastShotEjectEffect` from the BO2 `WeaponDef` (the three fields were there in the layouts but
unused), turns each into a Zero Hour FXList with `fx.rs`'s existing `FxEffectDef`-to-particle-
system conversion, and the weapon table carries the names (`ViewFlashFx`, `ViewShellFx`,
`ViewLastShotShellFx`). `names::bo2_fx` builds the FXList names (the registry), so no literal is
copied between the converter, the table and the bridge. `zhbo_viewmodel_fx(vm, ...)` (a new free
function on the append-only ABI
surface) hands the bridge the names for the weapon on the current viewmodel; `ZhboViewmodel.cpp`
plays them at `tag_flash` and `tag_brass` when `ClientOut.viewmodel_fired` says the sim's `Shot`
event fired. The generic `ZHBO_ViewmodelMuzzle` stays as the fallback.

**Design choice: the engine-FX path, not the native pass.** The engine already owns particle
systems, the material's blend mode, lifetimes and the world FX weapon pipeline; a native particle
system in `zhbo-gfx` would duplicate all of it and need its own headless renderer suite. The
native viewmodel path only supplies the tag transforms (`zhbo_native_bone`), so both viewmodel
paths share the effect art and the table. Shell model elements (`weapon/shellejects/fx_rifle`, the
`fx_rifle_shell` model — `build/list_patch_mp.txt`) become small sprites from the model's own
colour map: Zero Hour's `FXList` has no model nugget. **That is the one approximation** — exact
3D brass needs a native mesh draw.

Tracers/impacts (R8d) are not in this commit. The first-person player's `fire_fx` is deliberately
skipped in `cbFireFx` (the hidden body sits at the camera); per-weapon first-person impacts need a
decision on how the sim's shot end (surface-aware) reaches the bridge — the `rts_fx.txt` table is
the existing shape, but the first-person path has no ABI carrier for the hit point yet. Left for
the integrator's merge decision.

Tests: `zhbo-assets` `a_model_element_becomes_a_shell_sprite` (model → sprite conversion);
`zhbo-core` `viewmodel_effect_names_round_trip` (table schema) and
`viewmodel_fx_leaves_the_buffers_alone_when_the_viewmodel_is_unknown` (FFI). `cargo test -p
zhbo-core -p zhbo-assets -p zhbo-formats -p zhbo-gfx -p zhbo-native` green (with `ZHBO_BO2_DIR`/
`ZHBO_ZH_DIR` unset); clippy has no new warnings (the pre-existing `chunks_exact` one in `fx.rs`
remains). The ignored real-install converter test has not run here (no BO2 install on this
machine).

**Integrator:** the new archive art (effects, particle systems, textures for every weapon) needs
the `hud::VERSION` bump; `zhbo_viewmodel_fx` needs the `ZHBO_ABI_VERSION` bump. `ZhboViewmodel.cpp`
is the only C++ touched (it cannot be compile-checked here). In-game eyeball: every weapon's flash
and shell at `tag_flash`/`tag_brass` on the native path (`ZHBO_NATIVE_VIEWMODEL=1`) and the W3D
path, and that the fallback flash still plays for a weapon whose effect was not converted.

## Integrating Track R8: per-weapon muzzle flash and shell eject on the engine's FXList path (2026-10-21)

Merged `v1/track-r8` (7 commits): the three previously-unread `WeaponDef` fields
(`viewFlashEffect`, `viewShellEjectEffect`, `viewLastShotEjectEffect`) parsed and converted
through the existing `FxEffectDef` → Zero Hour particle-system path; the weapon table carries
them (`ViewFlashFx`/`ViewShellFx`/`ViewLastShotShellFx`); the bridge's new `viewmodelFx` plays
the weapon's own flash at `tag_flash` (and brass at `tag_brass`) from the sim's `Shot` event on
both the native and W3D viewmodel paths — the generic `ZHBO_ViewmodelMuzzle` is the fallback
when an effect has no drawable child. Design: effects draw through the engine's `FXList` path,
NOT a new native particle system (the engine already owns particles, blending, lifetimes); only
the tag transforms come from the native path. Deliberate divergence: BO2's shell is a model and
ZH's FXList has no model nugget, so the shell converts to a small sprite textured from the
model's own colour map (documented in CONTRACT). `names::bo2_fx` is the single FXList-name
builder.

Integration caught one bad assertion: the agent asserted the AN-94 has a
`viewShellEjectEffect`; the real data says NO (probe: flash=yes, shell=no, last-shot=no; only
41 of 172 weapons define a shell effect). The AN-94's brass rides its xanim in BO2 — our
generic shell fallback is correct for it. A notetrack-driven `shell_eject` event (Track R3's
notetrack events) would match BO2 exactly; recorded as a follow-up. **[Retracted 2026-10-24, cycle
`notetrack-brass`: the AN-94's brass rides nothing. BO2 has no eject notetrack; brass is
`viewShellEjectEffect` + `EV_EJECT_BRASS`, and this def has no shell effect, so BO2 shows none —
the generic fallback was wrong for all 131 defs without one. See the entry at the end.]** ABI 23 → 24 (the
`zhbo_viewmodel_fx` free function); archive VERSION 50 → 51 (the per-weapon FXLists and shell
textures).

Verified: cargo test --workspace green; the FULL real-install suite green (the probe-corrected
assertions included); clippy clean; `cargo xtask build` clean (first compile of the bridge
changes); autotest 13/13 default, 13/13 native+compare, 14/14 `--rts`; zero missing-FXList
warnings in the game log. The per-weapon flash's exact look (and the shell sprite's scale) is
a human-playtest eyeball item. R8d (first-person tracers/impacts) is deliberately out: the
hit-point/surface carrier across the ABI is an integrator decision, parked.
## Track R7, milestone 1: the body and world weapons as native blobs (2026-10-21, branch `v1/track-r7`)

The third-person half of the native path starts with the data. `zhbo-native::thirdperson` adds two
manifests, both under `Art\Native\` in the mod's BIG (nothing shipped):

- **`ZB<faction>.zbc`** (body): each faction's BO2 MP body (`c_usa_mp_seal6_assault_fb`,
  `c_chn_mp_pla_assault_fb`, `c_mul_mp_pmc_assault_fb`) as one `.zbm` (skeleton, four-bone skin,
  surfaces, materials — the same `native::model` the viewmodels use), the body bone a weapon hangs
  from (`tag_weapon_right`), and one `.zba` clip per movement state
  (`thirdperson::state::{STAND_IDLE, STAND_WALK, STAND_RUN, STAND_SPRINT, STAND_ADS, CROUCH_*,
  PRONE_*, *_PISTOL}`). The states and the run-time fallback chains (`state::fallbacks`) live in
  the manifest, so a body missing one stance still draws the nearest one.
- **`ZW<vm>.zbw`** (world weapon): the weapon's `world_model` first, each converted combination's
  world attachment on its `worldModelTag` (or the model's origin when BO2 names no tag) at BO2's
  `world_offset`/`world_rotation`, plus the combination's `hideTags`. **The offsets stay in BO2
  inches** (the native blobs' unit); the W3D path scales them, and scaling the native ones would
  apply `SCALE` twice at draw. The manifest is per converted weapon index, so a weapon switch
  reloads only the weapon — the body never names one.

**Gotcha — the body clip names are not all proven.** The W3D conversion only ever used
`pb_stand_alert`, `pb_stand_alert_pistol`, `pb_stand_ads`, `pb_stand_ads_pistol` and
`pb_combatrun_forward_loop`; the walk, crouch, prone and sprint clips were never converted and
their exact spelling is not in the repo or the reference dump. `zhbo-assets::thirdperson` resolves
each state by a candidate list (the known names first) and then by scanning `common_mp`'s XAnim
assets for a name with the state's words and none of another stance's (backward/left/right, pistol,
ads, transitions). The scan is unit-tested with representative spellings; which names a real
install actually has is what the real-install test's strict "every state has a clip" assertion
will report (it names the state and the model in its panic). A state that genuinely does not exist
in MP falls back at run time.

Tests: `zhbo-native` 20 (the two manifests round-trip, every prefix refused, the fallback chains);
`zhbo-assets` 43 + 8 ignored, including the resolver's word scan; the real-install
`converts_from_real_install` now also runs `native_bodies_are_whole`: every faction body reads
back as a 66–78-inch body with a `tag_weapon_right` bone, every state's clip reads back, all body
and weapon textures are in the archive, and the body composes with **every** world weapon (and
each world weapon composes alone) with no missing tag. **Not run here — this machine has no BO2
install; the integrator's run is the first real check of the clip names.** Archive `VERSION`
(`zhbo_assets::hud`) needs a bump for the new blobs, deliberately left to the integrator.

## Track R7 milestone 3: the General's body native (Rust side)

The Rust side of the body mode is in `nativebody`: `load` reads the faction's `ZB<f>.zbc`
manifest, its body `.zbm`, and its `.zba` clips, composes the skeleton with the body first (no tag)
and the weapon's base part on the body's `weapon_tag`, then builds one mesh from all of it. The
mode is part of the cache key (`(faction, vm, mode)`), so switching from weapon-only to body mode
reloads the rig. `Loaded::clip_for` picks the movement state's clip, falling back through
`state::fallbacks` when the body has no clip for that stance/gait. `ffi::frame` now allows
`Mode::Body` through, reads the clip and hands it to `pose`, which samples the clip at the frame's
time and rate. The weapon-only mode still draws the rigid gun on the engine's `tag_weapon_right`.

**Deliberately skipped (M3 scope):** the full BO2 transition graph (start/stop/turn clips,
left/right strafes), the per-state blend times, and the reload/throw/melee clips. The body snaps
between clips rather than blending; a weapon reload/throw plays no body clip. The five W3D-proven
clip spellings plus the word-scan fallback cover idle/walk/run/sprint/ADS/crouch/prone/pistol.

**Bridge hook not compile-checked (integrator):** in body mode (`ZHBO_NATIVE_BODY=2`) the C++
bridge must hide the whole W3D General body when `zhbo_native_body` returns `ZHBO_NATIVE_DRAWN`,
not only the W3D weapon sub-objects. The bridge already reads `zhbo_native_body_enabled` and calls
`zhbo_native_body` per General in `applyWorldPoses`; the change is to set the drawable hidden
(`draw->setHidden(true)` or the equivalent model-condition state) while the native body drew, and
unhide when it stops succeeding. The Rust side places the native body at the General object's
`pos`/`yaw` (the sim's `NativePose`), so the W3D object's own transform is not needed.

Tests: `zhbo-core` 336 headless (6 nativebody, including the clip-by-state test and the tag-scaled-
once test) and 337 with the renderer feature; `zhbo-native` 20; `zhbo-assets` 43 + 8 ignored;
`zhbo-formats` 21. The in-game body check (does the native body sit on the General, do the clips
look right) is the integrator's.

## Integrating Track R7: the native third person — and the 15-char W3D name trap (2026-10-21)

Merged `v1/track-r7` (3 milestone commits). M1: the faction MP bodies (`c_usa_mp_*_fb`) and the
weapons' `world_model`s convert as native blobs (`ZB<f>.zbc` manifests, `.zbm`, `.zba` clips).
M2: `zhbo_client_native_poses` + `zhbo_native_body` draw the General's weapon natively in the
world on the engine's `tag_weapon_right` while the W3D body stays (`ZHBO_NATIVE_BODY=1`); any
refusal brings the W3D weapon back. M3: the Rust-side body mode composes the faction body and
the weapon into one rig with a minimal movement-state clip pick (idle/walk/run/sprint/ADS/
crouch/prone — deliberately no transition graph, blends, or reload/throw/melee clips); **the
C++ still drives weapon mode only** (`ZHBO_BODY_WEAPON`); the body mode's bridge hook (mode 2 +
hiding the whole W3D body) is the follow-up. The auto-merge took both ABI surfaces cleanly
(R8's + R7's, same 23 → 24 edit); the merged ABI is 24 + 1 = **25** (`ZhboNativePose` 28,
`ZhboNativeBodyIn` 204, size asserts both sides). Archive VERSION 51 → 52 (the new blobs).

**The 15-char W3D name trap (the integration's real bug).** The feature engaged nothing in game
— silently. Probe bisect (env ✓, poses ✓ 1 when overhead) pinned it to
`getCurrentWorldspaceClientBonePositions("tag_weapon_right")` returning false: **W3D names are
15 chars (`W3D_NAME_LEN` 16 with the NUL), so the converter writes the pivot as
`tag_weapon_righ` and the 16-char query never matches.** The engine's own sub-object attach is
index-based (or truncates identically), so nothing else ever noticed. The lookup now asks for
the truncated name (commented in ZhboRtsWeapons.cpp). After it: `ZW19U composed` (the TAR-21's
world model) in the `--rts` leg, the showcase General's native gun pixel-identical to the W3D
one's placement, and the W3D sub-objects hiding cleanly while the native draw succeeds.

Verified: cargo test --workspace green (incl. the 6 headless nativebody tests); full
real-install suite green; clippy clean; `cargo xtask build` clean (first compile of the +146
bridge hook); autotest 13/13 default, 13/13 native+compare, 14/14 `--rts`, 14/14 `--rts` with
`ZHBO_NATIVE_BODY=1`, `--showcase` with `ZHBO_NATIVE_BODY=1`.

## Body mode 2: the first-person hide is not reusable overhead (2026-10-21)

Tried to finish R7's body mode by reusing the first-person hidden body. The mechanism is
`setHiddenObject` (`host/zhbo/ZhboInput.cpp:90`), driven from `applyClientOutput`
(`host/zhbo/ZhboBridge.cpp:691`), which calls `Drawable::setDrawableHidden`
(`host/GeneralsGameCode/GeneralsMD/Code/GameEngine/Source/GameClient/Drawable.cpp:4621`). It is
per-drawable, but hiding the overhead General is not the same as hiding the first-person body:
`Drawable::updateHiddenStatus` (`Drawable.cpp:4603`) deselects the drawable, and the engine refuses
to select an effectively hidden drawable (`SelectionXlat.cpp:141`, `SelectionInfo.cpp:356`). The
mod's command bar and ammo overlay need exactly one selected General (`ZhboControlBar.cpp:149`,
`ZhboRtsWeapons.cpp:292`), so body mode 2 would leave the General uncommandable. No C++ was wired;
WEAPON mode and its W3D restore path are unchanged, and the Rust/ABI side needed no change. M3
needs a hide that leaves the drawable selectable, or must exclude the controlled General; do not
add a new engine hook for it.

## Body mode 2: hiding the W3D body without losing selection (2026-10-21)

The first-person hide could not be reused (above), so body mode 2 hides the W3D body one level
down: through the draw modules, not the Drawable. `Drawable::updateHiddenStatus`
(`Drawable.cpp:4603`) does two things when `m_hidden || m_hiddenByStealth`: it deselects the
General, then calls `ObjectDrawInterface::setHidden(hidden)` on every module (`Drawable.cpp:4609`).
The bridge now does only the second half: `draw->getDrawModules()` (`Drawable.h:370`) →
`ObjectDrawInterface::setHidden` (`Common/DrawModule.h:183`). `W3DModelDraw::setHidden`
(`W3DModelDraw.cpp:1826`) hides the render object, its shadow, its terrain decal and its particles;
it never touches `TheInGameUI` or the Drawable's selection flags. Selection checks
`Drawable::isDrawableEffectivelyHidden()` (`m_hidden || m_hiddenByStealth`, `Drawable.h:336`;
`SelectionXlat.cpp:141`, `SelectionInfo.cpp:356`), which stays false, so the General remains
selectable and the command bar/ammo overlay (`ZhboControlBar.cpp:149`, `ZhboRtsWeapons.cpp:292`)
keep working. The bridge already hides `W<vm><k>` sub-objects by name without side effects, but
that needs the body mesh's names and would not cover every module; the draw-module hide is the
engine's whole-model primitive and covers all of them, so it was preferred.

`applyWorldPoses` (`host/zhbo/ZhboRtsWeapons.cpp`) sends `ZHBO_BODY_FULL` when
`zhbo_native_body_enabled()` is `ZHBO_BODY_FULL`, tracks the Generals whose native draw returned
`ZHBO_NATIVE_DRAWN` in `s_nativeBodyHidden`, re-asserts the draw-module hide every frame (other
systems may toggle module visibility), and restores any tracked body the same frame the native pass
stops returning `DRAWN` — a refusal, a still-loading model, a switch back to weapon mode, or a
reset. The restore goes to `draw->isDrawableEffectivelyHidden()`, not unconditional `FALSE`, so a
first-person or stealth hide is preserved. WEAPON mode (`ZHBO_NATIVE_BODY=1`) takes the unchanged
`ZHBO_BODY_WEAPON` path: `nativeBodyDrawn` stays empty, no draw module is touched, and the W3D
weapon restore is byte-identical.

No engine patch and no ABI/Rust change: the core already accepts mode 2, and
`zhbo_native_body_enabled` reads `ZHBO_NATIVE_BODY`. The C++ was not compiled on this branch (the
integrator builds). The W3D shadow is disabled while the native body draws (`setHidden` hides the
shadow too); that is an in-game visual item, not a selection issue.

## Integrating Track R7 body mode 2: the selection-safe W3D body hide (2026-10-21)

Merged `v1/r7-body-hide` (1 commit). The hide is the engine's own whole-model primitive minus
its deselect: `Drawable::updateHiddenStatus` (Drawable.cpp:4603-4616) is two halves —
deselect, then per-draw-module `ObjectDrawInterface::setHidden` — and the bridge calls only the
second half, so `Drawable::isDrawableEffectivelyHidden()` stays false and
`CanSelectDrawable`/`SelectionInfo` never refuse the General. No engine patch. Body mode:
`ZHBO_NATIVE_BODY=2` sends `ZHBO_BODY_FULL`; when `zhbo_native_body` answers
`ZHBO_NATIVE_DRAWN` the General's draw modules are hidden (re-asserted every frame), and any
frame the native draw does not claim him restores the modules to
`isDrawableEffectivelyHidden()` — refusal, loading, mode switch, or off all bring the W3D body
back; `ResetRtsWeapons` restores too. Weapon mode (1) is byte-identical.

Verified: cargo test --workspace green; `cargo xtask build` clean (first compile of the seam);
autotest 13/13 default, 14/14 `--rts` (mode 0 regression), 14/14 with `ZHBO_NATIVE_BODY=1`
(mode 1 regression), **14/14 with `ZHBO_NATIVE_BODY=2`** — the script's command-bar order
checks passing with the body hidden IS the selectability proof — and `--showcase` with mode 2:
`ZW04U composed: 105 bones` (the full body rig), the captures showing the native BO2 body
running in its own clip with the weapon on its tag, no duplicate bodies.

Eyeball items for the playtest: `W3DModelDraw::setHidden` also hides the W3D shadow, so mode 2
has no body shadow yet (the native pass draws none); and a possible one-frame W3D/native
overlap when returning from first person (applyWorldPoses runs before applyClientOutput; the
next frame re-hides — watch for the flicker).
## v1.0 Track C, pixel pass: the class screens against `cac/sweep/` (2026-10-21)

The screenshot harness at 3024×1890 (`cargo run -p zhbo-assets -- screenshot --size 3024x1890`)
was compared with the sweep chapters: `cac-browse`/`cac-set-pager` ↔ `a-chooseclass/001`/`002`,
the editor ↔ `b-editor/002`/`028`/`031`, the weapon picker ↔ `c-primary/001`, the attachment grid
↔ `d-attach/002`, the dialogs ↔ `a-chooseclass/021`/`023`. Three fixes landed:

1. **The editor's prompt bar** (`classui/editor.rs`): the captures pair `ESC Back` with
   `TAB Personalize Knife` (`caceditclass.lua:46-63`) and carry only `R Remove` on the right
   (`b-editor/002`/`028`); the mod's invented `ENTER Open` and the picker's `LEFT/RIGHT Category`/
   `UP/DOWN Gun`/`ENTER Choose` are gone (`c-primary/001`, `d-attach/002` show only `ESC Back`).
   The `TAB` key still opens the category pager; the personalize screen itself is the remaining
   Track C personalize scope, so the label matches BO2 while the action is still the pager.
2. **The choose-class rows** (`classui/pick.rs`): the captures colour by kind — preset/default
   `BOIIOrange`, filled custom off-white with a white row rule, empty/locked custom grey
   (`a-chooseclass/001` Custom 1-3 white vs Custom 4/5 grey; `a-chooseclass/002` the empty set's
   focused Custom 1 white, Custom 2-5 grey). The focus is a white `button_over` border pulsing on
   the `CoD9Button.PulseDuration` (650 ms), not orange text.
3. **The shot's empty editor title** (`zhbo-assets/screenshot`): `Custom 1`, as `b-editor/002`
   and `profile::empty_slot_name` already say, not `CUSTOM 1`.

**`GridTop` is 80, not the Lua's 60.** The `d-attach/002` grid's selected-cell frame is at
y≈305 px and the cell border at y≈312 px at 3024×1890, i.e. the element top 80 at the captures'
scale; the Lua's 60 would put it at 252 px, inside the allocation bar (260-286 px). The earlier
wave's capture choice is right; do not "fix" it to 60.

Remaining pixel deltas (not changed: they need a layout/data pass, not a one-line tune):
- **The attachment grid is too large and the description/stat/`Equipped` panel is missing.**
  `d-attach/002`'s six cells span 1809 px (≈689 LUI) and are ≈130 px (≈50 LUI) tall, with
  `Reflex Sight`, `Precision red dot sight.`, the four stat bars and the `Equipped` MP7 below the
  grid; the mod's `grid_cell` uses the full 864-unit width and 94-unit height and leaves the lower
  half blank. `cacattachmentsmenu.lua` has the `GridTop`/cell/panel geometry to port.
- **The profile-level readout** (`43 / 7955` above the allocation `n/10`) is absent: the mod's
  profile has no level/XP, so the harness and the screens can only show the allocation.
- **The seeded secondary `fnp45_mp` displays `Tac-45`** in the harness's kit/preview while
  `b-editor/028`'s Five-seven is that weapon; the display name is the converted weapon table's,
  not `classui`'s.
- **The primary category column lists `LAUNCHERS`** (`cac-editor-weapons`); `c-primary/001`'s
  primary categories stop at `SPECIALS` (launchers are secondary). The category list comes from
  the weapon table/`weapon_categories`, so this is a data/filter pass.
- **The mod's `SAVE CLASS` row** is a deliberate mod affordance (BO2 saves on back-out) and stays.
- The stat bars' grey/white fill and the `cac-editor` seeded kit's `FlakJacket` second perk are
  harness/state choices, not captures to match literally.

## Integrating the Track C pixel pass (2026-10-21)

Merged `v1/cac-pixel` (4 commits): the choose-class rows colour like the sweeps (focused white
with a white border/rule, presets orange, filled customs off-white, empty/locked grey —
`cod9button.lua:414-420` is the highlight, the captures show the white label); the editor and
picker footers read as BO2's (`ESC Back`, `TAB Personalize Knife`, `R Remove`; the invented
`ENTER Open`/`Category`/`Gun`/`Choose` prompts are gone); the pristine fixture title is
`Custom 1`. **`GridTop` 80 stands**, proven against `d-attach/002` at 3024×1890 (the cell top
lands at 304.5 px; the Lua's 60 would put it inside the allocation bar). MODLOG.md was the only
conflict; both kept. The remaining deltas are recorded in the branch's MODLOG entry above — the
largest: the attachment grid's cell size + missing lower description/Equipped panel, and the
dialog popup chrome; a reference-guided pass follows.

Verified: cargo test --workspace green; the full real-install suite green; clippy clean; `cargo
xtask build` clean; autotest 13/13 default, `--classpick` all checks, 14/14 `--rts`; the nine
harness shots regenerated with every sprite resolving.
## The viewhands' magenta/hazard patches: the wrist datapad's campaign screen (2026-10-21, v1/viewhands-art)

The watch item from the Track R rounds is the first-person wrist **datapad**, not a renderer
placeholder. `zhbo-assets model faction_seals_mp.ff c_usa_mp_seal6_longsleeve_viewhands` shows
surface 2 = `mc/mtl_viewarm_gen_datapad` on all three factions' long-sleeve hands (and the
shortsleeve variants): BO2's `~-gviewarm_datapad_c` 1024x512 image. It is the campaign
map/readout screen — "DEEGAN", "KARACHI", a topo map with a yellow/red marker, a white radar ring
and columns of yellow text. Dumped to PNG, the "yellow/black hazard stripes" are its yellow
readout columns; the "magenta patch with a white dot" is the map marker/radar under the
watchglass. The hands model is the MP one, but BO2's MP watch face is not the campaign screen; the
converter drew the static campaign texture, so the UI leaked onto the arm.

Fix: `textures::textures` maps any `viewarm_gen_datapad` material to a generated neutral screen,
`zhbo_viewarm_gen_datapad_off.tga` (256x256, [16,18,20] RGB), instead of the campaign colour map.
The normal/spec maps and the watchglass stay, so the geometry and glass are unchanged; both the
W3D mesh and the native blob get the neutral name (one shared name). The campaign
`zhbo_viewarm_datapad_c.tga` is no longer written. This is a principled "screen off" rather than
dropping the device: the watch is still there, the campaign UI is not.

Tests: `textures::tests::the_wrist_datapad_screen_is_a_dark_neutral_panel`; the real-install
`generals::tests::converts_from_real_install` now calls `wrist_datapads_are_off`, which asserts
the campaign image is absent, the neutral texture is written, and every faction's hands datapad
material points to it (and never to `viewarm_datapad_c`). Verified: `env -u ZHBO_BO2_DIR -u
ZHBO_ZH_DIR cargo test -p zhbo-assets -p zhbo-formats` green (45 + 21, 8 ignored each);
`ZHBO_BO2_DIR=~/Games/BlackOps2 cargo test -q --release -p zhbo-assets -p zhbo-formats --
--ignored` green (8 + 8); clippy has no warnings in the touched files. Converter output changed,
so the integrator must rebuild `ConquerOps_BO2.big`; **`hud::VERSION` is deliberately not
bumped** here (task rule), so the launcher will keep an older archive until the bump is made with
the rebuild.

## Integrating the viewhands datapad fix — and what the "magenta blob" really is (2026-10-21)

Merged `v1/viewhands-art` (1 commit): BO2's wrist datapad material (`mc/mtl_viewarm_gen_datapad`,
on every faction's viewhands, long- and short-sleeve) carries a **campaign UI readout screen**
(topo map, magenta cursor, yellow text columns) that BO2 never shows in MP; the converter now
substitutes a generated neutral dark panel (`zhbo_viewarm_gen_datapad_off.tga`) and keeps the
watchglass and the datapad's normal/spec. The campaign texture is out of the archive (verified
by `strings`: the neutral texture is in, `mc/mtl_viewarm_gen_datapad` points at it; note BO2
also has `mtl_viewarm_gen_datapad_cl`, `mtl_viewarm_frnd_datapad`, and the character body's
`mtl_c_gen_mp_datapad` family — the fix keys on the exact material the drawn viewhands use,
verified per variant with the `model` CLI). Archive VERSION 52 → 53.

**The "magenta blob at the screen edge" watch item is RESOLVED — it is not stray art.** It is
Track H's damage-direction indicator (`hit_direction`), drawn exactly when the General is under
fire (the same frames print "Unit under attack"; frames without combat never show it). Its
shape/colour fidelity against BO2's arc is a Track-H polish question, not a defect. The
yellow/black "hazard stripes" are the viewarm sleeve-cuff texture peeking in only in the
high-arms pistol-draw frames — BO2's own sleeve art (a yellow-striped UV island whose edge the
cuff vertices sample); the vm-sweep harness's close-up renders will confirm per faction whether
the cuff reads as BO2 intends.

Verified: cargo test --workspace green; the full real-install suite green (incl. the new
`wrist_datapads_are_off` over all three factions); clippy clean; `cargo xtask build` clean;
autotest 13/13 default, 13/13 native+compare, 14/14 `--rts`.
## A headless viewmodel sweep (2026-10-21, branch `v1/vm-sweep`)

The first-person artifacts a playtest reports (see-through patches, an optic in the wrong place, a
gun off the aim axis) are hard to chase in game: each look is a match launch, a class, a gun. This
round adds a review harness that draws the whole catalog without the game.

`cargo run -p zhbo-core --example render_viewmodels --features renderer -- <outdir>` (with
`--release`: the on-demand conversion dominates) walks every player weapon of the installed table
and, for each, the bare weapon and the weapon with each of its `Allowed` attachments mounted solo.
One PNG per (weapon, attachment): hip on the left, ADS on the right, 640x360 each, on a flat 0.72
grey. `manifest.tsv` carries the result of every job (where the art came from, the part count,
whether the attachment added a view-model part, the viewmodel's log lines).

The harness is the game's own path, not a lookalike: `zhbo_native_viewmodel` with the
`zhbo_native_wanted`/`zhbo_native_provide` protocol (what `ZhboViewmodel.cpp` drives), the core's
composition, posing and `nativevm::place` (`ads = 0` hip, `ads = 1` aimed), and `nativevm::gpu`'s
submit into a headless `zhbo-gfx` renderer — the real native BO2 pass, shading and culling included.

* Base weapons draw the archive's own blobs (what the game draws today). A combination draws from
  the archive only when the table itself holds it (the loadout's kit, e.g. `an94_mp+gl+reflex`);
  every other combination is converted on demand by the new
  `zhbo_assets::viewmodel_sweep::Source`, which runs the same `viewmodels::weapon_viewmodels` the
  game build runs, straight out of the player's install, into memory for that render only. The
  blobs and the images never leave the machine and nothing is written outside the output folder.
* **Gotcha: `client::horizontal_fov` returns radians.** Its doc says degrees and its callers hand
  the value to the engine as a radian horizontal FOV, but a harness that calls `.to_radians()` on
  it again projects with a ~50x too narrow frustum: the first version drew a gun-sized sliver at
  the bottom edge, and at `ads = 0` (the gun exactly on the axis) nothing at all while the aimed
  pose showed a patch of the arms. The harness now uses the game's FOV as it is
  (`1 / tan(hfov / 2)`, vertical from the aspect), which frames the gun like a match does.
* **Gotcha: the clip arrives a round after the models.** The first frame that returns "drawn" can
  still pose the bind pose (the clip is requested while the models decode), so each pose draws one
  frame, discards it and draws the picture from the second. Without that, the hip and aimed images
  of one weapon come from two different poses.
* The pose numbers are the weapon definition's (`weapons::gunpos::GunPose::step` at stand, still:
  no bob, no sway), so BO2 decides where the gun sits; the camera is the game's eye frame with a
  fixed +X view direction, identical for every render.
* The install is only needed for the combinations the archive lacks: a base-weapons-only sweep
  (`--no-attachments`) runs off `ConquerOps_BO2.big` alone.

The first full sweep (475 images: 59 weapons, 416 solo attachments, 0 failed jobs) also produced
the first artifact list. The review findings, in the sweep's `FLAGS.md` beside the images:

* Four equipment viewmodels compose but draw nothing (`hatchet_mp`, `trophy_system_mp`,
  `flash_grenade_mp`, `proximity_grenade_mp`); `emp_grenade_mp` is a sliver at the bottom edge.
  Eight more are blank in hip and appear only in ADS (`frag_grenade`, `sticky_grenade`,
  `satchel_charge`, `tactical_insertion`, `bouncingbetty`, `willy_pete`, `sensor_grenade`,
  `concussion_grenade`). The blobs load (e.g. hatchet: 74 bones, 6921 vertices), so this is the
  clip/visibility or the hip `GunPose` offset, not missing art.
* Ten attachments add a part and vertices in the manifest but render pixel-identical to base:
  `as50_mp` `ir`/`silencer`/`swayreduc`, the pistol `dualclip`s (`fnp45`, `fiveseven`, `kard`,
  `judge`, `beretta93r`), and `kard`/`ksg` `steadyaim`. The added part is hidden or inside the gun.
* Dual wield still draws one pistol: the `*_dw_mp` bases have the same bone/vertex counts as the
  single pistols, the `dw` attachment says "adds no view-model part", and neither `viewmodel.rs`
  nor `ZhboViewmodel.cpp` has a second-instance path.
* The AS50 is the placement outlier: hip fills 0.388 of the frame at mid-screen, where the DSR50
  fills 0.101 at the bottom. `knife_held` ADS fills half the screen.
* The guns are near-black silhouettes. The harness uses the `nativevm/tests.rs` light rig
  (`ambient [0.22, 0.2, 0.17]`, two dim lights), not the scene's time-of-day lights, so this is
  likely the harness, not the converted albedo. There is no magenta/checkerboard/missing-texture
  output; the wrist datapad's neutral grey screen is the only flat surface.

These are review findings, not fixes; the next pass should confirm them in game before changing
the converter or the data.

## Integrating the headless viewmodel sweep (2026-10-21)

Merged `v1/vm-sweep` (1 commit): `cargo run --release -p zhbo-core --example render_viewmodels
--features renderer -- <outdir>` renders every player weapon — base and with each `allowed`
attachment solo — through the game's own pipeline (nativevm compose with the combination's
models/hide_tags/clips, `place` hip vs ADS, the native pass headless) against flat grey,
475 images over 59 weapons with zero failed jobs. MODLOG.md was the only conflict; both kept.

The first artifact pass (FLAGS.md in the run's outdir, not committed): **no magenta anywhere**
(independent confirmation of the datapad fix); real follow-ups, in order of weight —
1. four equipment viewmodels draw nothing in hip (`hatchet`, `trophy_system`, `flash_grenade`,
   `proximity_grenade`; `emp_grenade` a sliver) and eight more are hip-blank/ADS-only —
   their blobs compose, so this is pose/clip placement;
2. ten attachments add manifest geometry but render pixel-identical to base (hidden, inside
   the gun, or zero-scaled — dump `hide_tags` for those combos);
3. dual wield renders one pistol (no second-instance path in the conversion);
4. the AS50 sits far larger/higher than its sibling snipers (check its `view_pos`);
5. everything is near-black under the harness's test light rig — relight one weapon with the
   scene's global lights before blaming albedos.
`place::aligned` only translates the sight, so ADS keeps the hip body's orientation — a
documented placement limitation, not a converter bug.

Verified: cargo test --workspace green (the removed broken probe tests were the branch's own);
full real-install suite green; `cargo clippy --workspace --all-targets` clean of new warnings;
`cargo xtask build` clean; autotest 13/13 default, 13/13 native+compare, 14/14 `--rts`.

## v1.0 Track C, pixel pass 2: the attachment picker's lower half and the popup chrome (2026-10-21)

The first pixel pass left two presentation deltas; this pass closes both and corrects one
measurement in the earlier report.

**Attachment picker (`cac-editor-attachments`).** The grid's pitch is **268 px** at 3024×1890
(94 LUI cell + 8 LUI gap), not the 171 px an earlier note recorded; the 312/483/654 rows were
the cell's inner edges, not three rows. BO2 derives the cell from
`cacattachmentsmenu.lua:12-25`: `GridItemWidth = (Menu.Width - 8*5)/6 = 137.33 LUI` (360.5 px)
and `GridItemHeight = 64 + ContentGridButton.TitleHeight + 10 = 94 LUI` (246.75 px), because
`TitleHeight` is `textSize.ExtraSmall = 20` (`contentgridbutton.lua:5-10`,
`codbase.lua:344-345`). The declared `CACAttachments.GridTop = 60` (`:8`) is never read; the
menu's own title/bar layout would derive 83 (`:116-168`), but `d-attach/002` puts the selected
cell's top border at y≈305, i.e. 80 LUI (`layout.rs`'s `GRID_TOP`; the Lua's 60 would sit inside
the allocation bar). The port now uses the captured 80 and keeps the 94-unit cell.

The screen now draws BO2's lower half (`cacattachmentsmenu.lua:238-536`): the panel starts at
414 LUI (`83 + 3*94 + 2*8 + 3 + HintText.Height 25 + 5`, `:238-247`; title box y≈1181, Big glyph
y≈1205 in both the capture and the shot), the highlighted entry's Big title, its reserved
description line (Default, alpha 0.4), the four `AttributeBarList` bars (18 dashed blocks,
`attributebarlist.lua:266-273`; base white, positive green, negative red, `:16-30`), the 300×150
weapon image lifted 40 above the panel top (`:422-450`), and the `Equipped` slot list: a 168×74
box with two 64-unit bottom notches (`:452-536`, `:707-724`). The attachment grid shows exactly
three rows above that panel (`picker_window`/`grid_rows_visible`), instead of four.

Remaining data gaps for the integrator (not hand-copied): `AttachmentDef` has no description
(`Engine.GetAttachmentDesc`, `cacattachmentsmenu.lua:1029-1038`) and its `display_name` is the
tile's short name, so the lower title reads “Reflex” rather than “Reflex Sight”; the table also
has no BO2 attribute-table deltas, so a Reflex shows no green range/accuracy ticks. Add those
fields to the weapon-table schema/converter and the panel will read them; no new numbers were
invented here.

**S/O popups (`cac-dialog-set`, `cac-dialog-class`).** The dialogs were a 460-wide square with a
bright custom footer and sat too low. They now use `CoD.Menu.NewSmallPopup`'s 448×256 body, 271
for the class dialog (`codmenu.lua:35-36`, `cacclassoptions.lua:15-18`), centred; the Big title at
0, the set/class name in Default at 48/alpha 0.5 and the `CoD9Button` list at 93
(`cacclasssetoptions.lua:48-137`, `cacclassoptions.lua:79-137`). `addSmallPopupBackground_Multiplayer`
(`codmenu.lua:1377-1622`) supplies the shape: a 498-wide dotted image bled around the 448 content,
a light bevel and a shallow bottom pointer. The popup-specific `menu_mp_popup_top/stretch/bottom`
materials are not in the converted archive, so the existing `menu_mp_cac_backing` supplies the
texture and lines the chamfer/pointer; adding the popup materials to `lui::materials::MATERIALS`
and rebuilding the archive is the next fidelity step. The prompt is the normal `ESC Back` footer
(`codmenu.lua:185-226`) instead of the extra bar, and the panel fill is opaque #262626 (the
capture's interior ~36/255 over the dimmed screen). Measured at 3024×1890: panel width 1262 px vs
the capture's 1264; title glyph top 632 vs 633; RENAME 868 vs 870.

Verified: `env -u ZHBO_BO2_DIR -u ZHBO_ZH_DIR cargo test -p zhbo-core -p zhbo-assets` green (340
core + 45 assets at the attachment commit; 341 core after the dialog test landed); clippy has no
warnings in the touched files; nine harness shots re-render with every sprite resolved
(`cargo run -p zhbo-assets -- screenshot --size 3024x1890`). Commits: `1926b54` (attachment
picker), `cc94cff` (popup chrome). Not this pass's scope (still open): profile-level readout,
`Tac-45` display name, the LAUNCHERS category, set COPY, and the attachment order/extra entries.

## Integrating the CaC pixel pass 2: the attachment picker's lower half and the popup chrome (2026-10-21)

Merged `v1/cac-pixel` round 2 (3 commits). The attachment picker gains BO2's whole lower panel
(`cacattachmentsmenu.lua:238-536`: the Big attachment title, the reserved description line, the
four stat bars as BO2's 18 dashed blocks with white base and green/red deltas per
`attributebarlist.lua:16-30,266-273`, the large weapon image, and the notched `Equipped` box) —
the Lua's `GridTop = 60` is never read and the capture's pitch is 268 px, both settled with
measurements (`d-attach/002`). The S/O dialogs now use BO2's `NewSmallPopup` chrome
(`codmenu.lua:35-36,1377-1622`: the bevelled panel with the shallow bottom pointer, opaque
`#262626`, the list at 93 LUI, the normal `ESC Back` footer) — panel width 1262 px vs the
capture's 1264. MODLOG.md auto-merged; the only integration work was the ladder.

Recorded data gaps for the follow-up (all single-data-source, none hand-copied): attachment
description/full-name fields (the table has only the short tile name) and the BO2 attribute
deltas for the stat bars (needs the attribute table through the converter); the popup's own
materials (`menu_mp_popup_top/stretch/bottom`) are not yet in the archive.

Gotcha, second sighting of the class: the default leg failed with "the General never entered
first person" because an ORPHANED launcher-spawned `generalszh` squatted the single-instance
lock and took seconds to die even after SIGTERM. The back-to-back guard drains normal exits,
not orphans: before re-running a failed leg, check `pgrep -f generalszh` is clear (and give a
TERM'd process a few seconds).

Verified: cargo test --workspace green; full real-install suite green; clippy clean; `cargo
xtask build` clean; autotest 13/13 default, `--classpick` all checks, 14/14 `--rts`; the nine
harness shots regenerate with every sprite resolving.

## Attachment metadata: full names, descriptions, and attribute-table deltas (2026-10-22)

The prior pass reserved the attachment panel's description line and computed its stat bars from the
weapon's own fields because `AttachmentDef` had only the short tile name and no attribute deltas.
The schema now carries BO2's own metadata:

- `AttachmentDef.full_name` and `AttachmentDef.description`, filled from
  `mp/attachmenttable.csv` in `patch_mp` (row 22 for the Reflex: display key `MPUI_REFLEX` in column
  4, short name `reflex` in column 5, description key `MPUI_REFLEX_DESC` in column 8; the localized
  text comes from the same `names` map the converter already uses).
- `AttachmentDef.attribute_deltas[5]`, filled from `mp/attributestable.csv` in `patch_ui_mp`. The
  Lua's `WeaponAttributeList.SetupWeaponAttributeBars` uses 0-based column constants 3, 4, 2, 6,
  5 (`patch_mp/ui/t6/weaponattributelist.lua:99-142`), i.e. damage = col 3, range = col 4,
  accuracy = col 2, mobility = col 6, fire rate = col 5. The converter writes them to the table as
  `FullName`, `Description`, and `AttributeDeltas`.

The converter's `generals::files` now loads `patch_ui_mp` and calls `weapons::table_with_metadata`;
`weapons::table` remains as a wrapper with an empty map so the tests and viewmodel sweep compile
unchanged. The attachment table's row 22 gives the Reflex's description key; the attribute table's
row for the Reflex (`weapon_attachment,reflex,15,0,5,0,0,0,,xm8`) yields deltas [damage 0, range 5,
accuracy 15, mobility 0, fire rate 0], matching the capture's green range/accuracy ticks.

Verified: `env -u ZHBO_BO2_DIR -u ZHBO_ZH_DIR cargo test --workspace` green (341 core tests, 45
assets tests); the new fields survive the schema's format+parse round trip in the unit tests. The
install-layer conversion (and its `generals::files` debug fixed-point assertion) still needs the
real BO2 install; the converter now requires `patch_ui_mp`, so note that in the install-layer run.

## Attachment panel consumption and popup slices (2026-10-22)

The lower attachment panel now consumes the metadata added in the prior data pass. The Big title
reads `AttachmentDef.full_name` ("Reflex Sight" rather than the tile's short "Reflex"), the
reserved Default line at alpha 0.4 reads `AttachmentDef.description`, and the four
`AttributeBarList` bars apply `AttachmentDef.attribute_deltas` to the base weapon's
`weapon_stats`. The deltas are BO2's attribute-table columns in the order the Lua reads them
(damage 0, range 1, accuracy 2, mobility 3, fire rate 4; the panel has four bars and leaves
mobility unused). `stat_bars_delta` keeps BO2's white base, green positive ticks and red negative
ticks (`attributebarlist.lua:16-30,266-273`; `weaponattributelist.lua:99-142`). The lower panel's
geometry and text roles remain those of `cacattachmentsmenu.lua:238-536` (title `:268-306`,
description `:307-353`, bars `:354-372`, `Engine.GetAttachmentDesc` `:1029-1038`), matching
`d-attach/002`.

The built-in stand-in table carries the Reflex's cited row (`mp/attachmenttable.csv` row 22:
`MPUI_REFLEX`, `MPUI_REFLEX_DESC`) and the attribute row
(`mp/attributestable.csv`: `weapon_attachment,reflex,15,0,5,0,0,0,,xm8` → [0,5,15,0,0]), and the
MP7's stand-in gains `Allowed = reflex=reflex` so the screenshot fixture's attachment panel
resolves the Reflex definition instead of falling back to the short name. The hermetic screenshot
test now asserts the "Reflex Sight" title and "Precision red dot sight." description, so a
regression in either field fails the suite.

The S/O popup chrome now draws the real `menu_mp_popup_top`, `menu_mp_popup_stretch` and
`menu_mp_popup_bottom` slices (`codmenu.lua:1400-1493`; `a-chooseclass/021`) in place of the
`menu_mp_cac_backing` stand-in and the hand-drawn chamfer/pointer outline. `lui::materials::MATERIALS`
lists the three names so the converter's material pass can archive them; the slice heights are the
32-unit placeholder the ABI's image draw takes. No `hud::VERSION` bump.

Before/after: the pre-change panel drew "Reflex" with a blank description and no green/red ticks
because `AttachmentDef` had only the short tile name and no deltas. After the change the same
fixture draws "Reflex Sight", "Precision red dot sight." and green ticks on the range and accuracy
rows (the Reflex's +5/+15 columns). The pre-built archive still lacks the three popup slices, so a
regenerated harness shot logs the missing materials and draws the documented fallback; the
integrator's archive rebuild closes that gap. The real-install conversion suite passed
against the local BO2 install (`ZHBO_BO2_DIR=$HOME/Games/BlackOps2`: 9 `zhbo-assets` ignored tests
and 8 `zhbo-formats` ignored tests), including the converter's fixed-point assertion; the in-game
autotest still needs the integration machine.

Verified: `env -u ZHBO_BO2_DIR -u ZHBO_ZH_DIR cargo test -p zhbo-core -p zhbo-assets` green (342
core tests, 45 assets tests, screenshot test included); clippy has no warnings in the touched files;
nine harness shots regenerate (`cargo run -p zhbo-assets -- screenshot --size 3024x1890`), with the
three popup materials logged as missing from the pre-built archive and their fallbacks drawn.

## Integrating the CaC attachment data pass: real names, descriptions, deltas and popup slices (2026-10-21)

Merged `v1/cac-data` (2 commits). The `AttachmentDef` schema gains `full_name`, `description`
and `attribute_deltas[5]` (schema count 14 → 17); the converter fills them from the player's own
tables — `mp/attachmenttable.csv` (display/description keys, localized through the existing
names map) in `patch_mp` and `mp/attributestable.csv` in `patch_ui_mp`, with the Lua's column
constants (`weaponattributelist.lua:99-142`: damage 3, range 4, accuracy 2, mobility 6, rate 5).
The attachment panel now draws "Reflex Sight" and "Precision red dot sight." where it drew
"Reflex" and a blank line, and the four stat bars get their green/red deltas (Reflex: range +5,
accuracy +15). The popup chrome uses the real `menu_mp_popup_top/stretch/bottom` slices where
the captures show them. No conflicts (fast-descendant). Archive VERSION 53 → 54 (the schema and
the three materials). The converter now also requires `patch_ui_mp.ff`.

Verified: cargo test --workspace green; the full real-install suite green (incl. the metadata
assertions); clippy clean; `cargo xtask build` clean; autotest 13/13 default, `--classpick` all
checks, 14/14 `--rts`; the harness shots show the real strings, deltas and slices (the
pre-rebuild fallback was documented in the branch note). The mobility delta is carried but
undrawn (the panel has four bars) — a presentation choice, recorded.
## The MP7/reflex tear: the clip sweep, the light rig, and the hide tags (2026-10-21, branch `v1/vm-fixes`)

The MP7 + Reflex in-game artifact (jagged see-through gaps along the receiver top and the reflex
base) needs the harness to animate the weapon, not hold `IDLE` at frame 0. `render_viewmodels
--sweep` now draws the `FIRST_RAISE` clip across 40 frames (0.05 s each) at the game's own eye
frame and light rig. The rig is not a test rig: the harness's `ambient`/`lights` already match
Zero Hour's shipped `TerrainObjectsLightingAfternoon*` from `INIZH.big` (ambient 56/52/44,
diffuse 255/255/255 at lightPos -0.81/0.38/-0.45; second light 60/60/80 at 0.79/0.62/0), the
values `ZhboViewmodel.cpp::sceneLights` hands the native pass. The eye is the same frame the
bridge uses: origin, +X forward, Z up.

The bisect (frame 39, `mp7/sweep_base` vs `mp7/sweep_att_reflex`):
- **Culling is not the cause.** Forcing every surface's cull off (`ZHBO_SWEEP_NO_CULL`) leaves the
  base-vs-reflex difference unchanged (0 pixels at frame 39; 5 at frame 10).
- **Hide tags are the only difference.** Clearing the hide list (`ZHBO_SWEEP_NO_HIDE`) removes the
  200-pixel gap at `(376,200)-(404,217)` — the iron-sight post the reflex's `hide=["tag_sights"]`
  correctly removes. The receiver top under it stays solid.
- The native hide mechanism itself diverged from the W3D path: `Skeleton::palette` zeroed a
  hidden bone, so blended vertices collapsed toward the model origin. `build_mesh` now omits a
  triangle whose heaviest bone is hidden and drops hidden weights from the kept vertices, exactly
  as `zhbo_assets::character::skin_hiding` does. (This changes no MP7 pixels because its sight
  vertices are single-bone, but it is the correct behavior for blended hidden geometry.)

The remaining branch was the projection. A near-plane sweep (`ZHBO_SWEEP_NEAR`) reproduced the
tear at 2.0 world units: the receiver top and reflex base clipped jaggedly (near 3.0 left only a
fragment), while the harness's default 0.1 is solid. The engine's scene camera uses
`NearZ = MAP_XY_FACTOR = 10` world units
(`Core/GameEngineDevice/Source/W3DDevice/GameClient/W3DView.cpp:106`), and the native pass
inherited that projection; the viewmodel sits at the eye, so its rear was inside the near plane.
`zhbo-gfx`'s `viewmodel_projection` now rebuilds the scene projection with a 0.1-unit near,
keeping the FOV/aspect and the far, so the viewmodel keeps the scene's projection but not its
terrain near. The harness's near 0.1 is unchanged.

Evidence: `.../opencode/vm-sweep/mp7-sweep` (baseline near 0.1), `mp7-nocull`, `mp7-nohide`,
`mp7-near1.0`, `mp7-near2.0`, `mp7-near3.0`, and `near_compare.png` (0.1 / 1.0 / 2.0 / 3.0).

## Integrating the MP7 tear fix: the viewmodel was inside the scene's near plane (2026-10-21)

Merged `v1/vm-fixes` (4 commits). **Root cause of the user's in-game MP7+Reflex tear (jagged
sky-through gaps along the receiver top and the reflex base): the native pass inherited the
scene camera's near plane — Zero Hour's `NearZ = MAP_XY_FACTOR = 10` world units
(`W3DView.cpp:106`) — and the viewmodel sits at the eye, so the gun's rear third was inside the
near plane and clipped into jagged holes.** The pass now rebuilds the projection with a
0.1-unit near (scene FOV/aspect/far kept), renderer-side only. The harness's near-plane sweep
reproduces the report exactly (solid at 0.1/1.0, the tear at 2.0, a fragment at 3.0). The
bisect exonerated the other suspects with committed evidence: forcing cull off changed 0 px at
the affected frames (R4's mapping is innocent); the 200-px gap by the reflex is its
`hide=["tag_sights"]` correctly removing the iron post — and the native hide path now omits
hidden-bone triangles like the W3D path (`build_mesh`); the receiver top is single-bone
(`j_gun`), so no skinning tear (the only blended verts are the sling straps). The harness grew
the fidelity tooling that pinned it: clip sweeps across frames, the game's eye frame, the
`TerrainObjectsLightingAfternoon*` rig, the skin-weight and cull probes. No ABI/VERSION change.
MODLOG.md was the only conflict; both kept.

Verified: cargo test --workspace green; full real-install suite green; clippy
--workspace --all-targets clean; `cargo xtask build` clean; autotest 13/13 default, 13/13
native+compare, `--classpick` with `ZHBO_NATIVE_VIEWMODEL=1` all checks — **the in-game MP7 is
solid in the captures: receiver top, rail and sight housing intact where the user's screenshot
tore** — 14/14 `--rts`.

## The native viewmodel becomes the default — and the macOS signature-cache trap (2026-10-22)

`ZHBO_NATIVE_VIEWMODEL` flips to **on by default** (`0`/`false`/`no`/`off` falls back to the W3D
conversion): after the 10-21 fixes (display-space shading, culling, skin weights, the near
plane, hide tags, R6 placement) the native pass out-verifies the W3D path it was built to
replace. The user's "still tearing" report that night had a simpler explanation than a bad fix:
they play the DEFAULT path, which was W3D — the native fixes never applied to their game. One
Rust config read is the whole gate (`config::get().native_viewmodel`; the C++ delegates to the
core). CONFIG.md and zhbo_core.h updated; a `native_viewmodel_is_on_by_default` config test
pins the semantics.

**Gotcha (build environment):** the evening's "the game won't start at all" cascade was macOS,
not code. After an in-place rewrite of `dist/generalszh` WHILE a previous build from that path
was (or had recently been) running — the user playtesting — the kernel kills new execs of the
path with SIGKILL before `main`, even though `codesign --verify` passes ("valid on disk"). The
launches printed nothing (killed before stdout flushed), which read as a launcher failure, a
lock, and a bad build in turn. `codesign --force --sign -` refreshes the registration
instantly; `cargo xtask build` now re-signs `dist/generalszh` after staging on macOS. Second
member of the single-instance family: when a leg fails with "never entered first person",
check `pgrep -f generalszh` AND, if a build just rewrote the binary, re-sign.

Verified: config tests green; `cargo xtask build` clean; autotest 13/13 default **(now the
native path with no env var)**, 13/13 with `ZHBO_NATIVE_VIEWMODEL=0` (the W3D fallback),
`--classpick` all checks, 14/14 `--rts`.

## The W3D viewmodel's near plane: the fallback gets the native pass's 0.1 (2026-10-22)

The W3D fallback (`ZHBO_NATIVE_VIEWMODEL=0`) drew the gun through the engine's first-person
camera, whose near plane was `0.5f` — set by patch 0001's camera override, itself replacing
Zero Hour's terrain near `NearZ = MAP_XY_FACTOR = 10` (`W3DView.cpp:106`). The gun sits at the
eye, so its rear geometry was inside that near and clipped into jagged holes — the same defect
the native pass fixed by rebuilding its projection with a 0.1-unit near. The bridge now owns
the first-person view's near: `Zhbo_GetCameraOverride` returns `0.1f` (`ZhboBridge.cpp`), and
patch 0001's `W3DView::update` hook uses it in `Set_Clip_Planes` instead of the hardcoded 0.5.
The RTS view is untouched: when first person releases the camera, the engine's own
`NearZ = MAP_XY_FACTOR = 10` is what `updateCameraClipPlanes` applies again. The first-person
terrain's far is kept (`std::max(zhboFar, 2000.0f)`); its far/near ratio widens as the near
shrinks, so 0.1 is the smallest value that clears the clip — the "care" the near choice needs.
0.1 is the value the native pass's viewmodel projection uses (`zhbo-gfx`'s
`runtime/native.rs`); the MP7 near-plane sweep in the 10-21 entry found 0.1/1.0 solid and
2.0/3.0 torn. BO2's `r_znear 4` inches (`ghidra/answers/G4.md`) is ~0.67 ZH units, so 0.1 is as
small as needed, not a cosmetic tightening. No ABI/VERSION change; Rust behavior untouched.

## Integrating the W3D fallback's near plane (2026-10-22)

Merged `v1/w3d-near` (1 commit): the bridge-owned first-person near plane. `Zhbo_GetCameraOverride`
now returns `nearPlane` (0.1 world units — the value the native pass's projection uses, proven by
the MP7 sweep; BO2's `r_znear` 4″ ≈ 0.67 units is the parity anchor), and patch 0001's existing
first-person hook sets the camera's clip planes from it in place of the hardcoded 0.5. The world's
own `NearZ = MAP_XY_FACTOR = 10` (`W3DView.cpp:106`) is untouched and returns when the RTS camera
takes over; the terrain's far is kept. CONTRACT.md's camera row documents the split.

**Integration gotcha: patch SERIES, not patch.** The agent verified its 0001 edit against the
pristine engine but not against the patch series — 0004 anchors on the `Set_Clip_Planes(0.5f, …)`
line the new 0001 replaced, so the build's patch step failed at 0004. Any patch edit must be
checked by applying the whole `host/patches/*.patch` series in order (`git apply` loop) before
handoff; 0004's context now follows the new line.

Verified: full 16-patch series applies cleanly; cargo test --workspace green; real-install suite
green; `cargo xtask build` clean (engine recompiled the patched W3DView.cpp); autotest 13/13
default, **13/13 with `ZHBO_NATIVE_VIEWMODEL=0` (the W3D fallback, exercising the 0.1 near)**,
`--classpick` all checks, 14/14 `--rts`. Eyeball item for the playtest: first-person terrain
z-fighting from the wider near/far ratio (the noted trade-off; the fix if it appears is the
separate viewmodel pass, not a smaller terrain near).
## Personalize M3: reticles, colours, the editor rows — and the camo tab cap (2026-10-22)

The reticle and colour screens now use BO2's own data instead of the captured placeholder lists.
`mp/attachmenttable.csv:57-74` has the base `reticle_0..10` rows (the localized name is "Dot" for
all eleven) and `reticle_color_1..7` = Red, Green, Blue, Purple, Teal, Yellow, Orange; the table
now carries `ReticleOption`/`ReticleColorOption` blocks (`CamoOption`'s shape, round-trip tested)
and the catalog indexes them. The art is the 11 stock images per reflex/holo/acog optic in
`code_post_gfx_mp.tsv:1073-1104` (materials `:2023-2054`: `mc/mtl_t6_attach_optic_<optic>_
reticle_01..11`); `hud::build_archive` converts all 33, and its dynamic lookup now also tries the
`mc/` material prefix. `cacreticles.lua:1150-1240,1390-1470` selects the optic's image by option
index; `weapons::optic_of`/`reticle_art` encode that (`reticle_0` -> `_01`, `reticle_10` ->
`_11`), the reticle grid draws it, and the colour grid draws the seven swatches. The HUD's ADS
optic branch (`hud/center.rs`) now draws the class's choice: the `CustomClass`'s
`reticle`/`reticle_color` are carried onto `PlayerSim` when the class lands, and the crosshair
draws the optic's `reticle_N` art tinted by `reticle_color_N` (falling back to the red dot without
a choice). A fake-host HUD test pins `reticle_3` -> `mtl_t6_attach_optic_reflex_reticle_04` with
Green's tint.

The editor rows follow `caceditclass.lua`: `:137/147` registers `AddCamoButton`/`AddReticleButton`
and the functions are at `:748`/`:825`. `C` on the primary weapon row opens Camo and `V` on the
primary attachment grid's optic row opens Reticles (the footer carries `C Camo`/`V Reticle`; `V`,
not `R`, because `R` is the editor's Remove). `menu.rs` opens
`Personalize::new(kind, personalize_choices_for(kind, Some(primary)), draft)`. Tests cover both
rows, including the attachment grid, and the one-shot `Editor.personalize`.

**Camo tab cap.** The single personalize grid has no tabs or scroll, so `WeaponDef.camo` now holds
only BO2's base Camo tab: `camo_none` + 18 (`attachmenttable.csv:32-50`). `Table.camo` still
carries every row (`:51-129`) for the follow-up. The same split applies to reticles: the base 11
(`:57-67`) are in the table, while the DLC/MTX rows (`:87-119`) are not; the reticle colours are
the seven base rows. **Follow-up:** add the DLC/MTX tabs (or a scrolling grid) to the camo,
reticle and colour screens; the option tables already have the camo rows and the base reticle/
colour rows, so the screens need the tab strip and the per-tab slices.

**Knife stays blocked on art.** `weapons.rs:337-349`'s `is_player_weapon` includes only
`knife_held_mp` and explicitly excludes `knife_mp`; `converted` (`:754-767`) therefore carries only
the standard knife viewmodel. The `common_patch_mp` set has `camo_knife`/`camo_ballistic_knife`,
but no second knife viewmodel is converted, so the knife screen cannot offer a real alternate.
No values were invented.

**Integrator note (camo swap).** The final table shape is `CamoList` (base tab ids, `camo_none`
first), `CamoMaterial` (the weapon's `camo_<weapon>` `WeaponCamo` asset) and the `Camo` blocks
(id/name/thumbnail/sort). The minimal path for the in-game swap is to keep `WeaponCamo`'s
`camoMaterials` base->camo pairs and its `camoSets` images out of the table: the converter already
reads them through `zhbo_formats::weapon::camo_all`, and the existing viewmodel/material
conversion can bake the base patterns (the `CamoList` from `camo_multicam` on; `camo_none` has no
pattern), so the native swap selects the converted set by the chosen `CamoList` index at runtime.
That avoids a new table field; if a later DLC tab changes the order, add the pairs as a
`CamoMaterialPair` table.
The camo schema/materials need one `hud::VERSION` bump. `nativevm`/`zhbo-gfx` stay off-limits.

Verified through M3: `cargo test -p zhbo-core -p zhbo-assets` (362 + 47), the real-install suite
(`ZHBO_BO2_DIR=~/Games/BlackOps2 cargo test -q --release -p zhbo-assets -p zhbo-formats --
--ignored`: 13 + 9), clippy clean in the touched files. The screenshot harness has the new
`cac-reticles`/`cac-reticle-color` shots; the PNGs need the launcher's built
`ConquerOps_BO2.big` (`cargo xtask build` was out of scope), so the harness test is the code
evidence.

## Integrating Personalize: camo, reticles, colours and the knife row (2026-10-22)

Merged `v1/personalize` (6 commits, milestones M1–M3). The class carries `camo`, `reticle`,
`reticle_color` and `knife` through the tolerant profile/spec (`classes.rs`); the editor's pure
machine has the personalize grid edges (`classmenu.rs`, `PersonalizeKind`, the TAB one-shot);
the screens are BO2's (`caccamomenu.lua`/`cacreticles.lua` geometry — 6-column camo, 5-column
reticle grids, `None` first, the `*` chosen marker, `EQUIPPED` label) matched to the
`cac/sweep/i-personalize/` captures. M2 makes the choices REAL: the 48 per-weapon `WeaponCamo`
assets parse (`common_patch_mp`), the weapon table carries the camo options (the 98-row table
with BO2's base tab = `camo_none` + 18 as the weapon's list), the `menu_camo_*` thumbnails
convert, and the screen draws the weapon's own set. M3: the 11 reticles and 7 colours from
`attachmenttable.csv:57-74` are tables with their 33 optic art images converted, the editor's
weapon row opens camo (`C`) and the optic row reticles (`V`; `R` is Remove), and the HUD's ADS
optic branch draws the class's reticle+colour choice. **Knife is blocked-on-art** (only the
standard knife converts; the ballistic-knife viewmodel isn't in the player's weapon set —
evidence in the branch's MODLOG). Archive VERSION 54 → 55 (camo schema + camo/reticle art).
The in-game camo MATERIAL SWAP is the documented integrator follow-up (nativevm selects the
converted `camo_<weapon>` set by the chosen index; the screen/profile carry the choice).
classmenu.rs had one trivial conflict (explicit fields + the default spread — both kept);
MODLOG.md the usual.

Verified: cargo test --workspace green; the full real-install suite green (incl. the camo-table
and AN-94 camo-set assertions); clippy clean; `cargo xtask build` clean; autotest 13/13
default, `--classpick` all checks, 14/14 `--rts`.
## The MP7 magazine's authored offset (2026-10-22, branch `v1/vm-fixes-2`)

**The MP7's base magazine was mounted at the gun's root with an identity offset.** BO2 carries the
base magazine as the weapon's `attachViewModel` part: on the MP7 that is slot 7
(`t6_attach_mag_mp7_view`) with an empty `attachViewModelTag` and an authored
`attachViewModelOffset` of `[-0.473, 0, -3.817]` inches. The converter searched the magazine by
name and mounted it at `j_gun` with no offset, so the magazine's top poked through the receiver's
top rail in the hip pose (most visible at the reflex base in the harness's static pair). The
converter now selects the `attachViewModel` entry whose model contains `attach_mag`, carries its
offset through `Weapon::attach_view_offsets`, and mounts it at that offset from the gun root in
both the W3D rig and the native manifest. The native pose adds the part's attach offset to the
root track's base, because the magazine's `tag_clip` track is relative to the part's attach point,
not the gun's root.

Evidence, MP7 base static pair (`--weapon mp7_mp --no-attachments`):
`.../opencode/vm-sweep/magfix/before_archive/mp7/base.png` (archive blob, old identity mount) vs
`.../opencode/vm-sweep/magfix/after_convert/mp7/base.png` (fresh conversion,
`ZHBO_SWEEP_CONVERT_ALL=1`). The before/after diff is confined to the receiver top at
hip `x415-442, y200-229` and ADS `x952-968, y173-192`; the converted render no longer shows the
magazine's top crossing the rail. The ignored real-install test
`the_mp7_magazine_rides_its_authored_offset` pins the manifest part to `j_gun` and the authored
offset. The converted archive's base weapons still hold the old mount until the integrator
rebuilds the asset archive; no `hud::VERSION` or ABI change.

Verified: `cargo test -p zhbo-core -p zhbo-assets -p zhbo-formats -p zhbo-native` green
(45 passed / 12 ignored, 343, 21 / 8 ignored, 21); clippy on the touched crates clean;
`ZHBO_BO2_DIR=~/Games/BlackOps2 cargo test -q --release -p zhbo-assets -p zhbo-formats --
--ignored` green (12 + 8 passed).

## Blank equipment poses: the hand/tag probe (2026-10-22, branch `v1/vm-fixes-2`)

The sweep's equipment blanks are not missing art or a zero `view_pos`: `hatchet_mp`,
`trophy_system_mp`, `flash_grenade_mp` and `proximity_grenade_mp` compose (74–90 bones, 6.8k–10k
vertices) and their authored view positions are present (e.g. hatchet `stand_move [0,1,-1.5]`;
G4 applies that stance value only while moving, so it is not the static hip offset). The new
ignored probe `probe_equipment_idle_pose` samples the idle clip and shows the mount problem: the
converted part hangs from the hands' `tag_weapon` at the animation's tag position, but the
equipment idle tracks move that tag far away from either wrist. For the hatchet,
`tag_weapon` is `[-80.29, 0.01, -6.74]`, the right wrist `[-13.28, -4.44, -29.46]` and the left
wrist `[-4.32, 11.26, 9.30]`; the frag grenade (which appears in ADS) keeps its tag at the bind
`[15.16, 0, -6.75]`. `handModel` is empty on all of these, so the faction's default two-arm
viewhands are used. The ADS aim-marker alignment brings the eight hip-only cases into view; the
four above stay off-screen because their tag is behind the camera. The fix belongs in the
converter's equipment/hand attachment (and, if BO2 selects a different hand model for the
offhand, in the hand-model lookup), not in the harness pose. Probe only in this commit.

## Integrating the MP7 magazine mount and the equipment-pose probe (2026-10-22)

Merged `v1/vm-fixes-2` (2 commits). **The MP7's magazine clipped through the receiver because
the converter ignored BO2's `attachViewModelOffsets`: BO2 stores the base magazine as the
weapon's `attachViewModel` part with an authored offset, but the converter searched by name and
mounted it at `j_gun` with identity.** The converter now parses the offsets, selects the
`attach_mag` part, and mounts it at the authored offset on both paths (the native part root
uses the attach offset as its root-track base). Before/after sweep: the protruding mag top is
gone; the diff is confined to the receiver-top region. The equipment-blank probe: the
off-hand equipment (hatchet, trophy, flashbangs…) composes fine but hangs from the hands'
`tag_weapon`, which the equipment idle tracks move away from both wrists (hatchet's tag sits at
[-80, 0, -7] — behind the camera) — so the gear is off-screen at idle, not missing art or a bad
`view_pos`; ADS aim-marker alignment is what brings the eight hip-only cases into view. The fix
belongs in the converter's equipment/hand attachment (or BO2's offhand hand-model lookup);
recorded, not hacked. Archive VERSION 55 → 56 (the mag mount in the baked archive). MODLOG.md
was the only conflict; both kept.

Verified: cargo test --workspace green; the full real-install suite green; clippy
--workspace --all-targets clean; `cargo xtask build` clean; autotest 13/13 default,
`--classpick` all checks, 14/14 `--rts`.

## Integrating the off-hand attach fix: the equipment rides the left palm's `tag_knife_attach` (2026-10-22)

The first full research → plan → implement cycle (the `parity-cycle` skill; artifacts in
`~/.opencode/plan/equipment-offhand/`). **BO2's first-person off-hand equipment hangs from
`tag_knife_attach` (parented to `j_wrist_le`, the left wrist)** — not `tag_weapon` (the primary
gun's mount, which the equipment clips park behind the camera), not `tag_weapon1` (a static
torso duplicate), and `tag_weapon_left` does not exist on the viewhands at all (body-only
bones). The converter mounted all 13 there; all 13 are the wrong tag, none BO2-parity blank.
The fix is one `mount_tag` helper (`inventory == OFFHAND || ITEM → tag_knife_attach`) carried
through the W3D `Rig::new` tag, the native manifest's gun part, and the aim-marker fallback;
the raw `attachViewModelTag[0..8]` probe came back empty for every item, so the inventory rule
stands (the plan's fallback branch). The knife stays on `tag_weapon`. Archive VERSION 56 → 57.

Caveat recorded for the follow-up cycle: the sweep harness's hip pose puts the equipment
viewmodel behind the camera for 12 of 13 items in BOTH before and after, so the plan's §5 hip
criterion could not pass as written — the harness's equipment `view_pos` is its own follow-up.
The tag fix is proven independently: the manifest assertions (13 + claymore + pda_hack on
`tag_knife_attach`, `an94_mp` on `tag_weapon`), the extended real-install blob check, and the
ADS renders (items move from the primary-gun position to the left hand). The trophy's door
tracks and the flash's `tag_clip` track remain the predicted sub-bone follow-up, with the
after-renders as evidence.

Verified: cargo test --workspace green (496 across 19 suites); the full real-install suite
green; clippy clean; `cargo xtask build` clean; autotest 13/13 default (**throws both
offhands — the engine-rule arbiter**), `--classpick` all checks, 14/14 `--rts`.

## Integrating the sub-bone parking pin (2026-10-22)

Merged `v1/subbone-pin` (1 commit, test + docs only). The `equipment-subbone-tracks` cycle's
verdict: the trophy's ±550″ door tracks and the m84's ±200″ `tag_clip` track are **authored BO2
parking** (the deploy panels fly off on `sndnt#fly_falling_panels`; the pin leaves the hand),
the playback rule `posed local = stored local + track` is BO2's (proven on the body rig), and
the sweep's aimed-half blow-outs are a harness staging signature — not a track bug. The new
ignored real-install test `equipment_subbone_tracks_are_bo2s_authored_parking` pins all of it
on the converted clips (exact values, slot mapping, the compose rule); gotcha #81 carries the
"do not filter large translations / do not read a track as absolute" warning. No VERSION bump
(test/docs only). The silhouette fix belongs to the `equipment-hip-pose` staging cycle.

Verified: cargo test --workspace green; the full real-install suite green (the new test
included); clippy clean; `cargo xtask build` clean; autotest 13/13 default (the offhand-throw
behavioral guard).

## The equipment hip pose: sample the held slot and retire the pixel criterion (2026-10-23, branch `v1/equipment-hip-pose`)

The `equipment-hip-pose` cycle (research and plan in `~/.opencode/plan/equipment-hip-pose/`; base
`main` @ `22c1f1e`). The blank equipment hip half in the viewmodel sweep was **not** a transform
bug: the harness's hip placement is the game's — identity at stand-still — and the equipment's own
clips author the item at the waist. The left palm (`tag_knife_attach`) sits 9.6–26.5″ below the eye,
so its NDC y is −1.55 to −18.8 and the first-person cone is empty for 12 of 13 items (the trophy's
tall geometry is the edge sliver). A hand-tuned hip offset would move the harness away from the game,
so the cycle changes only how the sweep samples and labels the pose.

The sweep now samples the slot the game actually plays while an off-hand is held: `HOLD_FIRE` at
frame 0 (`viewmodel.rs:181-190`), with a manifest check and an `IDLE` fallback note for a stale
archive (`nativevm/mod.rs:304-310`); guns and the melee knife keep `IDLE` (the knife's game slot is
`MELEE`, out of this cycle's scope). Both halves use the same slot and time, so the only difference
between them remains `ads`. The aimed half stays — it is the one image that shows the item in the
palm — but its `manifest.tsv` note now starts with the stable marker `ads half: harness artifact`,
because equipment never ADSes and `place::aligned` lifts the rig by the item's palm aim marker. The
old `equipment-offhand` ">200 px at hip" criterion is retired: the new install-gated ignored test
`the_equipment_palm_is_outside_the_hip_frame` asserts the mount in model space instead — the item
part hangs from `tag_knife_attach`, its root coincides with the palm, the palm is within 8″ of
`j_wrist_le`, and the palm is outside the hip view volume (`forward <= 0` or `ndc_y < −1`); it prints
one row per item. The AN-94 control keeps its gun-at-hip pose.

Verified: `cargo test --workspace` green (496 passed, 28 ignored across 19 suites); the three
real-install ignored tests green (`the_offhand_items_ride_the_knife_attach`, the new palm test with
13 printed rows all outside the frame, `equipment_subbone_tracks_are_bo2s_authored_parking`);
`cargo clippy --workspace --all-targets` exit 0 (warnings only in untouched files). The after set's
14 renders: all 13 equipment hip halves are 0 px (the trophy's idle sliver is gone — the plan allows
present or absent), the aimed halves keep the same order of magnitude but their counts move with the
sampled clip (flash 38,135 → 95,611; trophy 165,603 → 81,297), and the AN-94 pair is byte-identical
to the before set. Gotcha: the plan's §3.3 calls `ZHBO_SWEEP_BACK=12` "72 inches"; the argument is
inches (the code divides by 6 for world units), so 12 is 12 inches — a bottom sliver — and 72 is the
whole rig; the repo docs now say 72. Every pixel that is background in both a before and an after image is unchanged
(0 such pixels changed), so the camera, FOV and near plane are untouched. No autotest leg: the change
is a development-only example plus docs and an ignored test; the game's drawn path is unchanged
(AGENTS.md's ladder: no sim or ABI change). The ADS label and the slot line are in every equipment
row; the AN-94 row keeps its gun note. Follow-ups recorded in the plan: the primary-gun composition
gap (separate cycle), the frame-of-hold choice (one-constant change if the orchestrator wants the
steady pose), `Class::Melee`'s `MELEE` slot, and `--sweep`'s `FIRST_RAISE` for equipment.

## Integrating the sweep's equipment pose fidelity (2026-10-23)

Merged `v1/equipment-hip-pose` (2 commits; harness + one ignored test + docs — no shipped path,
no VERSION bump). The `equipment-hip-pose` cycle's verdict, measured: the blank equipment hip
halves were **BO2's own geometry, faithfully rendered** — the harness's hip transform already
equals the game's (stand-still `GunPos::step` returns zero), and the equipment clips hold the
palm 9.6–26.5″ BELOW the eye (NDC y −1.55…−18.8), so 12/13 blank is what BO2's numbers do, not
missing art. The real infidelities fixed: the harness sampled `IDLE` where the game plays
`HOLD_FIRE` for a held item (now sampled for off-hand classes, with a manifest `has_clip`
check and a labeled fallback note), and the equipment ADS half is now labeled in the manifest
note as the harness artifact it is (`place::aligned` lifts the rig by the palm aim marker —
equipment has no ADS). The old ">200 px at hip" criterion is retired for
`the_equipment_palm_is_outside_the_hip_frame` (model-space: mount on `tag_knife_attach`, palm
within 8″ of the left wrist, and outside the hip view volume — 13 pass, printed rows). The
AN-94 control pair is byte-identical; the background invariant held (camera/FOV/near
untouched). Doc note: `ZHBO_SWEEP_BACK`'s argument is INCHES (12 = bottom sliver; 72 = the
whole rig) — the plan's "12 = 72 inches" was a slip, corrected in the sweep's docs and
TESTING.md.

Verified: cargo test --workspace green (496/19 suites); the three install-gated suites green
(off-hand mount, palm-outside-frame, sub-bone parks); clippy clean both ways; `cargo xtask
build` clean. No autotest legs (approved plan §4.2: dev-only example + docs + ignored test;
no sim/ABI path).

Deferred (recorded in MODLOG/plan): the primary-gun composition gap (cycle
`equipment-offhand-composition`, gated on the user-routed VM capture of a held item at hip);
the frame-of-hold choice (`HOLD_TIME = 5.0` is a one-constant change); `Class::Melee`'s MELEE
slot; `--sweep`'s FIRST_RAISE for equipment.

## Integrating invisible-attachments phase 3a: classification, the nearest-bone fallback, and the frame verdict (2026-10-23)

Merged `v1/inv-att-3a` (3 commits). The gate split the cycle's plan into stages (3a classify +
c2 + c1-diagnostic; 3b frame change and 3c base-part replacement follow). Results:

- **The ZHBO_SWEEP_NO_HIDE A/B rules out (b) hide for all ten** (MAD 0.0 in both legs; the
  triangle-omission argument holds — attachment verts weight to their own bones). All ten are
  (c) mounting/placement.
- **c2**: absent/None attachment tags now resolve to the base gun's nearest bone by bind
  distance instead of `j_gun` (`native.rs` + the unit test). **Measured effect: almost none of
  the four c2 items move** — only `kard_mp+steadyaim` shows a small 87-px delta; the other
  three (`as50_mp+ir`, `as50_mp+swayreduc`, `judge_mp+dualclip`) are pixel-identical. Their
  invisibility is NOT the fallback: it lives in the frame (c1), the replacement gap (c3), or
  BO2-parity. Archive VERSION 57 → 58 (the manifest content changes for those items).
- **The c1 diagnostic settles `as50_mp+silencer` for Convention B** (mount-tag frame: the
  part lands outside the gun's bbox and inside the view cone). The four pistol dualclips and
  `ksg_mp+steadyaim` are ambiguous (both conventions put the part outside bbox AND cone —
  deeper than a frame choice); the `tag=None` items coincide (their mount is the gun root).
  3b's frame change, if any, is per-item via a manifest flag, not global.
- Regression guard (integrator-run, both legs forced on-demand conversion): every previously
  working item unchanged except two review items — `an94+gl` shows an 819-px localized delta
  in the front-sight/leaf-sight region (the launcher itself mounts identically in both; the
  default autotest passes) and `kard+steadyaim`'s 87-px move; **a playtest eyeball item, not
  a blocker**. Pre-existing: seven AN-94 attachment combos and two others are refused by the
  renderer under `ZHBO_SWEEP_CONVERT_ALL=1` in BOTH legs — a sweep-infrastructure follow-up,
  not this change.
- "Unresolved — parity suspected" per the plan's §4: the dualclips and steadyaims stay in
  that bucket until an in-game check or a BO2-side `au_*` geometry read confirms no visible
  part; the AS50 trio carries the base-pose caveat.

Verified: cargo test --workspace green; the full real-install suite green; clippy
--workspace --all-targets clean; `cargo xtask build` clean; autotest 13/13 default,
`--classpick` all checks, 14/14 `--rts`.

## The AS50 placement is BO2-parity, proven with numbers (2026-10-23)

The `as50-placement` cycle's verdict: **the AS50's big, forward pose is BO2's own data; nothing
is wrong.** The read-only probe (raw fields from the install's `common_mp.ff`, plus converted
bboxes) against the sibling snipers:

- `stand_move` is IDENTICAL to the DSR50's (`[-0.4, 0, 0]`) — the "offset too close/high"
  suspicion in the sweep's FLAGS.md §3.3 is false for the field.
- The AS50's MESH is the outlier, authored: it starts ~5.94″ closer to the camera
  (`mins[0] = -8.04` vs DSR50's `-13.98`) and extends ~9.49″ further forward
  (`maxs[0] = 42.62` vs `33.13`).
- `stand_min_speed = 0` (DSR50/Ballista: 80; SVU also 0) — BO2's value, keeping the stand
  offset applied at all speeds.
- The aim marker sits at the MUZZLE (`[42.345, 0, 3.163]` from `j_gun` vs DSR50's
  `[-0.811, 0, 3.668]`) — BO2's value, so the R6 `place::aligned` slide pulls the AS50 back
  ~42″ into ADS. Large, but data-correct.
- Raw and converted values match exactly; the schema round-trip covers them.

The "base-pose caveat" (the AS50 rows were "conditional until the base pose is confirmed")
lifts to: data-proven parity; the remaining check is the human eyeball in game (does the big
forward pose + the 42″ ADS slide READ right), a playtest item, not a bug. The AS50 trio's
attachment rows in the invisible-attachments cycle are no longer conditional on the base pose.
Cycle artifacts: `~/.opencode/plan/as50-placement/`.

## Integrating dual wield: the left gun composes, animates, and mounts on `tag_weapon1` (2026-10-23)

Merged `v1/dual-wield` (3 commits). BO2 composes dual wield at runtime from the base
`t6_wpn_pistol_<x>_view` and a left companion `t6_wpn_pistol_<x>_view_lh` (suffixed bones); no
monolithic `*_dw_view` exists. The converter's cascading detection (`_dw` suffix → `DWLEFTHAND`
inventory → `au_<weapon>_dw` attachment) loads the `_lh` model; the manifest records both
instances' clip tables (the `INSTANCE_LEFT = 0x100` clip-id bit; the blob VERSION stays 1).
Archive VERSION 58 → 59 (the DW composites add the second rig's blobs).

Two defects were caught by the integrator's GPU sweep and fixed in the branch: (1) **the left
instance was never animated** — only the base clip was sampled, so the left gun sat at the bind
pose; `nativevm::Loaded::pose` now overlays the left clip via `Skeleton::sample_over`, with the
`the_left_clip_overlays_the_base_pose` test. (2) **the mount was wrong** — the GPU A/B proved
`tag_knife_attach` is the equipment palm (barrel up/back) and `j_wrist_le` sits behind the hand;
the viewhands' **`tag_weapon1`** is the mount the `viewmodel_fnp45_dw_left_idle` clip animates
to the left gun's authored pose (track `[19.48, 5.90, -1.92]`, near-identity rotation), with
`the_dual_wield_mount_is_the_clips_left_weapon_mount` pinning it and `ZHBO_DW_MOUNT` as the dev
override. The final sweep: both pistols visible in BO2's DW pose, no degenerate spike.
`fiveseven_dw`'s left clip isn't in the searched zones (composition covered by the test).
Recorded follow-ups: a thin dark artifact at a fixed screen position in the pulled-back view
across three `_lh` models AND the base viewmodel's aimed half — an arms/nativevm artifact
exposed by the DW clips, outside the composition, to re-check on the rebuilt archive; the
third-person `_world_lh` path stays out of scope (its own cycle).

Verified: cargo test --workspace green; the full real-install suite green (incl.
`the_dual_wield_composite_loads_both_guns` over all five); clippy --workspace --all-targets
clean; `cargo xtask build` clean; autotest 13/13 default, `--classpick` all checks, 14/14
`--rts`.

## Integrating base parts and the sniper scopes: the empty scope mounts are art (2026-10-23)

Merged `v1/base-parts-scope` (3 commits; absorbs `scope-apertures` and the c3 base-part gap).
**The AS50's (and Ballista's, DSR50's, SVU's, M32's) scope is a separate `attachViewModel`
part the converter never loaded** — it read the array only for the magazine, so the scope
mount was an empty hole. The single `mag` read is now a `base_parts` list: every non-empty
entry loads at its authored offset on the gun root (the MP7 pattern), `tag_scope`/`tag_sights`
marks it optic and drives `tag_sights` (the snipers' ADS eye moves from the bare gun's
top-front to the scope window — `OPTIC_WINDOW` is the single knob). Replacement semantics are
data-driven through BO2's own two flags: `disableBaseWeaponClip` governs the magazine,
`disableBaseWeaponAttachment` (newly parsed, with the two adjacent fields parsed-not-applied)
governs the scope/attach art — proven disjoint by the exhaustive flag sweep (only the four
snipers' ACOGs and `au_ballista_is` set the attach flag; every dualclip sets the clip flag).
The M32's `_view`/`_ads` sight variants dedupe by material-set + bbox IoU (only `_view` loads;
the swap is a follow-up). FHJ18/crossbow/EMP: parity, untouched. Archive VERSION 59 → 60.

Measured acceptance (475-row sweep before/after): part counts moved on EXACTLY the five
weapons (nothing else changed); **the AS50's ADS hole went 749 → 0**; SVU 21 → 7; the ACOG
replacement holds (`as50+acog` 4 parts, no base scope; `ballista+is` 3, scope off, nothing
added); M32 2 → 3. **The ADS-usability A/B FAILED as routed: the physical scope's eyepiece
blocks the frame centre at full ADS** — BO2 solves this with its full-screen `scope_overlay_*`
swap, which is the next cycle (no lens/converter hack applied, per the plan). Also recorded:
a pre-existing `zhbo-gfx` refusal of meshes with a zero-index surface (101/475 sweep jobs —
an empty surface should be skipped, not refuse the mesh; its own fix), and the IR combos
staying partial (override offsets + `viewModelAdditional` parsed, unapplied).

Verified: cargo test --workspace green (505); the full real-install suite green (24 + 9,
incl. the four new base-part tests and the equipment/DW/MP7 suites); clippy
--workspace --all-targets clean; `cargo xtask build` clean; autotest 13/13 default,
`--classpick` all checks, 14/14 `--rts`.

## Integrating the scope overlay: the AS50 hands off to BO2's 2D scope at full ADS (2026-10-23)

Merged `v1/scope-overlay` (6 commits; the first cycle run under the diff-level plan rule —
`IMPLEMENTATION.md` carried exact edits and the implementation applied them mechanically in
per-sub-phase commits). The overlay material path: `overlayMaterial`/`overlayMaterialLowRes`/
`overlayReticle` parse from the `AttachmentUnique` (the AS50's base unique `au_as50_none` →
`scope_overlay_xpr50`, probe-confirmed) into the weapon table (schema 69 → 72; AGENTS.md's
stale schema path fixed in the same commit); the 1024-px overlay image converts as a
standalone page (not the atlas); at `ads == 1.0` the native viewmodel hides the base optic
**only when the overlay art is present** (the amendment: otherwise the physical scope stays —
three weapons' overlays live in map zones `Source::Dynamic` doesn't cover, recorded as the
search-extension follow-up) and the HUD draws the 2D overlay full-screen (the host's clip
callback clips 2D only, so the scissor is the image's own alpha — the plan's fallback, chosen).
FOV snap and no fade, per the dvar evidence.

Three integrator fixes were needed beyond the branch: the `--scope` autotest leg's shell code
(it broke the script's conventions — `$bin` undefined, repo-relative output, a custom log
path; rewritten to the standard `wait_for_log` pattern), **`Client::reset()` dropped the
`scope` autopilot flag** (it rebuilt the autopilot with only equipment/build/classpick, so
loading the save silently reverted the run to the default timeline — the reason the leg first
"ran the default script"), and the leg needed its own AS50 profile seed (the pick key selects
whatever the profile holds; it had the classpick leg's MP7 "AB"). The leg passes: `base optic
hidden at full ADS` logged in game with the AS50 in hand. The 150-frame screenshot interval
misses the 75-frame hold window, so the overlay's exact look remains the manual-capture item
(alongside the hide-whole-vs-scope-part and fade-vs-snap questions for it). Also recorded:
attachment variants inherit the base overlay material (variant-propagation follow-up), and
the sweep's `steadyaim` mesh-size refusal (pre-existing).

Verified: cargo test --workspace green (509); the full real-install suite green (24 + 9,
incl. the relaxed overlay-materials rule); clippy --workspace --all-targets clean; `cargo
xtask build` clean; autotest 13/13 default, `--classpick` all checks, 14/14 `--rts`,
`--scope` PASS. Archive VERSION 60 → 61 (standalone pages + new INI entries).

## Integrating the IR override: the base scope slides for the dual-band, and the mount has its own offset pair (2026-10-23)

Merged `v1/ir-override` (5 commits + the integrator's amendment). The IR combos' two parsed-
but-unapplied channels are now live: `overrideBaseWeaponAttachmentOffsets` REPLACES the base
scope part's offset in the weapon's viewmodel frame (measured — not additive: as50
`[4.456,0,4.253]` → `[9.889,0,4.253]`, ballista → `[12.529,0,4.737]`, dsr50 → `[-0.339,0,4.843]`,
svu → `[10.523,-0.033,6.564]`; the part is moved, never suppressed, and only when the flag is
set), and `viewModelAdditional` (the dualband mount) loads as its own part drawn BEFORE the
optic. **Correction to the cycle's research §4: the mount has its OWN offset/rotation pair
(`viewModelAddOffsets`/`viewModelAddRotations`, layouts offsets 116/140)** — the research said
"no offset of its own" because it missed the parallel pair; measured, the rail sits 0.73–1.09″
BELOW the optic (as50: `[1,0,4.711]` vs the optic's `[1.045,0,5.568]`), applied absolutely.

The amendment (the implementation's stop-and-report, decided by the integrator): `+ir`
COMBINED combos include the mount pair because `combo()` keeps the single `au_<w>_ir` entry
per short regardless of the exact combined unique — the same single-entry-vs-combined-unique
rule that was already drawing the dual-band OPTIC on combined combos (a pre-existing
over-draw, neither introduced nor fixed here; documented at the test). Measured acceptance:
`as50_mp+ir` 5 → 6 parts with every other weapon's render byte-identical. Archive VERSION
61 → 62.

Verified: cargo test --workspace green; the full real-install suite green (25 + 9, incl. the
amended combined-combo assertion); clippy --workspace --all-targets clean; `cargo xtask
build` clean; autotest 13/13 default, `--classpick` all checks, 14/14 `--rts`.

## Integrating the empty-surface fix: 475 renders, zero refusals (2026-10-24)

Merged `v1/zero-index` (1 commit). Zero-index surfaces were never in BO2's data or the blob
(catalogue scan: 0 across all MP zones) — they were CREATED by the viewmodel composer
(`nativevm::build_mesh_visible` drops hidden-bone triangles, then pushed a `SurfaceSpec` with
a surviving count of zero for fully-hidden surfaces, dating to `27b95ec`'s hide-tags change).
Three-site fix: the composer SKIPS empty surfaces (W3D parity, `character.rs:187-189`), the
renderer splits its refusal (skip + one dedup'd upload-time log line on `index_count == 0`;
the `% 3`/range refusals stay; a new `built.is_empty()` refusal so `DRAWN` never lies), and
the converter asserts corpus-wide it never writes one (`no_empty_surfaces` over every
converted viewmodel part plus every body/world weapon model). The blob's content is
unchanged — no `hud::VERSION` bump, no ABI change.

Acceptance: the full sweep now renders **`475 images, 0 failed`** (was 101 refused jobs).
Subset accounting: surfaces down exactly by the empty ones (51 in the refused set), vertices
byte-identical except the two `+ir` rows, which gain bones/verts/surfaces from the IR-override
archive rebuild (content drift, explained — not this change). The class-B finding (attached
parts mounted on HIDDEN stub tags; `under_hidden`, ~71k triangles) is deliberately not
touched — its own cycle, tracked by the same sweep's triangle total.

Verified: cargo test --workspace green (513); the full real-install suite green (25 + 9);
clippy --workspace --all-targets clean; `cargo xtask build` clean; autotest 13/13 default,
`--classpick` all checks, 14/14 `--rts`.

## Integrating brass parity: BO2's 41-eject / 131-silent split, oriented, on the tag's X (2026-10-24)

Merged `v1/brass` (3 commits). **Retraction: the "brass rides the xanim via notetrack" theory
was wrong** — BO2 has no eject note at all (only `sndnt#`/`rmbnt#` prefixes exist); its rule is
`viewShellEjectEffect` non-null → `EV_EJECT_BRASS` → the effect at the posed `tag_brass`, whose
X axis is the eject direction. 41 of 172 MP defs define it; the other 131 (the AN-94, every
other AR, every sniper, launchers) eject NOTHING and there is no default. The mod's trigger was
already right; the defects were three: the `ZHBO_ViewmodelBrass` fallback (made the 131 eject a
Zero Hour shell — DELETED, including its INI definition), the converted FXList never setting
`OrientToObject = Yes` (so ZH never applied the tag's matrix), and the surface-normal axis map
putting the eject on the tag's Z. Dual-wield ejects from BOTH ports (`tag_brass` +
`tag_brass1`, self-gating; matches the both-hands-fire presentation). The AN-94 is now
correctly silent.

The implementation's stop-and-report caught a FOURTH defect: `fx_pistol`'s shell models are
comma-prefixed reference stubs in the patch zone while the real model's inline material is
texture-less — the real model lives in `common_mp`, the real material in `patch_mp`.
`add_shell` now resolves cross-zone (model from the first zone with surfaces; material by name
when the inline one has no colour map). Result: **21 of the 30 in-table defs with the field
convert a non-empty, oriented shell FXList** (the fx_pistol family's 24 were empty before);
the other 9 are alt-mode/variant defs outside the FX loop's id set (the same pre-existing
population gap as their missing flash — widening it is a recorded option, not done). Archive
VERSION 62 → 63.

Verified: cargo test --workspace green; the full real-install suite green (25 + 9: the exact
41/131 split, the AN-94 silent, every shell FXList oriented with `VelOrthoZ` all zero); clippy
--workspace --all-targets clean; `cargo xtask build` clean (the bridge change's first
compile); autotest 13/13 default, `--classpick` all checks, 14/14 `--rts`, zero
missing-FXList lines. Eyeball for the playtest: AN-94 ejects nothing; the MP7's shell leaves
along the port's X, not straight up.

## Integrating R8d: first-person impacts with the surface's effect and blood (2026-10-24)

Merged `v1/r8d-impacts` (6 commits + 3 integrator fixes). The local player's shots now land
with BO2's impact table: per pellet the sim emits `EventKind::Impact { at, dir, surface, head,
fatal, impact_type }` (terrain surface from the NEW `LogicHost::surface_at` — the engine's
`TheTerrainVisual->getTerrainTile(...)->getClass()`, called from logic per the BuildAssistant
precedent; object surfaces from `objflags`; the bogus air-impact sound is gone with the
`Nothing` arm). The bridge resolves the name through `zhbo_impact_fx` (the table's
`nonflesh[surface]` cell, or the 4-way `flesh[]` blood variant = `head<<1|fatal`, BO2's own
FxFleshType) and plays it with `FXList::doFXPos` at the point with X = −dir. Tracers are OUT
(parity: `cg_firstPersonTracerChance` defaults 0). Two data findings are pinned: buckets are
bullet-only (7/9 are launcher explosion types, 14 is `fx_missing_impact`), and bucket 1 has no
nonfatal blood cells in BO2's data (so a nonfatal body hit plays no blood — faithful; the
fallback policy is the place to change it if it reads wrong). ABI 25 → 26 (both sides
together: `ZhboClientHit[8]` + `LogicApi.surface_at`, field counts, size asserts, FakeHost,
the CONTRACT rule that the surface value travels ONLY to FX/sound, never gameplay). Archive
VERSION 63 → 64 (`impacts.txt`).

The three integrator fixes beyond the branch: the bridge's shared `zhboFrameAt` needed
`ZhboBridgeInternal.h` in `ZhboViewmodel.cpp` (use-before-declaration); the script's impact
greps used `player 0` where this game logs `player 2` (now `player [0-9]+`); and **the impact
oracle's first form was defeated by the client's per-client-frame `sim.events.clear()`** — a
frame-indexed count only ever saw the print frame's events, hiding any kill on the other 29
frames — so it now accumulates counts every logic frame into a static per-player table and
prints + resets on the 30s. Verified: cargo test --workspace green; the full real-install
suite green (25 + 9); clippy --workspace --all-targets clean; `cargo xtask build` clean;
autotest 13/13 default INCLUDING `impacts: flesh`, `impacts: terrain`, and `impact fx played
(N played, 0 missing)`, `--classpick` all checks, 14/14 `--rts`.

## Integrating the ADS pose: the aimed pose is the authored clip, not a computed slide (2026-10-24)

Merged `v1/ads-pose` (6 commits; absorbs the `ads-alignment` + `ads-transition-clips` research).
**BO2's aimed pose is animation data, not a sight alignment: every weapon's `ads_up`/`ads_down`
keys exactly one bone, `tag_torso` (the rig root), with the rigid hip→aimed transform; the
engine adds only `fAdsAimPitch` and the offset fade, and `tag_sights`/`tag_scope` never appear
in the executable.** The mod's native path was substituting `place::aligned` — a translate-y/z
slide that kept the hip depth and rotation — right only where three coincidences held. The fix:
layer the converted 0x55/0x56 clips over the action clip with the existing
`Skeleton::sample_over`, scrubbed by the ADS fraction (`frame = ads · N` up / `(1−ads) · N`
down — provably the rate-scaled mapping while the fraction is linear, and it reproduces the
measured 63 %-at-half S-curve), and retire the slide on the native path only (the W3D fallback
and clip-less weapons keep it). Reversal switches clip at the same fraction, no cross-fade
(the twins are continuous to 4 decimals). Everything the slide never did falls out free: **eye
relief on all 42 guns** (the AS50's scope leaves the camera), **the launchers straighten**
(SMAW 19.9° → 0.38°), **the DW pair stops moving at ADS** (BO2's DW clip is a literal no-op),
and the eased blend replaces the linear lerp. **Authored residuals are parity, not bugs**: the
KSG's sight sits 2.2″ below centre and the FHJ-18 keeps a 2.32° cant — measured and preserved;
do not re-align them (the sweep review and this entry are the guard against "fixing" them
back). Fire/reload at ADS keeps the aim (the clips are channel 2); reloading while aimed now
retracts through `ads_down` (BO2's shape, a visible change). No ABI, converter, manifest or
VERSION change — the clips were converted since R6; only the state machine never asked for
them. Open Ghidra question recorded: the engine's time-scale vs `fWeaponPosFrac` cross-fade
and the reversal blend time.

Verified: cargo test --workspace green (536, incl. the six layer tests); the full
real-install suite green (26 + 9, incl. `the_ads_clip_reproduces_bo2s_authored_pose` against
the measured table); clippy --workspace --all-targets clean; `cargo xtask build` clean;
autotest 13/13 default (the aimed burst), `--classpick` all checks, 14/14 `--rts`, `--scope`
PASS (the AS50 held past the transition with the new pose). The sweep's ADS halves change
exactly per family (hip halves untouched everywhere).

## Integrating the camo swap: the class's camo renders on the weapon (2026-10-24)

Merged `v1/camo-swap` (6 commits). **BO2 camos via a camo LAYER texture (semantic 22): every
camo-capable surface carries a layer whose default is `camo_off_solid`/`camo_off_pattern`;
pattern camos replace that one image (tiled by the per-weapon `WeaponCamoSet` offset/scale +
`shaderConsts`), and the 15 style camos additionally swap whole materials (the
`camoMaterialSets` pair list, name-matched per surface material).** The cycle fixed five gaps
in dependency order: the parser (`read_camo` returned None images, mojibake pair names, one
pair, no boundaries, no shaderConsts — its ignored test passed on garbage); no camo art
converted (only the 46 thumbnails); the blob/`SurfaceSpec` dropped the layer and the material
name; the choice never left the class layer (`PlayerSim.camo` now carries it, transported
core-global with NO ABI change); and the renderer had no camo input (a second texture binding,
a 32-byte material uniform, and `albedo * mix(1, camo.rgb * gain, camo.a)` with `gain = 2.0`
— picked by the C0 probe's coverage-alpha evidence, 1.0 the recorded fallback). The recipes
live in a new versioned `camo_<weapon>.zcm` blob per weapon; the table's `CamoList`/
`CamoMaterial` fields are unchanged; `replaceFlags`/`shaderConsts` are carried, NOT
interpreted (no hand-copied numbers).

**38 camo assets resolve** through a data-driven view-model join (never an alias table):
`camo_xpr50` → `as50_mp` and peers; the 7 attachment camo sets are excluded by design
(optic-camo is a follow-up, recorded); 3 assets stay unresolved (`camo_b23r`, the model is
`b2023r`; `camo_peacekeeper`, `camo_specter` — absent from the table); `multicam`/`matuka`
have no art anywhere in the 48 assets and draw stock (the option-id enum is a user-routed
Ghidra question). First-person native viewmodel only this cycle (W3D fallback and the
third-person native body draw no camo — documented divergences). The sweep found and fixed a
load-order bug (the stock mesh cached before the `.zcm` arrived). `zhbo_native::model::VERSION`
2 → 3 (the camo byte) and archive VERSION 64 → 65.

One merge-semantics fix beyond the branch: `nativevm`'s `Loaded` gained both the 3-tuple key
and the `camo` field on the two sides of the merge; the test fixture needed
`key: (7, b'U', 0), camo: None`.

Verified: cargo test --workspace green (incl. --all-features for the shader); the full
real-install suite green (26 + 9); clippy --workspace --all-targets clean; `cargo xtask
build` clean; autotest 13/13 default, `--classpick` all checks, 14/14 `--rts`. Sweep: gold
and diamond change the right pixels per weapon (style pairs share `zhbo_camo_code_spc` — the
MP7's gold/diamond are identical, a `shaderConsts` fidelity note, not a bug).

## Integrating the per-bone hide: 71,435 attachment triangles back (2026-10-24)

Merged `v1/per-bone-hide` (2 commits). The composer's `under_hidden` collapsed the whole
subtree below a hidden bone, so an attachment mounted on a hidden sight/grip stub tag (the
nearest-bone fallback's target when BO2 leaves `viewModelTag` empty — most attachment
uniques) had its WHOLE part hidden even though the part isn't skinned to the stub. The parity
model is the in-game-verified W3D path's: **`hidden = hidden_here`** — only the geometry
skinned to the hidden bone is hidden; what mounts on it draws (`pose.rs`, `character.rs`'s
precedent). The evidence chain: a hidden mount must not hide what is mounted on it; BO2's
hide lists enumerate sibling bones rather than relying on hierarchy; and the W3D path already
draws these same attachments correctly in game. Class A (directly-named empty surfaces) is
unchanged by construction. **Measured acceptance: +71,435 triangles across the 40 affected
combos (optics, silencers, grips, GL, DBAL, BCPU, IR, vzoom), zero gun-triangle change.** BO2's
exact hide granularity and its default mount for an empty `viewModelTag` stay open as the
user-routed Ghidra question (the exact ask is in the cycle's plan dir); if it proves BO2 hides
more, the mount rule is the separable follow-up. No converter/blob/ABI change and no VERSION
bump.

Verified: cargo test --workspace green; the full real-install suite green; clippy
--workspace --all-targets clean; `cargo xtask build` clean; autotest 13/13 default,
`--classpick` all checks, 14/14 `--rts`.

## Next steps

The macOS port is built and passing its in-game autotest (`scripts/autotest-mac.sh`); setup and
macOS-specific gotchas live in [docs/PLATFORMS.md](docs/PLATFORMS.md). Start from `AGENTS.md` and
the docs read order there. The list below is the older gameplay backlog.

1. Manual playtest by a human with real mouse/keyboard (the autotest injects input inside the mod).
2. More BO2 weapons (only the AN-94 viewmodel and the B23R sound are converted).
3. BO2's own reflection probes: keep `GfxImageLoadDef` data when the zone loader reads it.
4. Save/load of first-person state; two-client multiplayer test.
5. Strike Force (`so_rts_*`) content as a natural next theme.

## Invisible attachments phase 3a: nearest-bone tag fallback and the convention dump (2026-10-23, branch `v1/inv-att-3a`)

The `invisible-attachments` cycle's phase 3a landed two changes and one diagnostic. The ten
weapon+attachment combinations that render pixel-identical to their base weapon split into three
sub-shapes of (c) mounting, not hide tags: the `ZHBO_SWEEP_NO_HIDE` A/B (run per the plan's §2
pre-step) leaves MAD at 0.0 in both legs for all ten, confirming RESEARCH §2.4's triangle-omission
reasoning — an attachment's vertices weight to its own bones, so the base gun's hide list cannot
drop them. The hide hypothesis is ruled out.

**c2 — nearest-bone tag fallback.** When an attachment's `viewModelTag` is None or names a bone
absent from the base gun, the converter fell back to the gun's first bone (`j_gun`). The authored
`viewModelOffset` is measured in the gun's local frame, so the nearest bone to that offset is the
better mount. `nearest_bone` in `crates/zhbo-assets/src/native.rs` now resolves the tag by
minimum bind-position distance; the four c2 items (`as50_mp+ir`, `as50_mp+swayreduc`,
`judge_mp+dualclip`, `kard_mp+steadyaim`) take this path. A unit test proves a tag=None attachment
near a named bone resolves to that bone, not `j_gun`. No manifest schema change, no VERSION bump.

**c1 — convention dump.** The ignored test `dump_attachment_conventions` in
`crates/zhbo-assets/src/viewmodel_sweep.rs` prints, per item, the mount tag's bind, the gun's root
bind, the authored offset, and the composed position under Convention A (weapon-origin frame) and
Convention B (mount-tag frame), compared against the gun's mins/maxs and the view cone. The
verdict table (saved in the plan directory as `c1_convention_verdict.txt`) settles one rotated-tag
item — `as50_mp+silencer` lands outside the bbox and inside the cone under B — and leaves the four
pistol dualclips and `ksg_mp+steadyaim` ambiguous (both conventions put the part outside the bbox
but below the cone). The plan's §3.1 default applies: adopt Convention A behind a manifest flag in
cycle 3b, so the regression guard can flip to B per item. The four tag=None items coincide under A
and B; their path is c2/c3, not the frame convention. The AS50 trio carries the base-pose caveat
(PLAN §7.1): its base pose is an outlier, so its rows are conditional until the base pose is
confirmed in game.

**Blocker for the sweep evidence.** The `render_viewmodels` example requires the converted
`ConquerOps_BO2.big` for base weapons; the archive does not exist on this workstation (the
launcher's dist folder holds only `ConquerOps.big`), and the cycle's instructions forbid
`cargo xtask build`. The §5.2 before/after sweep sets and the §7.2 full-sweep regression guard
therefore could not be run here. The unit tests, the real-install suite (`converts_from_real_install`,
the equipment sub-bone pin, the off-hand mount and palm tests), and clippy are green; the sweep
rows remain to be filled by the integrator on a machine with the archive. The c2 fix's behavior
is proven by the unit test and the real-install suite's mount assertions, not by the sweep.

Verified: `cargo test --workspace` green (all suites); `cargo clippy --workspace --all-targets`
clean for `zhbo-assets` (pre-existing warnings in `zhbo-gfx`/`zhbo-core`/`zhbo-formats` only);
the real-install suite green (`ZHBO_BO2_DIR=~/Games/BlackOps2 cargo test -q --release -p
zhbo-assets -p zhbo-formats -- --ignored`: 19 + 9 passed); the c1 diagnostic test prints the
convention table. No VERSION/ABI bump (the c2 fix changes only the chosen tag; the manifest
schema is unchanged).

## Dual-wield viewmodels: the left gun as a second rig on the left palm (2026-10-23, branch `v1/dual-wield`)

BO2 ships dual-wield pistols as two single-gun viewmodels — the base `t6_wpn_pistol_<x>_view` and a
bone-suffixed `_lh` duplicate — not as one composite model. The converter now detects the pair
(the weapon id's `_dw` suffix, the `DWLEFTHAND` inventory, or the base pistol plus the
`au_<weapon>_dw` attachment), loads the `_lh` companion, and composes it as a second rig on the
left palm. The five DW pistols (`fnp45_dw_mp`, `fiveseven_dw_mp`, `kard_dw_mp`, `judge_dw_mp`,
`beretta93r_dw_mp`) each gain the left gun's bones, vertices and surfaces over the base composite;
the ignored `the_dual_wield_composite_loads_both_guns` test proves it from the real install.

**Mount A/B.** The candidates were `tag_knife_attach` (the equipment-offhand default, parented to
`j_wrist_le`) and the fallback `j_wrist_le`. The `zhbo-assets pose` renders of the `_lh` guns on
the USA viewhands (`c_usa_mp_seal6_longsleeve_viewhands`) put the gun at the left palm for
`tag_knife_attach`; its bind position (`[16.49, 6.45, -14.09]`) is the palm, 2.7″ from the wrist
joint (`[13.79, 8.04, -13.77]`). The verdict is `tag_knife_attach`; the fallback stays available
via `ZHBO_DW_MOUNT=j_wrist_le` for a future comparison. Renders: `/tmp/dw_knife.png` (default),
`/tmp/dw_wrist.png` (fallback), `/tmp/dw_kard.png`, `/tmp/dw_judge.png`; the 640×360 hip slot's
left-palm region is x ≈ 180–360 px, y ≈ 220–340 px. The `pose` CLI's CPU rasterizer was used
because the GPU `render_viewmodels` example needs the converted archive, which this workstation
does not have; the integrator should fill the full-sweep rows on a machine with the archive.

**Clip routing.** The left gun's clips are keyed by the matching right slot with the
`INSTANCE_LEFT` (`0x100`) bit in the manifest's flat clip table. `nativevm::Loaded::clip_path`
masks the bit off for the lookup and reapplies it to the fallback, so a left clip falls back to the
left idle, not the base idle. The base W3D path's clip list stays unchanged; the left gun's clips
are native-only. The clip-routing unit test in `zhbo-core/src/nativevm/tests.rs` proves the base
and left clips resolve independently.

**Classpick.** The autotest's `--classpick` seed keeps its MP7 primary: changing it would break the
`MP7 <n>\|` kit assertion. The manual-capture procedure is in `docs/TESTING.md`; the seeded
profile's `secondary=` can be swapped to a DW pistol in a local run without touching the script.

Verified: `cargo test --workspace` green; the real-install suite green (`ZHBO_BO2_DIR=… cargo test
-q --release -p zhbo-assets -p zhbo-formats -- --ignored`: 20 + 9 passed); `cargo clippy
--workspace` clean in the touched files (pre-existing warnings elsewhere). No VERSION/ABI bump:
the serialized manifest's `Part`/`Viewmodel` structs are unchanged; the clip id's high bit is
reused and the blob `VERSION` stays 1. The converter's input `native::Viewmodel` gains an optional
`dual` field; the integrator should decide whether that warrants a bump when merging.

## Dual-wield integration fix: the left clip overlay and the `tag_weapon1` mount (2026-10-23, `v1/dual-wield`)

The integrator's GPU sweep with the rebuilt archive showed two defects the CPU A/B missed. **First,
the left gun was absent from the normal hip frame.** The base/right clip never tracks the left arm
or the `_lh` gun, so the native composite posed the left instance at the base clip's bind; the
`clip_path` routing table was correct but nothing ever requested or sampled the left clip. **Second,
the `tag_knife_attach` mount rotated the gun up/back** (a thin dark spike in the 12″-pulled-back
view; the grip was at the left edge, the barrel off-frame).

**Fix — sample both clips.** `nativevm::Loaded::pose` now takes the base and left anims, samples the
base, then overlays the left with the new `Skeleton::sample_over` (`crates/zhbo-native/src/pose.rs`):
the left clip's tracks (left arm, `tag_knife_attach`/`tag_weapon1`, the `_lh` gun) replace the base
clip's for those bones, while the right arm and base gun keep the base clip. `ffi::frame` loads
`clip_path(clip)` and `clip_path(clip | INSTANCE_LEFT)` (skipping the duplicate for shared clips).
New test `the_left_clip_overlays_the_base_pose`; the load-count test expects the extra left clip.

**Fix — mount `tag_weapon1`.** The plan's A/B candidates were `tag_knife_attach`, `j_wrist_le` and
`tag_weapon1`. The GPU A/B renders: `tag_knife_attach` points the gun up/back (the equipment palm's
rotation, not a gun mount); `j_wrist_le` points it forward but sits behind the hand; `tag_weapon1`
is the viewhands' second weapon mount and the dual-wield left clip animates it to the left gun's
authored pose (`viewmodel_fnp45_dw_left_idle`'s `tag_weapon1` track, `[19.48, 5.90, -1.92]`, rotation
near identity). The default `DW_MOUNT` is now `tag_weapon1`, with the `ZHBO_DW_MOUNT` dev override
kept for future A/B. New test `the_dual_wield_mount_is_the_clips_left_weapon_mount`.

**Acceptance evidence.** The normal 640×360 hip frames for all five DW pistols with the overlay and
`tag_weapon1` show the left gun in the left side of the frame (fnp45/judge/kard inspected; fiveseven
and beretta rendered too): `/tmp/dw-final/<weapon>/base.png`. The normal frame is clean. The
12″-pulled-back fnp45 hip frame still shows a thin dark artifact at the right edge of the hip half;
hiding the dual gun's `tag_clip1` magazine or the viewhands' `tag_knife_attach` knife does **not**
remove it, and the base viewmodel's pulled-back aimed half shows the same class of artifact, so it
is a pre-existing nativevm/arms artifact outside this change set, not the dual gun's skin data
(the real-install probe showed the `_lh` model's vertices use bones 0–4 of its six-bone table, and
the per-part max indices stay inside each part). The acceptance hip frame is clean; the integrator
should re-check the pulled-back view against the base on the rebuilt archive.

Verified: `cargo test --workspace` green (52 `zhbo-assets`, 364 `zhbo-core`, 21 `zhbo-native`);
the real-install suite green (`ZHBO_BO2_DIR=… cargo test -q --release -p zhbo-assets
-p zhbo-formats -- --ignored`: 20 + 9 passed); clippy clean in the touched files (the remaining
warnings are pre-existing in untouched modules). No VERSION/ABI bump.

## Base parts and scope replacement: the base-attachment channel (2026-10-23, `v1/base-parts-scope`)

The sniper scopes were missing from every converted viewmodel. BO2 composes the AS50 — and the
Ballista, DSR50, SVU and M32 — from the gun body **plus** a second `attachViewModel` part. The
converter read that array only to find the magazine (`viewmodels.rs` `find("attach_mag")`), so
the scope model was never loaded, textured, attached or drawn: the AS50's 744 px ADS "hole" was
the empty scope mount, not an aperture. Adding any optic attachment collapsed it (0–12 px), which
is the A/B proof.

**Two channels, BO2's own two flags.** `AttachmentUnique` now parses
`disableBaseWeaponAttachment`, `overrideBaseWeaponAttachmentOffsets` and
`viewModelOffsetBaseAttachment` (flags used; offsets parsed now, **not applied** — see the
deferrals). `Combo::no_base_attachment` is computed over both selection paths (the single
uniques and the exact multi-attachment unique), read **only** from the flag — never from a
name and never from `hide_tags`:

| Channel | Members | Suppressed by |
|---|---|---|
| magazine | the `attach_mag` entry (`no_clip` already governed it) | `disableBaseWeaponClip` |
| attachment art | every other non-empty `attachViewModel` entry | `disableBaseWeaponAttachment` |

The exhaustive sweep gave exactly four weapons a setter: the AS50/DSR50/SVU/Ballista ACOGs and
`au_ballista_is`. All five have `disableBaseWeaponClip=0` (the ACOG keeps the magazine) and every
dualclip sets the clip flag with the attachment flag 0 — the two flags select disjoint sets.
`au_ballista_is` has no view model at all, so suppression must fire with no replacement geometry:
the scope simply comes off. IR/VZOOM uniques leave the flag 0 and stack on the scope, so optic-
point matching would have been wrong.

**Which base parts are separate art.** The non-magazine parts are not baked into the gun: each
has its own `tag_scope` and its own lens/body materials, and none is fully inside the gun's
bounding box (the magazine is 96–100 % inside and still drawn, so "inside the bbox" cannot mean
"baked" — the rule is a sanity check on this channel only):

| weapon | non-mag part | authored offset (in) | part bbox in the gun frame | inside gun bbox |
|---|---|---|---|---|
| as50_mp | `t6_wpn_sniper_xpr50_scope_view` | [4.456, 0, 4.253] | [-1.24,-1.77,3.93]..[12.20,1.71,7.40] | 10 % |
| dsr50_mp | `t6_wpn_sniper_dsr50_scope_view` | [-5.701, 0, 4.843] | [-4.90,-1.66,3.04]..[8.89,1.64,6.51] | 56 % |
| svu_mp | `t6_wpn_sniper_svu_scope_view` | [4.253, -0.033, 6.839] | [-2.22,-1.07,4.18]..[10.05,1.08,6.93] | 6 % |
| ballista_mp | `t6_wpn_sniper_ballista_scope_view` | [5.312, 0, 4.737] | [-0.83,-1.71,3.10]..[13.55,1.64,6.76] | 38 % |
| m32_mp ×3 ids | `t6_attach_optic_m32_view` | tag `j_reload`, [5.191, 0, 1.563] | [4.80,-0.80,4.80]..[8.54,0.70,6.84] | 18 % |

**The implementation.** `weapon_viewmodels` builds a `BasePart { model, tag, offset, magazine,
optic }` list from the raw `attach_view_models`/`attach_view_offsets` pairs (not
`base_view_parts`, which loses the offsets). The magazine keeps its exact old semantics (first
`attach_mag` entry, its authored offset, the `magazine()` name search only as the fallback, gated
by `is_gun`/`no_clip`). Every other non-empty entry loads when `!no_base_attachment`, at the gun
root (`j_gun`) with its authored offset; the empty authored tag is `j_gun`. `optic` is the model
having a `tag_scope`/`tag_sights` bone, not its name. The W3D meshes use the free `B{k}` prefix
(the magazine keeps `M` and its rigid `N` copy on `tag_clip1`). The conversion report now appends
`, N base parts (names)` so a dropped part is visible in the launcher log — it was silent before
precisely because the scope was never loaded.

**Native plumbing.** `native::Viewmodel` carries `base_parts: Vec<BasePart>` in place of
`mag_offset`; the parts are pushed after the gun and before the attachments, so
`aim_marker`'s `parts[1]` is still the gun and an attachment optic still overrides a base optic.
`aim_marker` takes the optic's part index + mins/maxs instead of an `xmodel::Model`. The base
optic drives `tag_sights` (`OPTIC_WINDOW` up its glass): the marker is created when the gun has
none and moved when the gun already carries one. That changes the four snipers' ADS eye from the
bare gun's top-front to the scope window — the intended fix, and the reason the ADS A/B below
matters.

**M32 dedupe.** The M32 lists `_view` (hip) and `_ads` (aim) at the same offset: complete
alternatives with identical lens/reticle materials and bboxes within 0.12 in. Loading both would
draw the glass twice and there is no ADS-state swap in our manifest, so `same_sight` skips a later
part with the same non-empty surface-material set and bbox IoU ≥ 0.9. The first entry (the hip
sight) wins. The `_ads` swap is deferred, not a bug to rediscover.

**Sweep (§4.2).** Part counts moved on exactly the five weapons and nowhere else (475-row
manifest diff): as50 3→4, svu 3→4, dsr50 3→4, ballista 3→4, m32 2→3; every combination follows
(+1, except the ACOG/`is` combos, which stay level because the base scope is suppressed). The
AS50's ADS hole collapses from the baseline's 744 (749 by the locally reimplemented heuristic) to
**0**; svu 21→7; m32's 1→7 is the sight's own aperture (a see-through ring, visible in the
render), not an enclosed background region. The ACOG render is 4 parts with no `xpr50_scope_view`
and the Ballista's `is` is 3 parts with no `ballista_scope_view`.

**ADS usability A/B: FAIL.** The AS50's `base.png` aimed half now shows the scope's eyepiece
filling the centre of the frame; the world is visible only above the tube, not through the glass.
The physical scope is on the aim axis and blocks the centre at full ADS without BO2's full-screen
`scope_overlay_*` mask (unparsed). The part-count goal is met; the ADS goal is **not**. Do not
hack the lens or hide the base optic by name — the fix is the overlay feature cycle, as §3.6
says. The integrator's manual in-game AS50 capture should settle the eye position in a real
match (the harness camera is fixed).

**Three deferrals (carried from §7).** (1) `overrideBaseWeaponAttachmentOffsets` and
`viewModelAdditional` parse but are not applied: the IR override frame is not established from
data (the AS50 IR optic sits at x≈1 while the overridden base scope sits at x≈9.9) and every
acceptance combo has `overrideOff=0`; the IR combos stay "partial". (2) The pistol dualclips set
neither flag and have no separate base clip part, so their fast-mag sleeve is an additive overlay
on the baked magazine — no suppression, no new part; the offset-frame defect is the c1 cycle.
(3) The M32 `_ads` sight swap and the ADS overlay are future cycles.

**Pre-existing, not this change:** the macOS sweep refuses 101 of 475 jobs because a freshly
converted model can contain a zero-index surface (`zhbo-gfx`'s "surface 5 indices N..N") — it
reproduces on unmodified HEAD for ballista, dsr50 and many attachment models, and predates this
cycle. Those rows still report the composed bone/vertex/surface counts, and the part deltas are
+1 exactly where the table says. The ballista/dsr50 ADS hole rows could not be measured on this
workstation; the Windows VM sweep should fill them.

Verified: `cargo test --workspace` green; `ZHBO_BO2_DIR=… cargo test -q --release -p zhbo-assets
-p zhbo-formats -- --ignored` green (24 + 9: the new base-part tests, equipment, dual wield,
MP7); `cargo clippy --workspace --all-targets` clean in the touched files (the remaining warnings
are pre-existing). No `hud::VERSION` or ABI bump: `Part` is unchanged, the manifest gains only
more entries, and the integrator bumps `zhbo_assets::hud::VERSION` 59→60 when merging.

## Scope overlay (2026-10-2X)

* The AS50's ADS hand-off: at `ads >= 1.0` the physical scope part is hidden by the viewmodel draw
  and the screen-space overlay material replaces it. The HUD draws the overlay in the 2D pass;
  the native viewmodel's `visible_parts` filter drops the optic's surfaces at full ADS.
* The overlay materials are standalone 1024-px pages (not packed into the HUD atlas): each is
  written as its own texture named after the material (`ZHBO_Mat_<material>`). The HUD's
  `OVERLAY_MATERIALS` list in `crates/zhbo-core/src/hud/mod.rs` defines the seven snipers.
* No ABI change: the draw uses the existing `Prim::Image` and the host's clip callback (which clips
  2D images only, not the 3D view). The base-optic hide is the `visible_parts` predicate, gated on
  the overlay material's MappedImage being present in the archive's `ZHBO_BO2.ini`: a weapon whose
  overlay art never converted keeps its physical scope rather than leaving neither sight.
* The three map-zone-only overlay materials (`scope_overlay_envg_ir`, `scope_overlay_specter`,
  `scope_overlay_vzoom`) are allowed-and-reported by the install-gated build test; the four that
  convert from `common_mp` (`ballista`, `dsr50`, `svu`, `xpr50`) must stay present. Follow-up: extend
  the converter's `Source::Dynamic` search to the map zones (`pakistan`, `angola_2`) for these three.
* Verified: `cargo test --workspace` green; the converter test is install-gated (the AS50 parser test
  uses a synthetic zone); the ignored `builds_from_real_install` test now passes with the relaxed
  rule and reports the three missing map-zone overlay materials.

## Zero-index surfaces (2026-10-24)

* The `native mesh refused: surface N indices X..X of X` refusal (101 of 475 sweep jobs) is neither
  BO2 data nor the converter: no converted blob (all 475 jobs, every LOD) and no raw `XSurface` in
  the converted zones has a zero-index or vertex-less surface. The **composer** created them —
  `nativevm::build_mesh_visible` dropped the triangles of hidden bones (`27b95ec`, merged
  `d146434`) and still pushed a `SurfaceSpec` with a surviving count of zero. The earlier "0 failed
  jobs" sweep (`9202b3b`) predates that commit by 4h53m; the "pre-existing" reading at
  `MODLOG:2993`/`:3289` was relative to the base-parts cycle, not to the base game.
* Fix: the composer skips an all-hidden surface (W3D parity, `character::skin_hiding` drops an
  all-hidden mesh), the renderer skips a zero-index surface with one dedup'd log line instead of
  refusing the mesh (a no-op draw is not corruption; the `% 3` and range checks still refuse), a
  mesh left with no drawable surface is refused so the caller falls back to W3D, and the
  converter's real-install test now asserts no blob surface is empty. Expected sweep deltas:
  101 → 0 failed, and the composed **surface** counts drop by the empty surfaces (255 across the
  101 rows); composed **triangles must not move** (the hidden ones are still dropped).
* **Not fixed here (separate cycle):** those same surfaces are why 71,435 triangles of attachment
  art never draw — `pose::compose`'s `under_hidden` hides a whole attached part when its
  nearest-bone-fallback mount lands on a stub tag the combination's `hideTags` hide (class B: 130
  surfaces over 40 jobs, 17 attachment models). Striking an empty surface makes those triangles
  invisible *quietly*; that cycle's acceptance is +71,435 composed triangles with no other change,
  measured with the same sweep after this one lands.
* Latent, documented not fixed: `zhbo-formats/src/xmodel.rs:285-287` returns `Surface::default()`
  for a quantized surface, which would fabricate an empty blob surface for a zone that had one
  (none in the converted zones; the new blob assertion would catch it).
* Third person is code-path reasoning only: a ballista/DSR50 world model carries the same
  `tag_sights` surface, so `ZHBO_NATIVE_BODY=2` would previously have lost the whole rig to a
  refusal; it now loses one surface and draws. Not captured in game.
* No `hud::VERSION` or ABI bump: the blob content and the ABI are unchanged; only the composed
  mesh's surface list changes.
## Shell ejection is the def's effect, not a notetrack (2026-10-24)

Phase 1 settled the R8c follow-up's premise: BO2 has no eject notetrack at all — common_mp's 4,233
XAnimParts carry only `sndnt#`/`rmbnt#` prefixes plus `end`, `laser_on/off` and `melee_knife_stab`,
and no `*_fire` clip has a note but `end`. Brass is `EV_EJECT_BRASS` → `CG_EjectWeaponBrass` → the
def's `viewShellEjectEffect` at the posed `tag_brass` (its X is the eject direction; `fx_pistol`'s
velocity is all +X). 41 of 172 common_mp defs define it; the other 131 — the AN-94, every other AR,
every sniper, the M32, the launchers, the crossbow — are silent, with no default
(`defaultweapon_mp` carries `fx_rifle` explicitly). The only brass-shaped note is the pump guns'
`sndnt#prj_brass_impact_plr_shell_default` on rechamber: audio, not a spawn.

The generic `ZHBO_ViewmodelBrass` fallback is gone (its FXList deleted from `ZHBO_FX.ini`): a shell
plays only when the table has a `ViewShellFx`. Three conversion defects went with it, all of which
made the 41 look wrong: the shell FXList never set `OrientToObject`, so ZH never applied the bone's
matrix and the velocity stayed in world axes; `fx::particle_system` mapped BO2 `(X,Y,Z)` to
`(VelOrthoZ,VelOrthoX,VelOrthoY)` — the surface-normal convention of the world effects — putting
the eject on the tag's Z; and the shell resolver looked for the model and its material in the patch
zone only, where several families are comma-prefixed reference stubs (the pistol's real model lives
in `common_mp`, its `mc/mtl_fx_shell` material in `patch_mp`), leaving those FXLists empty. Shell
FXLists now orient and use an identity map, and the resolver takes the base zone as well, falling
back to a material asset by name when the inline stub has no colour map; world effects are
unchanged. `tag_brass1` is played too, self-gating on the bone: a dual pair ejects both ports, a
single gun one.

Not done: `vViewShellEjectOffset`/`Rotation` (nonzero for scar/hamr/svu/ksg), the last-shot field
(identical in all 41, not on the ABI), world brass, the same `OrientToObject` gap in the per-weapon
flash FXLists. The integrator's merge bumps `hud::VERSION` 62 → 63; no ABI change. Verified in this
phase: `cargo test --workspace`, clippy, and the install-gated suite (`cargo test -q --release -p
zhbo-assets -p zhbo-formats -- --ignored`, 25 + 9 green); the in-game autotest and the eyeball pass
(AN-94 silent, MP7 along its port) are the integrator's merge legs.
## R8d: first-person impacts

* BO2 draws **no first-person tracer** by default: `cg_firstPersonTracerChance` is registered
  `default = 0, min = 0, max = 1` (`~/Documents/BO2-reference/ghidra/answers/raw/_g_view.c:210-213`)
  and it is the only tracer-chance dvar. Their absence here is parity, not an omission — do not
  "fix" it without a decision to diverge.
* BO2's impact is table data, not a per-weapon effect: the weapon's `impactType` picks an
  `FxImpactTable` entry, the hit surface picks the cell, and the four `flesh[]` cells are
  body/head × nonfatal/fatal. The mod carries the bucket on the weapon row (`ImpactType`) and the
  cells in `Data\ConquerOps\impacts.txt`.
* **The terrain surface comes from `TheTerrainVisual->getTerrainTile(x, y)->getClass()`.** The
  research thought no runtime getter existed; the engine's own `BuildAssistant::checkSampleBuildLocation`
  (`GeneralsMD/…/Common/System/BuildAssistant.cpp:488-492`) calls exactly this from logic code, and it
  reads the map's tile indices (`WorldHeightMap::getTerrainNameAt`), not a render product. Water is
  a water table over the terrain, so `TerrainLogic::isUnderwater` answers it.
* The impact record is the *local* player's only (the `ClientOut` path), so other players' shots
  still play the converted `FireFX` at the victim. Fixing that is a separate carrier.
* `ZHBO_CLIENT_HITS` is 8: one shot per logic frame, and the converter now fails an install test if
  any gun's `pellets` exceeds it — bump both if that ever trips.
* The impact frame is X = `-dir` (the surface's outward normal). BO2's exact basis is unverified;
  a wrong one shows as a sideways puff, so this is a look-check item.
* Install findings that changed the plan: the launcher buckets (7, 9) belong to the *explosion*
  table (`explosion_entry`, played by the projectile's own blast), and the crossbow/ballistic
  knife's 14 is BO2's own `fx_missing_impact` row, so only the four bullet buckets
  (`bullet_entry`: 1-5) get impact cells. The shotgun guess (5) resolves to `impacts/fx_small_dirt`
  in `fx --impacts`, a real effect. Bucket 1's row ships no nonfatal blood cells (only the two
  fatal-exit ones), so a nonfatal pistol/SMG hit resolves no blood FX — that is BO2's data, not a
  lookup bug; the install test asserts one blood cell per bucket, not the nonfatal one.
* The impact-count log line is sampled on `frame % 30` and counts only that logic step's events
  (the client clears the buffer each frame), so watch a few sampled lines in a firing burst rather
  than expecting every second to print.

## ADS pose clips: the aimed pose is BO2's animation, not a slide (2026-10-24, `v1/ads-pose`)

* **R6's stand-in is gone on the native path.** `place::aligned` slid the aim marker onto the view
  axis by the ADS fraction. That is a *sight-point* solution, and it agrees with BO2 only when three
  coincidences hold (the marker is the animator's point, the eye relief does not change, the
  rotation is negligible) — i.e. almost nowhere: −6.28″ of eye relief wrong on the AS50, −11.48″ on
  the Ballista, +8.55″ on the M32, and a whole family of "the marker is the optic's 0.72 window, not
  the line". BO2's aimed pose is instead `weapAnimFiles_t` 0x55/0x56, a `tag_torso`-only layer on a
  second anim channel: the whole hip→aimed rig move (translation on all three axes plus the
  launchers' rotation) is authored data. The native path now samples it with
  `zhbo_native::pose::sample_over` over the action clip and keeps the slide only as the fallback
  (no 0x55/0x56 in the manifest — equipment, a partial conversion — or the clip still loading),
  which is also the W3D path's rule. The W3D path itself is untouched.
* **The scrub rule: `ads · num_frames` up, `(1 − ads) · num_frames` down**, `num_frames` being BO2's
  last frame index, so `ads = 1` lands the authored aimed key. Scrubbing (not "play at
  `length / adsTime`") because `arsenal.ads` moves linearly at `1/ads_in_time`, which makes the two
  coincide — and the scrub is stateless, so it cannot desync from the fraction, inherits quickdraw
  and the exit delay, and makes a reversal exact without a clock. The research's mp7 fingerprint
  (63 % of the move at half) *is* this function at N/2.
* **Reversal is switch-and-step, no cross-fade.** The twins make it continuous: `ads_down`'s first
  key equals `ads_up`'s last within 0.02″ on every weapon the install test probes, and the frame
  rule follows *which clip* ran (up is always `ads·N`, down always `(1−ads)·N`) so the one-clip
  fallback does not jump by `1−2·ads`. A per-weapon twin deviation is the only reason to add a blend,
  and it should be measured before it is coded.
* **The authored residuals are parity — do not "fix" them onto the crosshair.** The aimed tag is
  not always on the axis and that is BO2's data: the KSG's `tag_sights` 2.198″ low at 13.1″, the
  AS50's `tag_scope` 1.302″ below BO2's line (its clip is authored to a different reference), the
  launchers' heuristic marker 1.7–2.9″ off, the dual-wield pair deliberately zero-move. The
  acceptance test measures the authored tag against the research's table, never "on-axis", or it
  would re-introduce the bug this cycle removed. The FHJ-18 keeps a 2.32° authored cant after the
  overlay (its hip is 25.34° off; the SMAW goes 19.86° → 0.38°) — the research measured only the
  SMAW's end angles, so that number is recorded here, not treated as a failure.
* **The research's an94/mp7 table rows are the reflex compositions.** Taken literally against the
  bare ids they read (−0.003, +0.491) and (−0.002, +0.050): with no optic mounted, `tag_sights` is
  the gun's own iron tag. They reproduce exactly on `an94_mp+reflex` (0.0336, 0.0170) and
  `mp7_mp+reflex` (0.0328, −0.5611). The install test pins the reflex rows and says why; the other
  nine families reproduce on the bare id to the thousandth.
* **Measured sweep (14 weapons, hip|aimed, before/after, outside the repo).** `manifest.tsv` is
  byte-identical before/after — no converter, archive or VERSION change (the clips have been
  converted since R6; only the state machine never asked for them). The dual-wield pair's aimed half
  is now *pixel-identical* to its hip half (3 px of float noise) where 26.3 % of the pixels moved
  before: the pair no longer slides 5.5–6.0″. Changed aimed-half pixels: SMAW 43.1 % (tube
  straightened), M32 25.9 %, KSG 17.6 % (the authored 2.2″ residual now visible), an94 10.5 %,
  snipers 2.6–6.0 % (their eye relief is large but they were already far from the camera). AS50: the
  scope leaves the eye (its rig moves +6.28″ forward).
* **Two flagged decisions stand as planned:** `tag_scope`'s converter read (snipers) and the
  mounted-optic mount (`(identity, view_offset)` at `view_model_tag`, the MP7 reflex on
  `tag_brass`) are **not** in this cycle. The clip path never consumes `AIM_TAG` for placement, so
  reading `tag_scope` would change only the W3D fallback and the manifest marker; the optic still
  *draws* in the wrong place. They get their own cycle.
* **Ghidra follow-up (open).** The site that sets 0x55/0x56 (the caller of `FUN_0078b1a0`) is not
  disassembled: whether the engine time-scales the clip into `fOOPosAnimLength` or cross-fades the
  root pose by `fWeaponPosFrac`, and what blend time a direction flip passes, is still inferred.
  It does not block — scrubbing and rate-scaling coincide while the fraction is linear — but the
  theory is only closed by reading it. Recorded in `docs/CONTRACT.md` too.
* **Plan-snippet bugs the tests caught** (worth knowing for the next clip layer): the clip-choice
  match in the plan had its rising arms bound to the *down* path (a rise sampled `ads_down`
  reversed) — the unit suite caught it in one run; and the plan's `fixture` notes say the DW track
  is "constant (0,0,0)" when it is constant at the rig root's *bind local* ([-4.64, 0, −1.38] on
  the viewhands): what must be zero is the *move*, not the value.
* **No reversals are covered in game.** The default autotest aims (4.5–7.5 s) and fires aimed
  (5.0–7.0 s), and the swap at 7.2 s snaps ADS to 0 — but no leg releases and re-presses mid
  transition. Unit tests 2–3 (`the_reversal_switches_to_the_down_clip_at_the_same_fraction`,
  `a_twin_pair_reverses_without_a_step`) are the guard; a scripted reversal leg is an optional
  follow-up, not this cycle's acceptance.
## Camo swap: the class camo on the native viewmodel (2026-10-24, `v1/camo-swap`)

**The parser was the first defect.** `zhbo_formats::weapon::read_camo` read the set images
(`GfxImage*`) and the pair arrays (`Material**`) with the `char*` helper, so every set image came
back `None`, only pair element 0 survived, `shaderConsts` was unread and the fifteen style recipes
were flattened into one list. A synthetic-zone test now fails on those reads; the install-gated
`reads_the_an94_camo` asserts the AN-94's 40 pattern slots, 15 style recipes, gold's exact
base/camo names and `shaderConsts[0] == 10.0`.

**C0 (pre-flight probe).** The atacs set's art, decoded through `zhbo-assets image` from
`base.ipak` (the patch zone's copies are name-only stubs with `stream_hash == 0`):
`t6_camo_atacs_solid` is 32×32, mean RGB (86.7, 79.8, 69.6), alpha 41/46.5/47;
`t6_camo_atacs_pattern` is 256×256, mean RGB (90.5, 84.6, 71.5), alpha 0/52.5/110, and it tiles at
its edges (mean |Δ| 4.2 left↔right, 3.2 top↔bottom). The layer's alpha is coverage, not opaque, so
the mix rule (`albedo * mix(1, camo.rgb * gain, camo.a)`) is the right shape and `CAMO_GAIN = 2.0`
stays the default; 1.0 is the fallback if the capture comparison rejects it. `camo_off_solid` /
`camo_off_pattern` are not in `common_mp` via `find_image` (only the patch stubs exist); the set
images are the real art.

**The option→weapon mapping needed the view-model join.** BO2 names camo assets after the weapon's
view model — `camo_xpr50` is `as50_mp`, whose view model is `t6_wpn_sniper_xpr50_view`. The
converter now matches by weapon id first and then by a data-driven token join through the parsed
BO2 weapon's `view_model` (no alias table). On this install that resolves 38 of the 48 assets,
including `camo_xpr50`, `camo_x95l`, `camo_chicom`, `camo_msmc`, `camo_ballistic_knife` and
`camo_riot_shield`; `camo_b23r` (the B23R's model is `b2023r`), `camo_peacekeeper` and
`camo_specter` do not resolve because their weapons/models are outside `common_mp`'s parsed table.
The seven optic camo assets (`camo_reflex`, `camo_holo`, `camo_ir`, `camo_acog`, `camo_hybrid`,
`camo_mms`, `camo_targetfinder`) are attachment recipes, excluded by design; optic-camo handling is
a follow-up, consistent with the `Neutral` layer's deferral. The converter's summary lists the 38
assets, 39 pattern images, 72 style materials and the art-less options (`camo_multicam`,
`camo_matuka`; DLC `camo_mtp`/`camo_snake` are mapped but not in the base tab).

**The sweep found a load-order bug.** `nativevm::camo::read` requested the `.zcm` through
`Files::get` but `load` cached the stock mesh immediately, so the blob arrived a frame after the
mesh and the camo never applied (the "camo art is not in the archive" log was misleading). The read
now reports `waiting` and `load` returns `Ok(None)` until the blob arrives; the sweep's 9
`--camo` images then apply with no missing-art log.

**Style materials and D8.** The style camos are material replacements; their colour maps are
largely the shared `zhbo_camo_code_spc` (the gold pair is `mc/mtl_weapon_camo_gold` (unresolved on
this install) + `mc/mtl_weapon_camo_gold_1` → `zhbo_camo_code_spc.tga`), and the per-style
differences live in `shaderConsts`, which the blob carries verbatim but does not interpret (D8).
MP7's gold and diamond renders are therefore identical; AN-94's differ (its diamond pairs resolve
different art). This is a known residual until the style technique is ported.

**Divergences.** The camo is first-person native-viewmodel only: the W3D fallback bakes textures
and has no override, and the third-person native body pins `camo` to `INVALID`. Only the primary's
viewmodel inherits the class camo (the class stores one). The Ghidra query for the engine's
camo-id table and an A/B Multicam-vs-None capture remain recorded follow-ups; until then those
cells draw stock.

**Versions.** `zhbo_native::camo::VERSION = 1` (new blob). The model blob's material record gains
the semantic-22 camo layer; its version bump 2→3 and the archive bump 64→65 are integrator-owned
(this branch deliberately leaves `model::VERSION = 2` and `hud::VERSION = 64`).

## Hiding is per bone: mounted external hardware keeps drawing (2026-10-24, branch `v1/per-bone-hide`)

`pose::compose` propagated a hide tag down the whole subtree (`under_hidden`: a child of a hidden
bone was hidden too). That is not BO2's rule — the W3D path hides only the geometry skinned to the
bone named in `hideTags` (`zhbo_assets::character::skin_hiding`) — so a part mounted on a hidden
stub was being hidden with the stub. The composer now sets `Bone.hidden` from the name match alone.
Class A is unchanged: a hidden stub's own all-hidden surface still drops (its triangles are skinned
to the directly named bone), its weights still vanish and the gun receiver is untouched. The
restored population is class B: 130 surfaces over 40 jobs / 17 attachment models — mounted optics,
silencers, grips, launchers and lasers whose geometry the subtree collapse silently removed. The
invisible-attachments ten are a mounting/placement population and stay out of scope.

**Sweep acceptance.** Full render sweep before/after against the real installs (`ZHBO_BO2_DIR`,
475 jobs, 0 failed each): triangles **7,095,953 → 7,167,388 (+71,435)**, surfaces **6,152 → 6,282
(+130)**, **40 affected jobs**, and **zero base-only jobs changed** (the gun and every Class A
directly named surface keep their counts; no negative delta anywhere). The affected jobs' frames
change by 3–47,083 pixels in the mount regions. One combo, `mp7_mp+silencer`, composes its +126
triangles but its sweep frames are byte-identical — the added surface is occluded/internal from the
hip/ADS camera; its geometry is composed and would draw from another angle. The delta is exact to
the triangle.

**Plan fixture anchors.** The plan's `strip()` fixtures assumed a `tag_stub` bone that `strip()`
does not carry. The base's second bone is renamed `tag_stub` and the attached part's bones are
`tag_stub_mount`/`tag_stub_child`; mounting the part on its own first bone was self-referential.
The rest of the plan's tests apply verbatim. The module and `compose` docs now state the per-bone
rule.

**Verified.** `cargo test --workspace` green (546 passed; the ignored counts unchanged);
`cargo clippy --workspace --all-targets` exit 0 (no warnings in the touched files; the 48 warnings
elsewhere are pre-existing); `ZHBO_BO2_DIR=… cargo test -q --release -p zhbo-assets -p zhbo-formats
-- --ignored` green (27 + 9). No version/ABI bump: only the value the composer assigns to
`Bone.hidden` changes; the blob, ABI and `hud::VERSION` are untouched. The in-game autotest and the
Windows VM legs were not run (protocol: no game launch); the change is platform-neutral composer
logic.

