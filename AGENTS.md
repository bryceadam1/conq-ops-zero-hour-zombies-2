# Agent guidelines

Instructions for any agent (or human) changing this codebase. Read this first, then the docs
below in the read order. If your change contradicts something written here or in the docs,
update the docs in the same commit — stale documentation is a bug.

## What this project is

A mod for the *Zero Hour* engine (Command & Conquer: Generals) that replaces the RTS with
BO2-style first-person shooting. The gameplay core and renderer are Rust; a thin C++ layer
bridges the engine to it. The mod never ships game files and never runs Black Ops II — see
the ground rules.

## Read order

1. This file.
2. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — the crate graph and the dependency rules.
3. [docs/CONTRACT.md](docs/CONTRACT.md) — engine ↔ Rust ownership, units, timing, lifecycle.
   Anything crossing the ABI boundary is defined here.
4. [docs/CONFIG.md](docs/CONFIG.md) — the mod INI files and the tuning pack.
5. [docs/TESTING.md](docs/TESTING.md) — every test layer and how to run it.
6. [MODLOG.md](MODLOG.md) — the development journal and its gotchas. **Append to it**, do not
   rewrite history.
7. As needed: [docs/RENDERER.md](docs/RENDERER.md), [docs/PLATFORMS.md](docs/PLATFORMS.md).

## Ground rules (do not break)

- **No piracy.** The mod refuses to run unless both Zero Hour and Black Ops II are installed.
  It ships no game files; assets are converted from the player's own installs at run time.
- **Never bypass anti-cheat. Never launch, hook or connect to Black Ops II.** Only its files
  are read; decrypting BO2 fastfiles from the player's own install for conversion is allowed,
  and the output stays local and is never shipped.
  - Development-workstation carve-out (approved 2026-10-20): running your own BO2 install for
    reference — screenshots/recordings, input automation, static analysis of its files — is
    allowed **offline only**: Steam in offline mode, Custom Games/local matches only, never a
    public or online match, and no debugger or memory tool attached while the game is online or
    in any VAC/TAC-protected context. The mod itself still never launches, hooks, or connects
    to BO2.
  - Addendum (approved 2026-10-20): stock BO2 MP requires Steam online even for Custom Games
    (verified in-game and by community research; no legitimate offline path exists). Steam may
    be taken ONLINE solely to reach the MP menu shell for Custom Games → Create a Class
    reference capture. Hard bans unchanged: never a public/online match or matchmaking of any
    kind, never League Play, no client modification, no debugger or memory tool attached at any
    time while the game is running (TAC loads in-menu; enforcement is not limited to public
    servers), and Steam is returned to persistent offline mode when a capture session ends.
- **Rust wherever possible.** C++ is limited to the engine hooks (`host/patches`), the bridge
  and the platform/adapter layer in `host/zhbo`. Do not add C++ outside those files.
- **Native builds, no Wine.** Windows, macOS and Linux all build from source.
- **Renderer: wgpu directly.** No bgfx library, no MoltenVK detour. `host/zhbo/bgfx_wgpu.cpp`
  implements the engine's bgfx API subset on the `zhgfx_*` C ABI; that ABI is append-only —
  new functions go at the end with a version bump and size asserts, per CONTRACT.md.
- **Determinism.** The sim must stay replayable: no wall-clock or render-time inputs in the
  logic step, RNG through the seeded streams, input via the event stream. The fake-host
  harness (`crates/zhbo-core/tests/`) is the arbiter — sim logic needs no engine.
- **Single data sources.** Gameplay numbers come from the tuning pack (`tuning.rs` defaults +
  the mod INIs) and the weapon table schema (`crates/zhbo-core/src/weapons/schema.rs`);
  never hand-copy a number between sites. Input uses the `Key` enum, never raw VK codes;
  sim→UI text goes through `describe_event`.

## How to verify a change

Follow the ladder in [docs/TESTING.md](docs/TESTING.md), bottom to top, until you hit the top
layer you can reach:

1. `cargo test --workspace` (unit, headless, conformance — runs everywhere, no installs).
2. `cargo clippy --workspace` — clean.
3. In-game scripted autotest: `scripts/autotest.ps1` (Windows) or `scripts/autotest-mac.sh`
   (macOS; `--input` for the real-keyboard/mouse test). `scripts/inputtest.ps1` is Windows-only.
4. Windows-only checks still needing the Windows VM: the DX8 reference build for side-by-side
   visual comparison, and regressions in the 64-bit Windows wgpu build.

A behavior change to the sim or the ABI is not done until the in-game autotest passes. Install
the game files once (`cargo xtask doctor` tells you what is missing); see PLATFORMS.md for the
SteamCMD steps.

## Code conventions

- Keep modules single-purpose; when a file accumulates unrelated responsibilities, split it
  along the existing seams (the bridge split and the renderer's `Device`/`Resources`/
  `FrameState`/`Caches` split are the patterns to follow). Files larger than ~1,500 lines are
  a smell worth fixing before adding to them.
- Prefer explicit phase functions over long god-functions; keep frame/sim hot paths free of
  per-frame allocations where the surrounding code already avoids them.
- Every new behavior gets a test in the layer it belongs to (see TESTING.md), and a new
  contract point in CONTRACT.md if it crosses the engine↔Rust boundary.
- Renderer draws must validate against the ABI and log-or-skip on failure — never panic in
  the frame path (the existing `validate_draw`/negative-path tests show the pattern).

## Commit and journal discipline

- Commit messages are descriptive prose (what changed and why, with the verification result),
  not one-liners; they end with `Co-Authored-By: Claude <noreply@anthropic.com>`-style
  trailers matching how prior commits did it.
- Append notable gotchas and decisions to MODLOG.md as you hit them — it is the project's
  institutional memory.
- Update the affected docs (CONTRACT, ARCHITECTURE, RENDERER, PLATFORMS, README) in the same
  commit as the change they describe.

## Planning documents (do not accumulate them)

- A big cross-crate effort (a refactor, a port) may get a temporary plan or handoff doc in
  `docs/`. That document is scaffolding, not documentation: it is deleted in the same commit
  that finishes the work.
- Before deleting it, fold anything durable into the permanent docs: gotchas and environment
  traps into MODLOG.md and the relevant platform doc, new test layers into TESTING.md, new
  contract points into CONTRACT.md. Never leave a plan file that still says "not yet done" for
  finished work — the next agent will redo it.
- The permanent docs (AGENTS.md, docs/*, README) must always describe the codebase as it is.
  If you change behavior and the docs contradict you, update the docs in that same commit;
  conflicting documentation is a bug.
