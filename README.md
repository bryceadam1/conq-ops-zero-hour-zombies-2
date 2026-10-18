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

## The General in the overhead view

Until you take control, the General is an ordinary unit that fights with the loadout's rifle. Their
command bar has **Take Control** and a button for each weapon, like the Ranger's: the rifle, the
pistol and the lethal (only used when picked). Each shows the ammo left. Ammo is the General's own
and limited, in both views: what is fired from the overhead view is gone in first person, and the
weapon picked carries over both ways. An empty weapon's button is greyed out, and the General
switches to a weapon that still has ammo.

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
| Set off C4 | R twice quickly (or G with no C4 left in hand) |
| Switch weapon (AN-94 ⇄ Tac-45) | 1 or mouse wheel |
| Attachment mode (grenade launcher, select fire, second optic) | 3 |
| Knife | V |
| Lethal (frag: hold to cook; Claymore, Betty: plant; C4: throw) | G |
| Tactical (concussion; Trophy, Insertion: plant; Black Hat: hold on the target) | Q |
| Field computer (back to RTS) | F |
| Whole map (Zero Hour's radar, needs radar) | Tab (hold) |
| Game menu | Esc |

## Platforms

The game is built natively from source on every platform; there is no Wine. On Windows there are two
builds: the 32-bit DX8 reference and the 64-bit SDL3 build on the mod's Rust wgpu renderer
([docs/RENDERER.md](docs/RENDERER.md)). Both are built and tested in game. macOS and Linux use the
same SDL3 + wgpu build. See [docs/PLATFORMS.md](docs/PLATFORMS.md).

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
| `docs/TESTING.md` | Every test layer and how to run it |
| `AGENTS.md` | How to work in this repo: ground rules, read order, conventions |
| `MODLOG.md` | Development journal and gotchas |

## Status and known limits

* The General and the first-person arms and weapons are BO2 models converted at launch from your
  install (`ConquerOps_BO2.big`). W3D skins take one bone per vertex, so joints bend a little less
  smoothly than in BO2, and the viewmodel can poke into walls or enemies standing very close.
* Weapons are BO2's own: every multiplayer weapon's definition (fire mode and rate, damage curve,
  spread, kick, reload/raise/ADS timings, grenade fuses and blasts), every attachment's numbers, the
  and alt modes they add are converted into `Data\ConquerOps\weapons.ini`.
  The General carries an AN-94 with a Reflex sight and Grenade Launcher, the Tac-45, a frag, a
  concussion and the knife; Lightweight, Fast Hands, Dexterity.
  Every weapon's art is converted (about 420 MB; `ZHBO_CONVERT_ALL_WEAPONS=0` converts only the
  loadout's). `ZHBO_LOADOUT` tries another kit, e.g.
  `lethal=claymore_mp;tactical=trophy_system_mp;primary=dsr50_mp+acog` (every
  player in a game must use the same one).
* Equipment works as in BO2: Claymores fire at enemies in front, Bouncing Betties hop, C4 waits for
  the detonator, the Shock Charge stuns, the Trophy System shoots down grenades, rockets and Zero
  Hour's missiles, the Sensor Grenade shows enemies on the minimap and finds stealthed units, the
  Tactical Insertion is where the next General comes in, and the Black Hat hacks enemy equipment
  and Zero Hour drones. Enemies can shoot equipment.
* BO2's scorestreaks are left out on purpose: the Generals' own upgrades and superweapons take
  their place. (The field computer still uses BO2's scorestreak tablet and HUD box.)
* Explosions use BO2's own effects (its impact table's grenade and rocket explosions, flashbang,
  equipment blasts), converted into Zero Hour particle systems; BO2's flipbook animation, models and
  decals in them are not converted.
* BO2's HUD font also lives in the fastfiles, so text uses Bahnschrift (Windows' DIN-style font),
  falling back to Arial. macOS asks for DIN Alternate, which has not been checked yet.
* First person sounds like BO2: gunshots, reload foley, hit markers, footsteps and the low-health
  heartbeat are converted from your install. Weapon metal and gear reflect a generated environment
  (weighted by BO2's specular maps), not BO2's own per-map reflection probes.
* First-person state is not saved in save games. Multiplayer is designed to be lockstep-safe but untested.
* The engine build is not network-compatible with retail Zero Hour.

## License

GPL-3.0-or-later (see `LICENSE.md`), because the mod links into the GPL-licensed Zero Hour source
release. Command & Conquer and Call of Duty are trademarks of their owners; this project is not
affiliated with or endorsed by them. Built with AI assistance (Claude).
