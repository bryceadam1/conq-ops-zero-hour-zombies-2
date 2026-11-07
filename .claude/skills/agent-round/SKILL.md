---
name: Agent Round
description: How to run a round of parallel item agents plus an integrator on the Conquer Ops mod — briefs, worktrees, the shared game lock, user-file backups, test scope, and the integrator's merge/bump/full-ladder duties. Use whenever orchestrating subagents on this project, starting a new round of fixes, or acting as an item agent or integrator.
---

# Agent Round

A round is several **item agents** (one per issue, each on its own branch) followed by one **integrator**
that merges them, bumps versions, and runs the full ladder. The orchestrator (the primary session) writes
the briefs, launches agents, reviews reports and screenshots, and pushes `main`.

## Layout

- **Plan dir:** `~/.conquerops/plan/`. Shared briefs `BRIEF*.md`, the latest one first; each brief names the
  earlier ones it extends. Each item's research, logs and captures go in `~/.conquerops/plan/<item>/`.
  Durable findings go into MODLOG and docs, never only into the plan dir.
- **Worktrees:** `.claude/worktrees/<item>` on branch `v1/<item>`, off `main` or off the previous integration's
  pushed head.
- **Conformance baseline:** the previous integration's `~/.conquerops/plan/integration<N>/conf/`. Any newly
  differing cell is a regression.

## Keeping rounds short (hard limits)

Rounds of 3–9 hours came from too many agents, oversized items, repeated full test passes, and reopening
finished agents. These limits are part of every brief:

- **At most 4 item agents per round.** More agents queue on the single game lock and hit the usage limit
  sooner, so they don't finish sooner. Split a longer list across rounds.
- **One item, one system, one change set.**
  - An item should finish in about 90 minutes.
  - Anything bigger is split before launch, with the larger port in its own round.
  - If an item outgrows its budget anyway, the agent lands the verified core, lists the rest under "Not
    covered", and stops.
- **Time-box research.**
  - About 20 minutes of Ghidra or GSC reading per rule.
  - If a rule can't be resolved by then, port what's verified, record the open question with the function
    address, and move on.
  - No open-ended "sweep everything" briefs. A sweep is a separate read-only audit agent with no builds or
    game runs.
- **At most 3 game builds per item agent.** Batch every fix before rebuilding, and use headless tests to
  iterate.
- **Item agents run at most 3 autotest legs:** the ones that exercise the change, plus the short `--mp` leg
  only if the sim or engine logic changed. Never a full pass, and never `ZHBO_MPTEST_ROUNDS`.
- **Never reopen a finished agent for new work.** Follow-ups, found-elsewhere items and the orchestrator's
  own ideas go on the next round's list. A finished agent is resumed only to answer a question about its
  own report or to fix a defect in its own branch.
- **The integrator runs the full ladder once.**
  - After a fix, rerun only the legs the fix touches.
  - Run `ZHBO_MPTEST_ROUNDS=3` once, and a second time only if the sim changed or the first run failed.
  - Push the merged head as soon as the merges and unit tests pass, so the next round can start in
    parallel.
- **Reports are short:** what changed, the evidence, the results table, bumps owed, open items. No narration
  of the process.
- **The orchestrator tracks time.** If an agent passes twice its budget, the orchestrator asks for its
  status and tells it to land what's verified.

## Rules every agent follows

- **Port BO2's rule from its code or data.** Cite the Ghidra function or GSC line, keep the fix data-driven
  (weapon table, tuning pack), and cover every similar case. The `bo2-reference` skill finds the ground
  truth.
- **Multiplayer must not break.**
  - New sim state goes into `Sim::state_hash`, and new sim inputs travel in the replicated stream.
  - Presentation never feeds the sim.
  - The lockstep gate and the real-install roster gate stay green.
- **Test scope.** An item agent runs only what its change affects:
  - the touched crates' tests;
  - the lockstep gate if the sim changed;
  - only the autotest legs that exercise the change;
  - the short `--mp` leg only if the sim or engine logic changed.

  Batch fixes before rebuilding. The integrator runs everything.
- **Builds.** Always pass `cargo xtask build --dist <plan>/dist` with `ZHBO_WORK_DIR=<plan>/work`. A plain
  build overwrites the player's own dist. Keep one `target/`, and pause if `df -h $HOME` shows under 10 GB.
- **One game machine-wide.**
  - Wait until `pgrep -x generalszh` is empty, then take the lock with
    `mkdir ~/.conquerops/plan/game.lock`, and `rmdir` it after.
  - Don't use `pgrep -f Generals`: it matches its own shell.
  - Back up and restore the user files in the Zero Hour user-data folder around every run:
    `ConquerOps.profile`, `Options.ini`, `Skirmish.ini`, `SkirmishStats.ini`, `Network.ini` and the last
    replay.
- **No version bumps by item agents.** ABI and archive changes go under CONTRACT's
  "Pending for the next bump" note.
- **Git.** Commit messages are descriptive prose with the `Co-Authored-By` trailer. No force-push, no bare
  `git stash`, and a WIP commit before anything risky. Append to MODLOG. Push your branch, then delete
  `target/`, the dist and bulky captures.
- **Never launch, hook or connect to Black Ops II.**

## Item agent report

Include:
- branch and commits;
- the root cause and the fix;
- the BO2 evidence;
- every case covered and how it was verified, with image paths;
- multiplayer safety;
- conformance against the baseline;
- ABI and VERSION needs;
- in-game checks for the orchestrator;
- anything found elsewhere.

## Integrator

1. **Merge.** Merge the branches `--no-ff` in a stated order, running `cargo test --workspace` after each.
   Resolve semantic overlaps, not just textual ones:
   - the union of weapon-table fields with consistent `Role`s;
   - the union of tuning keys, with `tuning.ini` listing every key;
   - duplicate implementations of the same rule (keep one);
   - the leg scripts.
2. **Bump.** `ZHBO_ABI_VERSION` on both sides with size asserts, `hud::VERSION`, and any `zhbo_native`
   format version whose record changed. Fold the pending notes into CONTRACT's Versioning section.
3. **Run the full ladder.**
   - `cargo test --workspace` with `ZHBO_BO2_DIR` unset.
   - Clippy: the workspace with `--all-targets`, and zhbo-core with `harness`, `harness,renderer` and
     `autopilot`.
   - The real-install `--ignored` suite.
   - Conformance into `conf/`.
   - `cargo xtask build`.
   - Every autotest leg, plus `ZHBO_MPTEST_ROUNDS=3` at least twice.
4. **Fix at the root.** Each failure gets its own commit and test.
5. **Finish.**
   - Write the MODLOG entry and `RESULTS.md`.
   - Fast-forward local `main`, but don't push; the orchestrator pushes.
   - Rebuild the player's default dist from `main`, seeded with the current archive.
   - Clean up.

## Orchestrator notes

- **When an agent stops** (usage limit, network drop or crash), resume it with SendMessage; don't start a
  new one.
  - After a crash, check for a stale `game.lock` and a seeded test profile left in the user-data folder.
  - Restore the real profile from an agent's `userbackup/`; the backups should agree.
- **Next round's list.** Collect cross-branch overlaps and leftovers from the item reports into the brief's
  "Integrator notes" section, and turn follow-up work into next round's items rather than reopening a
  finished agent.
- **Long rounds.** The integrator can push its merged head as soon as the merges and unit tests pass, so the
  next round's agents can branch off it while it runs the full ladder.
