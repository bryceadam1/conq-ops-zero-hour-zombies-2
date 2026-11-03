# Platforms: Windows, macOS, Linux

Conquer Ops builds the Zero Hour engine from source, so it runs **natively** everywhere, with no
Wine. It builds on bobtista's fork of the SuperHackers source (SDL3, OpenAL, 64-bit). Rendering
goes through the mod's own wgpu renderer (`crates/zhbo-gfx`, see [RENDERER.md](RENDERER.md)), so
the same renderer drives Metal on macOS, Vulkan or GL on Linux, and D3D12 or GL on Windows. The
games' data files come from their Windows releases. On macOS and Linux, those are the Windows
depots fetched with SteamCMD.

| | Windows | macOS | Linux |
|---|---|---|---|
| Game executable | `generalszh.exe`: 32-bit DX8 (`cargo xtask build`) or 64-bit SDL3 + wgpu (`--wgpu`) | `generalszh`, SDL3 + wgpu on Metal | `generalszh`, SDL3 + wgpu on Vulkan/GL |
| Launcher | `ConquerOps.exe` | `conquerops` | `conquerops` |
| Build | Visual Studio 2022 | Xcode CLT + Homebrew (`cargo xtask build`) | gcc/clang + distro packages (`cargo xtask build`) |
| Status | both builds pass the in-game autotest and real-input test | built and passing the in-game autotest (`scripts/autotest-mac.sh`; `--input` for real input) | build path written, not yet built |

## Where the games are found

`crates/zhbo-requirements` searches, in order:

1. `ZHBO_ZH_DIR` / `ZHBO_BO2_DIR`, the explicit override. Use these for SteamCMD's
   `force_install_dir` folders.
2. Steam libraries, via each Steam client's `libraryfolders.vdf` and `appmanifest_<appid>.acf`
   (Zero Hour: 2732960; Black Ops II: 202970, 202990, 212910):
   * Windows: registry `SteamPath`/`InstallPath`, `C:\Program Files (x86)\Steam`.
   * macOS: `~/Library/Application Support/Steam`.
   * Linux: `~/.steam/steam`, `~/.steam/root`, `~/.local/share/Steam`, Flatpak, Snap.
   * Windows Steam clients inside Wine/CrossOver/Whisky/Proton prefixes. Games there are ordinary
     folders the native build can read, and their library paths are mapped back to host paths.
3. Windows only: the EA registry key.

Zero Hour's Steam release includes the original Generals data (`ZH_Generals\`). The engine finds it
by itself: it checks `CNC_GENERALS_INSTALLPATH`, then the registry, then searches relative to the
working directory. The launcher's working directory is the Zero Hour folder.

## Getting the Windows game files on macOS/Linux

Steam's own client installs the macOS/Linux builds where they exist, and BO2 has none. SteamCMD can
fetch the Windows depots directly. The account must own both games:

```
steamcmd +@sSteamCmdForcePlatformType windows +force_install_dir ~/Games/ZeroHour \
         +login <steam-user> +app_update 2732960 validate +quit
steamcmd +@sSteamCmdForcePlatformType windows +force_install_dir ~/Games/BlackOps2 \
         +login <steam-user> +app_update 202970 validate +quit
export ZHBO_ZH_DIR=~/Games/ZeroHour ZHBO_BO2_DIR=~/Games/BlackOps2
```

The mod reads only Black Ops II's sound banks and image packs. BO2 is never run, and the mod never
connects to it. The step-by-step player instructions are in [INSTALL.md](INSTALL.md); a guided
installer script that wraps this is still to do.

## Graphics detail on every platform

