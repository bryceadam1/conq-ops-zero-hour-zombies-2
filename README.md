# Conquer Ops

A Command & Conquer Generals: Zero Hour mod that puts **you** on the battlefield. Build your General at
the Command Center, select them, press **Take Control**, and fight in first person with Black Ops II-style
movement, weapons, and HUD. Press **F** to pull out your field computer and the camera zooms back out to
command your army as usual; select the General and press Take Control to drop back in.

The General is an ordinary Zero Hour unit: enemies see, target, and shoot it like any other infantry.

## Requirements

You must own and have installed **both** games. The launcher and the game itself refuse to start
otherwise.

* Command & Conquer Generals: Zero Hour (the Steam release is detected automatically)
* Call of Duty: Black Ops II (Steam)

No files from either game are shipped with or written by this mod. Black Ops II is never launched; the
mod only reads files from your install (see `zhbo-assets`).

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

## Building

Windows only (the game is 32-bit x86). Needs:

* Rust (`rustup`, plus `rustup target add i686-pc-windows-msvc`)
* Visual Studio 2022 Build Tools with: C++ x86/x64 tools, CMake, ATL, MFC, Windows 11 SDK
  (on ARM64 Windows also the ARM64 C++ tools)
* git

```
git clone --recursive <this repo>
cargo xtask build        # Rust core + patched engine + ConquerOps.big + launcher
cargo xtask run -- -win -xres 1280 -yres 720
```

`cargo xtask build` checks out the Zero Hour engine source (TheSuperHackers GeneralsGameCode, the commit
pinned by `host/GeneralsGameCode`) into `%LOCALAPPDATA%\ConquerOps\work`, applies
`host/patches/*.patch`, builds it with the Rust core linked in, and stages everything in
`%LOCALAPPDATA%\ConquerOps\dist`. Start the mod with `dist\ConquerOps.exe`.

Tests: `cargo test --workspace` (headless simulation, formats, detection) and
`scripts/autotest.ps1` (starts a skirmish and verifies the full first-person loop in the real game).

## Layout

| Path | What |
|---|---|
| `crates/zhbo-core` | The Rust gameplay core: BO2 movement, weapons, hitscan, field computer, camera, HUD |
| `crates/zhbo-requirements` | Detects and validates both game installs |
| `crates/zhbo-launcher` | `ConquerOps.exe` |
| `crates/zhbo-formats` | `.big` archives (Zero Hour), `.sabs`/`.sabl` sound banks (Black Ops II) |
| `crates/zhbo-assets` | Lists/exports BO2 sound bank entries from your install to WAV |
| `host/zhbo` | C++ bridge compiled into the engine |
| `host/patches` | The engine hooks (~120 lines) |
| `data/` | Mod INI files, packed into `ConquerOps.big` |
| `docs/CONTRACT.md` | Engine ↔ Rust ownership, units, timing, lifecycle |
| `MODLOG.md` | Development journal and gotchas |

## Status and known limits

* No weapon viewmodel yet: you see the HUD and crosshair, not a gun in your hands.
* Weapon sounds are Zero Hour's. `zhbo-assets` can export your BO2 sounds, but BO2 banks identify sounds
  by hash only (names are in encrypted fastfiles, which this mod does not decrypt), so picking which
  sound is the MTAR is a manual step.
* First-person state is not saved in save games. Multiplayer is designed to be lockstep-safe but untested.
* The engine build is not network-compatible with retail Zero Hour.

## License

GPL-3.0-or-later (see `LICENSE.md`), because the mod links into the GPL-licensed Zero Hour source
release. Command & Conquer and Call of Duty are trademarks of their owners; this project is not
affiliated with or endorsed by them. Built with AI assistance (Claude).
