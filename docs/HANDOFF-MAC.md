# Handoff: continuing Conquer Ops on macOS

This page lets the work continue on a Mac without the conversation it came from. It records where
things stand, what to do first, what is likely to break, and the agreements the project runs on.
Read this first, then [PLATFORMS.md](PLATFORMS.md), [RENDERER.md](RENDERER.md) and the gotchas in
[../MODLOG.md](../MODLOG.md).

## Where things stand (2026-10-14, commit after `ca969f1`)

Everything below was built and run on Windows 11 ARM64, in Parallels on an M1 Pro. Results:

* **The 32-bit DX8 build** (`cargo xtask build`) is the reference. It passes the in-game autotest
  (7/7) and the real-input test.
* **The 64-bit SDL3 build on the Rust wgpu renderer** (`cargo xtask build --wgpu`) runs the whole
  game: intro video, shell map, menus, skirmish, RTS view and first person, matching the DX8
  screenshots. It also passes the autotest (7/7) and the real-input test (SDL3 keys and relative
  mouse). In the VM it runs on Parallels' GL driver. That driver lacks BC textures and float render
  targets, so the renderer falls back to CPU DXT decoding and RGBA8 depth targets. On the Mac, Metal
  supports both natively.
* **macOS/Linux build path: written, never run.** It covers `cargo xtask build` (native toolchain,
  the fork's macOS/Linux presets, deploy scripts), the bridge's SDL3 platform layer
  (`host/zhbo/ZhboPlatform.cpp`, verified on Windows) and per-OS link libraries in
  `host/zhbo/CMakeLists.txt`. The Rust core with the renderer compiles for `aarch64-apple-darwin`
  and `x86_64-unknown-linux-gnu`.
* `cargo test --workspace`: 73 tests pass. These include a headless renderer test that draws a
  frame and checks its pixels.

Roadmap position: M0 (fork re-base) through M4 (3D world) are done. M5 (render targets, shadows,
water, captures) works in game but has not been compared closely against DX8. M6 (native
macOS/Linux plus installers) is next, and that is this handoff.

## 1. Set up the Mac

```
xcode-select --install
brew install cmake ninja pkgconf ffmpeg dylibbundler steamcmd   # steamcmd: or Valve's tarball
curl https://sh.rustup.rs -sSf | sh                             # stable Rust
git clone --recursive <this repo> && cd <repo>
cargo xtask doctor
```

The engine's macOS preset targets macOS 15 or later (`CMAKE_OSX_DEPLOYMENT_TARGET`).

## 2. Get the games' Windows files

Steam on the Mac will not install BO2, which has no macOS build, so fetch the Windows depots with
SteamCMD. The account must own both games:

```
steamcmd +@sSteamCmdForcePlatformType windows +force_install_dir ~/Games/ZeroHour \
         +login <steam-user> +app_update 2732960 validate +quit
steamcmd +@sSteamCmdForcePlatformType windows +force_install_dir ~/Games/BlackOps2 \
         +login <steam-user> +app_update 202970 validate +quit
export ZHBO_ZH_DIR=~/Games/ZeroHour ZHBO_BO2_DIR=~/Games/BlackOps2
```

Zero Hour's depot includes base Generals (`ZH_Generals/`). The engine fork's own guide,
`host/GeneralsGameCode/docs/BUILD/GETTING_THE_GAME_FILES.md`, describes other ways to get the
files. BO2 is large; the mod only reads
`sound/*.sabs|*.sabl` and `zone/all/*.ipak` from it. **Rule: BO2 is never launched, hooked or
connected to.**

## 3. Build

```
cargo xtask build --log      # --log adds the engine's DebugLogFile.txt logging
```

The engine work tree and build live in `~/Library/Application Support/ConquerOps/work`. The staged
game is in `.../ConquerOps/dist`, holding `conquerops`, `generalszh`, dylibs, `ConquerOps.big` and
`ConquerOps_BO2.big`.

### Likely first-build problems, in the order you will meet them

1. **Engine configure** (`cmake --preset macos-generalsmd-sdl3-bgfx -DGGC_BGFX_RENDERER=wgpu ...`).
   `host/patches/0002-wgpu-renderer.patch` adds the `wgpu` renderer value. If a hunk fails, the
   fork's CMake moved: compare against `cmake/render-backend.cmake`, `cmake/bgfx.cmake` and
   `Core/Libraries/Source/WWVegas/WW3D2/CMakeLists.txt`. The scratch tree used to make the patches
   was `C:\zhbo\bobwork`; on the Mac, make one with
   `git -C ~/Library/.../work/GeneralsGameCode diff`.
2. **Compiling `host/zhbo`.** `ZhboPlatform.cpp` declares `extern SDL_Window *TheSDL3Window` (from
   the fork's `SDL3GameEngine.cpp`). `ZhboBridge.cpp` must not use any Win32 type outside
   `#if defined(_WIN32)`. All Win32 calls were moved to `ZhboPlatform.cpp`, but the engine's
   force-included precompiled headers may hide anything left over on Windows.
3. **Linking.** The Rust core's macOS system libraries are listed in `host/zhbo/CMakeLists.txt`
   (from `cargo rustc ... -- --print native-static-libs`). If symbols from `libzhbo_core.a` are
   missing (`_zhgfx_*`), check that the core was built with `--features renderer`; xtask does this
   on macOS.
4. **Deploy.** xtask runs the fork's `scripts/build/macos/deploy-macos-generalsmd.sh` with
   `GGC_MACOS_RUNTIME_DIR=<work>/stage`, then copies the stage into dist. It needs `dylibbundler`.

## 4. Run and look

```
dist/conquerops --check                      # what was found, and the exact command
dist/conquerops -win -xres 1280 -yres 720
```

Logs:
* `dist/zhbo-gfx.log`: the renderer. It should say `adapter <Apple GPU> (Metal, ...)`, should
  **not** mention CPU BC decoding or R32F substitutes, and should list no `wgpu error` lines.
* `ConquerOps.log` in the engine's user-data folder: the mod's oracle (lifecycle, kills, a pose
  line every second). Find the folder with `find ~ -name ConquerOps.log`.
* `DebugLogFile.txt` (with `--log`): look for the `[BgfxBackend] bgfx::init OK ... renderer=` line.

### Likely runtime problems

* **Surface**: on macOS the backend passes SDL's `CAMetalLayer` (`TheSDL3MetalLayer`) as the window
  handle, and the adapter uses wgpu's `CoreAnimationLayer` surface (kind 2 in
  `host/zhbo/bgfx_wgpu.cpp`). If `GGC_MACOS_USE_NSWINDOW` is set, an `NSWindow` arrives instead and
  surface creation fails. Leave it unset.
* **Retina**: check that the backend's width and height are pixels rather than points. A blurry
  image at half resolution, or HUD offsets, point there. See `reset()` in
  `crates/zhbo-gfx/src/runtime/mod.rs` and the backend's `LbSwapWidth()`.
* **HUD font**: `ZhboPlatform.cpp` asks for `DIN Alternate` on macOS (marked TODO). If text falls
  back badly, check how the fork renders fonts on macOS and pick a face it resolves.
* **Mouse look**: uses SDL relative mouse mode. Verified on Windows; on macOS check that the cursor
  hides and is confined in first person, and is released by the field computer (F).
* **Base Generals data**: the engine searches `CNC_GENERALS_INSTALLPATH`, then relative paths from
  the working directory, which is the Zero Hour folder. If base-game assets are missing, set
  `CNC_GENERALS_INSTALLPATH=$ZHBO_ZH_DIR/ZH_Generals/`.

## 5. Verify like the Windows runs did

Do the same checks as `scripts/autotest.ps1`, by hand or with new tooling:
1. Start a skirmish with `ZHBO_AUTOPILOT=1`. The autopilot spawns a General, takes first-person
   control, walks, fires, opens the field computer and re-enters. `ConquerOps.log` should contain:
   "entered first-person control", "pos (", "killed a target", "opened the field computer",
   "returned General N to RTS control", "taking control again", "General took N damage".
2. `ZHBO_AUTOPILOT=manual` sets up the same scene but leaves input to you. Hold W and move the
   mouse; the pose lines should show position and yaw changing. That is what
   `scripts/inputtest.ps1` checks.
3. Compare screenshots of the RTS view, first person and the shell map against the DX8 build.

**Open task:** a macOS version of the test scripts. `um win` is Windows-only. Options: drive the menus
with AppleScript/`cliclick`; add an engine flag that starts a skirmish directly; and take captures
through the renderer's `requestScreenShot` path, which writes PNGs through the backend callback.

## 6. What still needs the Windows VM

* The DX8 reference build, for side-by-side visual comparison.
* `scripts/autotest.ps1` and `scripts/inputtest.ps1` regression runs, until a macOS equivalent exists.
* Regressions in the 64-bit Windows wgpu build.

## 7. Next tasks after the Mac build runs

1. Get the macOS build running and passing the autopilot checks (sections 3–5). Record new gotchas
   in MODLOG.md.
2. A macOS test driver (section 5).
3. M5 parity: shadows, water, bloom, SSAO and smudge against DX8. Measure with the renderer's
   screenshot path, and check `zhbo-gfx.log` for skipped draws.
4. Installers: a script wrapping the SteamCMD steps above, plus macOS packaging. Unsigned local
   builds run as-is; distributing builds needs signing or notarization notes.
5. Linux: the same path with the `linux-generalsmd-sdl3-bgfx` preset. GNU ld may need the Rust core
   and `zhbo_bgfx_wgpu` grouped, because of link order.
6. CI: `.github/workflows/ci.yml` still has untested macOS/Linux jobs; replace them with real native
   builds once one has succeeded locally.
7. Gameplay backlog: weapon viewmodel (BO2 models are in encrypted fastfiles, which we do not
   decrypt), BO2 audio mapping, saving first-person state.

## Working agreements (from the original conversation)

* **No piracy.** The mod refuses to run unless both Zero Hour and Black Ops II are installed. It
  ships no game files; assets are converted from the player's own installs at run time.
* **Never bypass anti-cheat or DRM. Never launch, hook or connect to BO2.** Only its files are read.
* **Rust wherever possible.** C++ is limited to the engine hooks (`host/patches`), the bridge and
  the bgfx adapter.
* **Native builds, no Wine.** On macOS and Linux the install fetches the Windows depots for their
  data.
* **Renderer: wgpu directly.** No bgfx library, no MoltenVK detour.
* **Commits** end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. On the Windows VM,
  git had no identity configured, so commits passed
  `-c user.name=unknown -c user.email=bryceadam1@gmail.com`.

## Map of the code

| Path | What |
|---|---|
| `crates/zhbo-core` | Gameplay core (BO2 movement, weapons, hitscan, camera, HUD) and its C ABI; `renderer` feature links zhbo-gfx |
| `crates/zhbo-gfx` | wgpu renderer: `shader.rs`/`blob.rs` (build-time .sc → WGSL), `state.rs` (bgfx state bits), `runtime/` (views, draws, resources, frame), `ffi.rs` (`zhgfx_*`), `bin/zhbo-shaderc.rs` |
| `host/zhbo/bgfx_wgpu.cpp` | Defines the `bgfx::` API the engine's BgfxBackend calls, on `zhgfx_*` |
| `host/zhbo/ZhboBridge.cpp` | Engine-side glue: input, camera, HUD drawing, autopilot |
| `host/zhbo/ZhboPlatform.cpp` | OS services (SDL3 or Win32): keys, mouse capture, focus, timer, message box, font |
| `host/patches/0001-*.patch` | Engine hooks (~120 lines) |
| `host/patches/0002-*.patch` | `GGC_BGFX_RENDERER=wgpu` in the engine's CMake |
| `xtask/src/main.rs` | Build orchestration for all platforms |
| `scripts/autotest.ps1`, `scripts/inputtest.ps1` | Windows in-game tests |
| `docs/CONTRACT.md` | Engine ↔ Rust ownership, units, timing |