Zero Hour's detail detector (`GameLODPresets.ini`) recognises only 2002's CPUs (P3, P4, K7), so on
every build here (Windows DX8 and wgpu, macOS on Apple silicon or Intel, Linux) it found no match and
recommended **Low**: 500 particles, half-resolution textures, no shadows. Host patch 0025 and the
mod's `UnclassifiedHardwareLOD = High` make **High** the default instead; the reason, and how a level
the player chose is kept, are in [CONFIG.md](CONFIG.md), "Graphics detail". The Options menu still
sets Low, Medium, High, Very High or Custom (Very High is SuperHackers' level: 5,000 particles).

## The render rate on every platform

Zero Hour rendered at 30 fps, its logic rate. Host patch 0026 renders at the display's refresh rate
(at least 60) on every build, the logic staying at 30; `MaxRenderFPS` in `Options.ini` sets another
cap (CONFIG.md, "The render rate"). Vsync stays off: the engine's limiter paces the frames, so the
DX8 build (whose swap interval only applies in fullscreen) behaves like the wgpu builds.

## How the launcher starts the game

`conquerops` (or `ConquerOps.exe`):
1. Checks that both games are installed.
2. Builds `ConquerOps_BO2.big` from BO2's art if needed.
3. Starts `generalszh` from its own folder with the Zero Hour install as the working directory,
   `-useCwd -mod <its folder>`, and `ZHBO_ZH_DIR`/`ZHBO_BO2_DIR` set.

On Linux it adds `libs/` (staged by the engine's deploy script) to `LD_LIBRARY_PATH`. Run
`conquerops --check` to print what was found and the exact command, without starting anything.

## Building on macOS and Linux

```
# macOS
xcode-select --install
brew install cmake ninja pkgconf ffmpeg dylibbundler
# Debian/Ubuntu (plus the engine fork's SDL3/X11/Wayland/audio dev packages, see
# its .github/workflows/bgfx-builds.yml "linux" job)
sudo apt install build-essential cmake ninja-build pkg-config \
     libavcodec-dev libavformat-dev libavutil-dev libswscale-dev

cargo xtask doctor    # lists anything still missing
cargo xtask build     # core (host) + zhbo-shaderc + engine (fork's preset, wgpu) + launcher
```

`cargo xtask build` on macOS/Linux:
1. Builds `zhbo-core` for the host with `--features renderer`, and builds `zhbo-shaderc`.
2. Configures the engine with the fork's `macos-generalsmd-sdl3-bgfx` or
   `linux-generalsmd-sdl3-bgfx` preset, plus `-DGGC_BGFX_RENDERER=wgpu -DZHBO_SHADERC=...`.
3. Builds `z_generals`.
4. Stages the binary and its libraries with the fork's `scripts/build/<os>/deploy-*.sh` into the
   work dir, then copies them to dist.

The work dir is `~/Library/Application Support/ConquerOps/work` or `~/.local/share/ConquerOps/work`;
dist is the `dist` folder next to it.

## Tests

* `cargo test --workspace`: simulation, formats, discovery, shader translation, and a headless
  renderer frame with pixel checks. The renderer test uses whatever GPU or software adapter wgpu
  finds and skips when there is none.
* `scripts/autotest.ps1` (scripted first-person loop) and `scripts/inputtest.ps1` (real keyboard and
  mouse) are Windows-only, because they drive the game with `um win`. On macOS,
  `scripts/autotest-mac.sh` runs the same scripted loop (plus `--input` for real keyboard/mouse);
  see [TESTING.md](TESTING.md) for the full ladder.

### macOS troubleshooting

Gotchas that cost time when the macOS build was first brought up (from the old handoff notes):

* If surface creation fails, make sure `GGC_MACOS_USE_NSWINDOW` is unset — the backend must
  arrive as SDL's `CAMetalLayer`, which the adapter turns into a wgpu `CoreAnimationLayer`
  surface (`host/zhbo/bgfx_wgpu.cpp`, surface kind 2).
* On retina displays the backend's width and height must be pixels rather than points — a blurry
  image at half resolution or HUD offsets point there (`reset()` in
  `crates/zhbo-gfx/src/runtime/mod.rs`, the backend's `LbSwapWidth()`).
* Mouse look uses SDL relative mouse mode: the cursor must hide and stay confined in first
  person and be released by the field computer (F).
* `ZhboPlatform.cpp` asks for the `DIN Alternate` font on macOS; if text falls back badly, check
  how the fork resolves fonts there.
* Zero Hour's data search: the engine tries `CNC_GENERALS_INSTALLPATH`, then paths relative to
  the working directory (the Zero Hour folder). If base-game assets are missing, set
  `CNC_GENERALS_INSTALLPATH=$ZHBO_ZH_DIR/ZH_Generals/`. The packaging step (`cargo xtask build`)
  needs `dylibbundler` and sets `GGC_MACOS_RUNTIME_DIR` on the staged binary.
