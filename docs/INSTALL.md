# Installing Conquer Ops

Conquer Ops is built from source on your own machine and plays on top of your own copies of the two
games. Nothing from either game is downloaded or shipped by the mod.

The steps are the same everywhere:

1. Own and install **Command & Conquer Generals: Zero Hour** and **Call of Duty: Black Ops II**.
2. Install the build tools, then build the mod with `cargo xtask build`.
3. Start the launcher. The first start converts Black Ops II's art from your install, which takes a
   while; later starts are quick.

| | Windows | macOS | Linux |
|---|---|---|---|
| Status | Supported; tested in game | Supported; tested in game | Build path written, **not yet built or tested** |
| Getting the games | The normal Steam client | SteamCMD (the games' Windows files) | SteamCMD (the games' Windows files) |
| Launcher after building | `%LOCALAPPDATA%\ConquerOps\dist\ConquerOps.exe` | `~/Library/Application Support/ConquerOps/dist/conquerops` | `~/.local/share/ConquerOps/dist/conquerops` |

Jump to: [Windows](#windows) · [macOS](#macos) · [Linux](#linux) · [Updating](#updating) ·
[Multiplayer](#multiplayer) · [Troubleshooting](#troubleshooting)

## What you need from Steam

The Steam account must own both games. The mod reads Black Ops II's files but never starts it,
hooks it or connects to it. Black Ops II's multiplayer data is what the mod converts, so install
the multiplayer app as well as the main game:

| Game | Steam app ids |
|---|---|
| Command & Conquer Generals: Zero Hour | 2732960 (includes the original Generals data) |
| Call of Duty: Black Ops II | 202970 (the game) and 202990 (Multiplayer) |

Let Steam finish updating both games before you build. Every player in a multiplayer game needs
the same Black Ops II patch level, and Steam keeps it on the final patch.

## Windows

1. **Install the games.** In the Steam client, install Zero Hour and Black Ops II, including Black
   Ops II's Multiplayer. Their default Steam locations are found automatically.
2. **Install the build tools:**
   * [git](https://git-scm.com/download/win).
   * Rust from [rustup.rs](https://rustup.rs), then:
     ```
     rustup target add i686-pc-windows-msvc x86_64-pc-windows-msvc
     ```
   * Visual Studio 2022 Build Tools with these components: C++ x86/x64 build tools, C++ CMake
     tools, C++ ATL, C++ MFC, and a Windows 11 SDK. On an ARM64 PC, also the ARM64 C++ tools.
3. **Get the source and build** (from a "Developer PowerShell for VS 2022" or any terminal):
   ```
   git clone --recursive https://github.com/bryceadam1/conq-ops-zero-hour-zombies-2.git
   cd conq-ops-zero-hour-zombies-2
   cargo xtask doctor
   cargo xtask build --wgpu
   ```
   `cargo xtask doctor` lists anything still missing. `--wgpu` builds the 64-bit game on the mod's
   own renderer, which is the full experience (it downloads FFmpeg once). Plain
   `cargo xtask build` builds the 32-bit DX8 reference instead, which lacks the renderer-only
   effects such as the native weapon models' lighting and the shellshock blur.
4. **Play.** Start `%LOCALAPPDATA%\ConquerOps\dist\ConquerOps.exe`, or run
   `cargo xtask run --wgpu`.

## macOS

Black Ops II and Zero Hour have no macOS releases on Steam, so you fetch their Windows files with
SteamCMD. The mod's game build is native (Apple silicon or Intel); nothing runs under Wine.

1. **Install the build tools:**
   ```
   xcode-select --install
   brew install cmake ninja pkgconf ffmpeg dylibbundler steamcmd
   curl https://sh.rustup.rs -sSf | sh
   ```
   (Install [Homebrew](https://brew.sh) first if you don't have it.)
2. **Download the games' Windows files** into folders of your choice. SteamCMD asks for your
   password and Steam Guard code on the first login.
   ```
   steamcmd +@sSteamCmdForcePlatformType windows +force_install_dir ~/Games/ZeroHour \
            +login <steam-user> +app_update 2732960 validate +quit
   steamcmd +@sSteamCmdForcePlatformType windows +force_install_dir ~/Games/BlackOps2 \
            +login <steam-user> +app_update 202970 validate +app_update 202990 validate +quit
   ```
   Both Black Ops II apps go into the same folder.
3. **Tell the mod where they are.** SteamCMD folders are not Steam libraries, so set these in
   your shell profile (`~/.zshrc`):
   ```
   export ZHBO_ZH_DIR=~/Games/ZeroHour
   export ZHBO_BO2_DIR=~/Games/BlackOps2
   ```
   Open a new terminal afterwards so they take effect.
4. **Get the source and build:**
   ```
   git clone --recursive https://github.com/bryceadam1/conq-ops-zero-hour-zombies-2.git
   cd conq-ops-zero-hour-zombies-2
   cargo xtask doctor
   cargo xtask build
   ```
5. **Play** from a terminal, so the launcher sees the two variables:
   ```
   ~/Library/Application\ Support/ConquerOps/dist/conquerops
   ```
   `conquerops --check` prints what it found and the command it would run, without starting the
   game.

## Linux

The Linux build path is written but has **not been built or tested yet**. Expect to fix things;
please report what you hit.

1. **Install the build tools** (Debian/Ubuntu shown):
   ```
   sudo apt install git build-essential cmake ninja-build pkg-config \
        libavcodec-dev libavformat-dev libavutil-dev libswscale-dev
   curl https://sh.rustup.rs -sSf | sh
   ```
   The engine also needs SDL3's X11/Wayland/audio development packages; the list is in the engine
   fork's `host/GeneralsGameCode/.github/workflows/bgfx-builds.yml`, "linux" job. Install
   SteamCMD from your distribution or from Valve.
2. **Download the games' Windows files**, exactly as in [macOS step 2](#macos).
   * Instead, if your Steam client already installed both games through Proton, they are found
     automatically and you can skip this step and step 3.
3. **Tell the mod where they are** in `~/.bashrc` (or your shell's profile):
   ```
   export ZHBO_ZH_DIR=~/Games/ZeroHour
   export ZHBO_BO2_DIR=~/Games/BlackOps2
   ```
4. **Get the source and build:**
   ```
   git clone --recursive https://github.com/bryceadam1/conq-ops-zero-hour-zombies-2.git
   cd conq-ops-zero-hour-zombies-2
   cargo xtask doctor
   cargo xtask build
   ```
5. **Play:** `~/.local/share/ConquerOps/dist/conquerops` (`--check` works here too).

## Updating

```
git pull
git submodule update --init --recursive
cargo xtask build          # add --wgpu on Windows
```

The launcher re-converts Black Ops II's art by itself when the new build needs it.

## Multiplayer

LAN and online games work through Zero Hour's own lobbies; **Create a Class** is in the game setup
screen of each.

* Every player needs the **same Conquer Ops build** (the same git commit) and fully updated games.
  Different Black Ops II DLC or game languages are fine.
* In the game setup screen, the chat shows a line like `Conquer Ops content 1B8C021D`. It must be
  the same for everyone. If it isn't, the join is refused and the message names the part that
  differs (weapon table, tuning pack, classes, build, …).
* Online games also need the same game build on every machine. A Mac and a Windows PC can try a
  LAN game together; that combination has not been tested yet.
* If a game ever desyncs, start every machine with `ZHBO_STATE_HASH_LOG=1` set, play until it
  happens, and compare the `state hash:` lines in each machine's `ConquerOps.log`: the first line
  that differs names the frame and the player.

## Troubleshooting

* **"Zero Hour / Black Ops II not found."** Run the launcher with `--check` to see where it looked.
  Set `ZHBO_ZH_DIR` and `ZHBO_BO2_DIR` to the game folders: Zero Hour's holds `W3DZH.big` and a
  `ZH_Generals` folder, Black Ops II's holds `t6mp.exe` and `zone/all/common_mp.ff`.
* **The first start seems to hang.** It is converting Black Ops II's art into
  `ConquerOps_BO2.big` next to the launcher, which can take several minutes. If the launcher
  stops with an error, the message is also written to `ConquerOps-launcher.log` in that folder.
* **Logs** are in the Zero Hour user-data folder (`ConquerOps.log`) and next to the launcher
  (`ConquerOps-launcher.log`, `zhbo-gfx.log` for the renderer). The Zero Hour user-data folder is
  `Documents\Command and Conquer Generals Zero Hour Data` on Windows and
  `~/Library/Application Support/Command and Conquer Generals Zero Hour Data` on macOS.
* **Build problems:** run `cargo xtask doctor` first. Platform details and macOS-specific fixes
  are in [PLATFORMS.md](PLATFORMS.md).
