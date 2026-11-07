# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**Read [AGENTS.md](AGENTS.md) first.** It holds the project's binding rules: the ground rules (no piracy, never launch/hook/connect to Black Ops II, Rust wherever possible, determinism, single data sources), the verification ladder, commit and journal discipline, and the read order for `docs/`. This file adds the commands and the big picture; it doesn't repeat those rules.

## What this is

Conquer Ops is a mod for the Zero Hour engine (C&C Generals, the SuperHackers `GeneralsGameCode` fork in `host/GeneralsGameCode`). It turns each player's General into a BO2-style first-person shooter, inside a normal Zero Hour RTS match. The Rust core decides gameplay and draws BO2's art. BO2's assets are converted at run time from the player's own install into `ConquerOps_BO2.big`, the "archive"; nothing from either game ships.

## Commands

Set the game dirs first (SteamCMD installs aren't Steam libraries): `export ZHBO_ZH_DIR=~/Games/ZeroHour ZHBO_BO2_DIR=~/Games/BlackOps2`.

| Task | Command |
|---|---|
| Unit, headless and conformance tests (no installs needed) | `cargo test --workspace`, run with `ZHBO_BO2_DIR` **unset**, because a requirements test reads it |
| A single test | `cargo test -p zhbo-core <name_substring>` (e.g. `cargo test -p zhbo-core lockstep`) |
| Lint | `cargo clippy --workspace --all-targets`, plus `-p zhbo-core --features harness,renderer` and `--features autopilot` |
| Real-install suite (ignored tests; needs both dirs set) | `cargo test --release -p zhbo-assets -- --ignored`; the full-roster lockstep gate is `the_install_roster_plays_in_lockstep` |
| BO2 conformance CSVs | `cargo run --release -p zhbo-assets -- conformance <dir outside the repo>` |
| What's missing to build | `cargo xtask doctor` |
| Build engine and mod into a dist | `cargo xtask build [--dist <dir>]` (`--wgpu` on Windows); `cargo xtask run` builds and starts it |
| Apply host patches to the engine only | `cargo xtask patch` |
| In-game autotest (macOS) | `scripts/autotest-mac.sh [--leg]`; the legs are listed in the script's header and in docs/TESTING.md |
| Two-instance multiplayer test (macOS) | `scripts/mptest-mac.sh`; a long run is `ZHBO_MPTEST_ROUNDS=3` |

**Default dist trap:** a plain `cargo xtask build` builds into the *player's own* `~/Library/Application Support/ConquerOps/{work,dist}` (Windows: `%LOCALAPPDATA%\ConquerOps`). For experiments, pass `--dist <dir>` and set `ZHBO_WORK_DIR=<dir>`.

**Before an autotest:** the scripts launch real Zero Hour and seed test profiles. Run only one game at a time. Back up and restore the files in the Zero Hour user-data folder: `ConquerOps.profile`, `Options.ini`, `Skirmish.ini`, `SkirmishStats.ini`, `Network.ini` and the last replay.

## Architecture (big picture)

The crate graph and every module are described in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md); the engine↔Rust contract is [docs/CONTRACT.md](docs/CONTRACT.md). The shape to keep in mind:

- **Two halves joined by a C ABI.** `host/zhbo` (C++) bridges the engine to `zhbo-core` through the `zhbo_*` functions and `Zhbo*` structs. Engine behaviour changes are numbered patches in `host/patches` (`0001-…` onward), applied by `xtask`. ABI changes are append-only, with a `ZHBO_ABI_VERSION` bump and size asserts on both sides.
- **Sim vs presentation.** Everything reachable from `zhbo_logic_*` (the `sim/` modules) runs in Zero Hour's 30 Hz lockstep and must be deterministic:
  - inputs arrive as replicated messages;
  - randomness comes from the seeded streams; there's no wall clock or render state;
  - all sim state goes into `Sim::state_hash` through the `state_hash!` macro, and a perturbation test checks every field.

  The client (`client/`, `hud/`, `nativevm`, `nativebody`) is presentation only. It renders at the display rate, interpolating between logic frames, and must never feed the sim. Weapon-table fields carry a `Role`: Sim fields go into the multiplayer content CRC, Presentation fields stay out of it.
- **Converter vs runtime.** `zhbo-assets` (the converter, run by the launcher) is the only code that reads BO2 files, using `zhbo-formats`' readers. It writes the weapon table, sprites, sounds and native model blobs (`zhbo-native` formats) into the archive. `zhbo-core` reads only that output. A change to what the converter writes needs a `hud::VERSION` bump, or old archives misbehave. Native blob formats have their own `VERSION`s.
- **Rendering.** The engine's bgfx calls are implemented on wgpu (`host/zhbo/bgfx_wgpu.cpp` → `zhbo-gfx` over the `zhgfx_*` ABI). BO2 models (viewmodels, bodies, world weapons) draw in `zhbo-gfx`'s native pass with BO2's own lit-shader model. See [docs/RENDERER.md](docs/RENDERER.md).
- **Single data sources.** Gameplay numbers live in the tuning pack (`zhbo-core/src/tuning.rs` defaults plus `data/ConquerOps/tuning.ini`, each key citing its BO2 source) and the weapon table schema (`weapons/table.rs`). The mod INIs are in `data/`; see [docs/CONFIG.md](docs/CONFIG.md).

## Parity work

The goal is to match BO2's own rules, not to approximate them.
- **BO2 evidence:** the `bo2-reference` skill points to the local reference environment: decompiled GSC/Lua, a Ghidra project of `t6mp`, captures and the asset catalog. Cite the BO2 function or GSC line in code and in MODLOG.
- **Conformance oracle:** `zhbo-conformance` builds the expected side from BO2 data. The gate fails on any differing cell.
- **Never game it:** a gap closes only when the mod matches BO2, or when a transcription bug on the expected side is cited. Never add an allowlist or schema exemption.

## Multi-agent rounds

Larger work runs as rounds of parallel item agents and one integrator. The `agent-round` skill (`.claude/skills/agent-round/`) holds the conventions: briefs and per-item research in `~/.conquerops/plan/`, worktrees in `.claude/worktrees/`, the shared game lock, user-file backups, which tests each role runs, and who bumps versions.

## Journal and docs

- **MODLOG:** [MODLOG.md](MODLOG.md) is the append-only development journal, holding gotchas, decisions and verification results. Append to it; never rewrite it.
- **Docs:** update the affected `docs/*.md` in the same commit as the behaviour they describe.
- **CONTRACT notes:** pending ABI and archive changes go under CONTRACT's "Pending for the next bump" note until an integration bumps the versions.
