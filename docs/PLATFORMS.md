# Platforms: Windows, Linux, macOS

Zero Hour and Black Ops II are Windows games, and Conquer Ops' game executable (`generalszh.exe`, the
source-built engine with the Rust core linked in) is a 32-bit Windows program on every platform. On
Linux and macOS it runs the same way the games themselves do there: through Proton or Wine. Everything
around it (install discovery, the launcher, the BO2 art converter, the build tooling) is native Rust.

| | Windows | Linux | macOS |
|---|---|---|---|
| Game executable | native | Proton (preferred) or Wine | CrossOver, Whisky or Wine |
| Launcher | `ConquerOps.exe` | `conquerops` (native) | `conquerops` (native) |
| Build | Visual Studio 2022 | MinGW-w64 cross compile | MinGW-w64 cross compile |
| Status | built and tested in game | groundwork; not yet run in game | groundwork; not yet run in game |

## Where the games are found

`crates/zhbo-requirements` searches, in order:

1. `ZHBO_ZH_DIR` / `ZHBO_BO2_DIR` (explicit override).
2. Steam libraries, via each Steam client's `libraryfolders.vdf` and `appmanifest_<appid>.acf`
   (Zero Hour: 2732960; Black Ops II: 202970, 202990, 212910):
   * Windows: registry `SteamPath`/`InstallPath`, `C:\Program Files (x86)\Steam`.
   * Linux: `~/.steam/steam`, `~/.steam/root`, `~/.local/share/Steam`, Flatpak
     (`~/.var/app/com.valvesoftware.Steam/...`), Snap (`~/snap/steam/...`).
   * macOS: `~/Library/Application Support/Steam`.
   * Windows Steam clients inside prefixes (below). Their Windows library paths (`D:\SteamLibrary`) are
     mapped back to host paths through the prefix's `drive_c` / `dosdevices`.
3. Windows only: the EA registry key.

Prefixes searched: `$WINEPREFIX`, `~/.wine`, `~/.local/share/wineprefixes/*`, `~/Games/*` (Lutris),
CrossOver bottles (`~/Library/Application Support/CrossOver/Bottles/*`), Whisky bottles
(`~/Library/Containers/com.isaacmarovitz.Whisky/Bottles/*`), and Proton prefixes
(`steamapps/compatdata/*/pfx`). File checks are case-insensitive, since Linux filesystems are not.

## How the launcher starts the game

`crates/zhbo-launcher/src/runner.rs`:

| Zero Hour found in | Runner |
|---|---|
| a native Steam library (Linux) | newest Proton (`steamapps/common/Proton*`, `compatibilitytools.d/*`), with Zero Hour's own Proton prefix (`compatdata/2732960`) |
| a Proton prefix | newest Proton, that prefix |
| a CrossOver bottle | `/Applications/CrossOver.app/.../bin/wine --bottle <name>` |
| a Whisky bottle | Whisky's `wine64` with `WINEPREFIX` |
| a Wine prefix | `wine` from `PATH` with `WINEPREFIX` |

Overrides: `ZHBO_PROTON=/path/to/proton`, `ZHBO_WINE=/path/to/wine`.

The launcher passes Windows-style paths to the game (`C:\...` inside the prefix, `Z:\...` for host
paths), including `ZHBO_ZH_DIR`/`ZHBO_BO2_DIR` so the requirements check inside the game process sees
the same installs. `conquerops --check` prints what was found and the exact command, without starting
anything.

## Building on Linux and macOS

```
# Debian/Ubuntu
sudo apt install g++-mingw-w64-i686 mingw-w64-tools cmake make git
# Fedora
sudo dnf install mingw32-gcc-c++ mingw32-widl cmake make git
# macOS
brew install mingw-w64 cmake

rustup target add i686-pc-windows-gnu
cargo xtask doctor    # lists anything still missing
cargo xtask build     # core (i686-pc-windows-gnu) + engine (preset mingw-w64-i686) + native launcher
```

The engine uses TheSuperHackers' own MinGW-w64 preset (`mingw-w64-i686`, which disables the MFC tools).
It needs `widl` (the IDL compiler from mingw-w64-tools) and fetches ReactOS's ATL headers on first
configure (a ~250 MB shallow clone).
The Rust core is built for `i686-pc-windows-gnu` and linked as `libzhbo_core.a`; `host/zhbo/CMakeLists.txt`
handles both toolchains.

## Verified so far

* Windows (MSVC): full build, in-game autotest 7/7.
* The Rust crates type-check for `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin`; the core builds
  for `i686-pc-windows-gnu`.
* Install discovery is unit-tested against a simulated CrossOver bottle with a second Steam library.
* MinGW-w64 build of the patched engine with the bridge: see MODLOG.md for the latest result.

## Known gaps

* Not yet run under Proton/Wine/CrossOver. Expected issues to check first: mouse capture (the bridge
  re-centres the cursor with `SetCursorPos`; Wine on Wayland and macOS can be unreliable), and Direct3D 8
  translation (Proton uses DXVK/d8vk; CrossOver uses its own D3D translation).
* The in-game autotest (`scripts/autotest.ps1`) is Windows-only (it drives input with `um win`).
* Apple Silicon runs the game through Rosetta and Wine's 32-bit support (CrossOver / Whisky with
  Wine 8+'s WoW64 mode); performance is untested.
