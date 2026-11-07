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
7. As needed: [docs/RENDERER.md](docs/RENDERER.md), [docs/PLATFORMS.md](docs/PLATFORMS.md),
   [docs/COMMANDS.md](docs/COMMANDS.md) (the first-person command system; design, not built yet).

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
  harness (`FakeHost` in `crates/zhbo-core/src/sim/mod.rs`'s tests, used by the `sim/*_tests.rs`
  and the other sim modules' `#[cfg(test)]` blocks) is the arbiter — sim logic needs no engine.
- **Single data sources.** Gameplay numbers come from the tuning pack (`tuning.rs` defaults +
  the mod INIs) and the weapon table schema (`crates/zhbo-core/src/weapons/table.rs`);
  never hand-copy a number between sites. Input uses the `Key` enum, never raw VK codes;
  sim→UI text goes through `describe_event`.

## How to verify a change

Follow the ladder in [docs/TESTING.md](docs/TESTING.md), bottom to top, until you hit the top
layer you can reach:

1. `cargo test --workspace` (unit, headless, conformance — runs everywhere, no installs).
2. `cargo clippy --workspace` — clean.
3. For parity work (anything that should match BO2): the BO2 conformance gate,
   `ZHBO_BO2_DIR=… cargo test --release -p zhbo-assets conformance -- --ignored`, and its CSVs
   (`cargo run --release -p zhbo-assets -- conformance <dir outside the repo>`); a fix is done when
   its gap clusters leave the report (TESTING.md "BO2 conformance").
4. In-game scripted autotest: `scripts/autotest.ps1` (Windows) or `scripts/autotest-mac.sh`
   (macOS; `--input` for the real-keyboard/mouse test). `scripts/inputtest.ps1` is Windows-only.
5. Windows-only checks still needing the Windows VM: the DX8 reference build for side-by-side
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

## Lessons learned

Distilled from the bugs found in play. MODLOG has each case in full. Read this section before changing a
system.

### BO2 parity

- **Assume an unported rule is a bug.** Nearly every bug players found was the mod not following one of
  BO2's rules. Examples: a placeholder from the first core (hip-fire spread), an approximation (the knife's
  facing cone, the grenade's invented lift), or BO2 data that was ignored (material polygon offset, the lit
  shader). Treat these as defects to port:
  - a numeric literal in logic that isn't from the tuning pack or the weapon table;
  - a comment saying "placeholder", "stand-in", "approx", "for now" or "roughly";
  - a branch on weapon class where BO2 reads a def field;
  - a tuning key with no BO2 citation.
- **Read BO2's code; don't trust memory or earlier notes.** Answer files, audits and an orchestrator's
  recollection have all been wrong:
  - BO2's server melee traces nine rays; the five-ray function is the client's impact effect.
  - The `bg_fallDamage*` dvars are registered but never read; the constants are in PM_CrashLand.
  - The view bob moves the eye, not the view angles.
  - The Sensor Grenade does go off on a timer.

  Re-read the function in t6mp (Ghidra, GSC or shader DXBC) before porting it, and cite it. Evidence beats
  intuition, including the user's or the orchestrator's.
- **Port the whole rule.** That includes:
  - its data: the weapon field through the converter, the table and the conformance schema;
  - its units: BO2 inches ↔ Zero Hour units via `ZH_PER_INCH`;
  - its rounding: BO2 truncates damage stage by stage;
  - its timing: in milliseconds or per second, not per 30 Hz frame;
  - every case it touches across the roster, with a test that sweeps them all.
- **Record deliberate breaks from BO2** in MODLOG and the affected doc, and don't "fix" them:
  - a class pick for every new General;
  - the General's toughness against Zero Hour weapons (`GeneralHeroToughness`, the heroes' health);
  - no ragdoll;
  - no scorestreaks;
  - no mantling.
- **A hidden engine object still has jobs.** Hiding the W3D body for BO2's native body also lost the walk
  animation, the muzzle position for fire FX, click-picking, the decal shadow and the shroud rule. When the
  native path replaces something the engine drew, list everything the engine did with it, and carry each
  job over.
- **Presentation must be right at any render rate.** Rendering follows the display, while logic runs at
  30 Hz. Code that moves a fixed amount per draw ran about 4× fast at 120 Hz: tracers, recoil, treads, and
  the autopilot sending inputs three times a frame. Advance by elapsed time, or once per logic frame
  (`Common/ZhboRenderStep.h`).
- **Version what the converter writes.** Any change to converted output needs a `hud::VERSION` bump, or old
  archives misread silently. A record that grows needs its `zhbo_native` format version bumped too.

### Multiplayer (lockstep)

- **Every piece of sim state goes into `Sim::state_hash`** through the `state_hash!` macro, and the
  perturbation test must cover it. A presentation-only buffer may stay out only with the reason written at
  the fingerprint, as `Sim::blows` does.
- **The sim never reads anything a client consumes or drains**, such as the event window a rendering client
  empties. Keep per-seat state in the sim instead.
- **The weapon table's `Role` split is the content CRC.** Sim fields go into it; Presentation fields must
  never be read by sim-reachable code, and a source-scan test enforces that. Give every new field the right
  `Role`.
- **Mind the order of events within a step.** Hits are scored after every seat has stepped, so a victim's
  record must survive his own seat's step. Deleting it early lost kills and assists whenever the victim's
  seat stepped after the killer's. Engine-side damage reaches the sim through an ordered queue
  (`enginedamage`).
- **Logic must never depend on render state.** Click-picks use the sim's pose and the logic position;
  render-only positions such as the drawn muzzle feed effects only.
- **Read multiplayer test results carefully:**
  - Quitting a network game self-destructs that player's General on every machine, in lockstep, so a run
    stopped by its time limit looks like a sudden death or a desync. `mptest-mac.sh` compares state hashes
    only up to the first "leaving the game" line, and fails a run that hit its time limit.
  - Lethality changes break scripted autopilots. At BO2 health whoever fires first wins, so retune the
    scripts in the same change.
- **Shared globals make parallel tests flaky.** Use per-test-thread state, as the native body tests do.

## Multi-agent rounds

Work split across parallel agents follows `.claude/skills/agent-round/SKILL.md`, including its hard limits
for keeping rounds short:
- at most 4 item agents;
- about 90 minutes per item;
- time-boxed research;
- at most 3 builds and 3 autotest legs per item agent;
- no reopening finished agents;
- one full ladder by the integrator.

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
