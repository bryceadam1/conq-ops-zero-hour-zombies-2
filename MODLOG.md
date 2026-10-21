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

## Next steps

The macOS port is built and passing its in-game autotest (`scripts/autotest-mac.sh`); setup and
macOS-specific gotchas live in [docs/PLATFORMS.md](docs/PLATFORMS.md). Start from `AGENTS.md` and
the docs read order there. The list below is the older gameplay backlog.

1. Manual playtest by a human with real mouse/keyboard (the autotest injects input inside the mod).
2. More BO2 weapons (only the AN-94 viewmodel and the B23R sound are converted).
3. BO2's own reflection probes: keep `GfxImageLoadDef` data when the zone loader reads it.
4. Save/load of first-person state; two-client multiplayer test.
5. Strike Force (`so_rts_*`) content as a natural next theme.
