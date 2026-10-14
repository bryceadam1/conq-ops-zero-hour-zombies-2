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
11. BO2 `.sabs` entries are `{id, size, offset, frames, rate_idx, channels, loop, format}`; format 8 payloads
    are plain FLAC. Names are hashes only → need alias tables from fastfiles (next step).

## Verification log

* Baseline source build (no mod) runs against Steam data with `-useCwd -win`: intro video plays (screenshot).
* `cargo test --workspace`: 41 tests (movement, weapons, hitscan, sim, client, HUD, formats, requirements).
* Requirements check on this machine: ZH found via Steam app 2732960, BO2 via Steam app 202970.
* Patched engine + Rust core: compiles and links (first try).

## Next steps

1. In-game verification: skirmish → build General → Take Control → walk/shoot → F → RTS → back.
2. BO2 assets: resolve `.sabs` name hashes (alias tables in fastfiles) → convert MTAR/B23R/knife sounds
   with a converter run on the player's install; weapon viewmodels from xmodels (fastfile parsing).
3. Save/load of first-person state; multiplayer test with two clients.
