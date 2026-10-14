# Conquer Ops

A Command & Conquer Generals: Zero Hour mod that puts **you** on the battlefield. Build your General at
the Command Center, select them, press **Take Control**, and fight in first person with Black Ops II-style
movement, weapons, and HUD. Press **F** to pull out your field computer and the camera zooms back out to
command your army as usual; select the General and press Take Control to drop back in.

The General is an ordinary Zero Hour unit: enemies see, target, and shoot it like any other infantry.

The first-person HUD follows Black Ops II's multiplayer layout and uses Black Ops II's own art (ammo
numerals, red-dot reticle, damage overlay, lock-on bracket). It is read from your Black Ops II install
the first time you launch and saved privately next to the launcher as `ConquerOps_BO2.big`.

## Requirements

You must own and have installed **both** games. The launcher and the game itself refuse to start
otherwise.

* Command & Conquer Generals: Zero Hour (the Steam release is detected automatically)
* Call of Duty: Black Ops II (Steam)

No files from either game are shipped with this mod or written into the games' folders. Black Ops II is
never launched, hooked or connected to. The mod reads its sound banks and image packs, and decrypts
fastfiles (`zone/all/*.ff`) from your own install to convert BO2's 3D models and animations. That
only works with the game installed, and the converted files stay in your local Conquer Ops folder.

Set `ZHBO_ZH_DIR` / `ZHBO_BO2_DIR` to point at installs the detector does not find.

## Controls (first person)

| Action | Key |
|---|---|
| Move | W A S D |
| Look | Mouse |
| Fire / Aim down sights | Left / Right mouse |
| Sprint | Shift (hold) |
| Crouch / Prone | C / Ctrl |
| Jump | Space |
| Reload | R |
| Switch weapon (MTAR ⇄ B23R) | 1 or mouse wheel |
| Knife | V |
| Field computer (back to RTS) | F |
| Game menu | Esc |

## Platforms

The game is built natively from source on every platform; there is no Wine. On Windows there are two
builds: the 32-bit DX8 reference and the 64-bit SDL3 build on the mod's Rust wgpu renderer
([docs/RENDERER.md](docs/RENDERER.md)). Both are built and tested in game. macOS and Linux use the
same SDL3 + wgpu build. Their build path is written but has not been run yet; continuing on a Mac
starts at [docs/HANDOFF-MAC.md](docs/HANDOFF-MAC.md). See [docs/PLATFORMS.md](docs/PLATFORMS.md).

## Building

On Windows (on Linux/macOS see [docs/PLATFORMS.md](docs/PLATFORMS.md)), you need:

* Rust (`rustup`, plus `rustup target add i686-pc-windows-msvc x86_64-pc-windows-msvc`)
* Visual Studio 2022 Build Tools with: C++ x86/x64 tools, CMake, ATL, MFC, Windows 11 SDK
  (on ARM64 Windows also the ARM64 C++ tools)
* git

```
git clone --recursive <this repo>
cargo xtask build        # 32-bit DX8 build: Rust core + patched engine + ConquerOps.big + launcher
cargo xtask build --wgpu # 64-bit SDL3 build on the Rust wgpu renderer (downloads FFmpeg once)
cargo xtask run -- -win -xres 1280 -yres 720
```

`cargo xtask doctor` lists anything missing. `cargo xtask build` checks out the Zero Hour engine source
(bobtista's fork of TheSuperHackers GeneralsGameCode, at the commit pinned by
`host/GeneralsGameCode`) into `%LOCALAPPDATA%\ConquerOps\work`, applies
`host/patches/*.patch`, builds it with the Rust core linked in, and stages everything in
`%LOCALAPPDATA%\ConquerOps\dist`. Start the mod with `dist\ConquerOps.exe`.

Tests:
* `cargo test --workspace`: headless simulation, formats, detection, shader translation, and a
  headless renderer frame with pixel checks.
* `scripts/autotest.ps1`: starts a skirmish and verifies the full first-person loop in the real
  game.
* `scripts/inputtest.ps1`: real keyboard and mouse input in first person.

The two scripts take `-Dist` to test a given build.

## Layout

| Path | What |
|---|---|
| `crates/zhbo-core` | The Rust gameplay core: BO2 movement, weapons, hitscan, field computer, camera, HUD |
| `crates/zhbo-gfx` | The renderer: the engine's bgfx API subset on wgpu, plus the build-time shader translator |
| `crates/zhbo-requirements` | Finds and validates both game installs (Steam libraries, SteamCMD folders, Wine/CrossOver/Proton prefixes) |
| `crates/zhbo-launcher` | `ConquerOps.exe` / `conquerops`: requirement check, BO2 art, starting the game |
| `crates/zhbo-formats` | `.big` (Zero Hour); `.sabs`/`.sabl` sound banks, `.ipak` image packs, `IWi` images, LZO1X (Black Ops II) |
| `crates/zhbo-assets` | BO2 HUD art converter, plus a CLI to list/export your BO2 sounds and images |
| `host/zhbo` | C++ compiled into the engine: bridge, platform layer (SDL3/Win32), bgfx-on-wgpu adapter |
| `host/patches` | The engine hooks (~120 lines) and the `wgpu` renderer option for the engine's CMake |
| `data/` | Mod INI files, packed into `ConquerOps.big` |
| `docs/CONTRACT.md` | Engine ↔ Rust ownership, units, timing, lifecycle |
| `docs/RENDERER.md`, `docs/PLATFORMS.md` | The wgpu renderer; platforms and native builds |
| `docs/HANDOFF-MAC.md` | Where the work stands and how to continue it on macOS |
| `MODLOG.md` | Development journal and gotchas |

## Status and known limits

* The General and the first-person arms and AN-94 are BO2 models converted at launch from your
  install (`ConquerOps_BO2.big`). W3D skins take one bone per vertex, so joints bend a little less
  smoothly than in BO2, and the viewmodel can poke into walls or enemies standing very close.
  Only the AN-94 is converted so far.
* BO2's HUD font also lives in the fastfiles, so text uses Bahnschrift (Windows' DIN-style font),
  falling back to Arial. macOS asks for DIN Alternate, which has not been checked yet.
* Weapon sounds are Zero Hour's. `zhbo-assets` can export your BO2 sounds, but BO2 banks identify sounds
  by hash only, so picking which sound is the MTAR is a manual step.
* First-person state is not saved in save games. Multiplayer is designed to be lockstep-safe but untested.
* The engine build is not network-compatible with retail Zero Hour.

## License

GPL-3.0-or-later (see `LICENSE.md`), because the mod links into the GPL-licensed Zero Hour source
release. Command & Conquer and Call of Duty are trademarks of their owners; this project is not
affiliated with or endorsed by them. Built with AI assistance (Claude).
