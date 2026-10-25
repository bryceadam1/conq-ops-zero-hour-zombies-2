# Conformance layer — plan (temporary scaffolding)

> Planning document per AGENTS.md "Planning documents": deleted in the commit that finishes the
> work, after anything durable is folded into TESTING.md, ARCHITECTURE.md and MODLOG.md.

## Goal

Prove or disprove BO2 parity mechanically: one shared CSV schema, two code-driven generators
(`expected.csv` from the player's BO2 install through BO2's own rules, `actual.csv` from what the
mod ships), one differ that clusters every mismatch. The initial backlog it prints is the proof,
not a work queue: **no gap is fixed here and the schema is never narrowed to shrink the diff.**

## Owner decisions (2026-10-25)

1. A fact is excluded only when it is meaningless in one world (scorestreaks, Zero Hour factions,
   the V1 deliberate divergences). A BO2 fact the mod should copy but does not is a column; the
   mod side is empty and the row fails.
2. `actual.csv` is **what ships**: the launcher's own conversion (`generals::files`, in memory) and
   the game's runtime lookup (`Catalog::get`, the def's `viewmodel` manifest). A combination the
   archive has no art for shows the art the game would draw.
3. Combinations: every CaC-reachable one, including Primary Gunfighter's third attachment and
   Secondary Gunfighter's second.
4. The expected side is BO2's rules from Ghidra (`~/Documents/BO2-reference/ghidra/answers/G14.md`,
   `G15.md`), transcribed independently with citations. Offline emulation of a **pure numeric**
   t6mp function is allowed as a cross-check if a diff hinges on one.
5. CaC menu facts that live only in BO2's Lua come from a small Havok-Script bytecode reader in
   `zhbo-formats` reading the player's own `patch_mp`/`patch_ui_mp`. Lua *rules* (code) are
   transcribed with citations like engine rules; the reader supplies the data.

## Architecture

* `crates/zhbo-conformance` (new; depends on `zhbo-formats` only — the crate graph makes it
  impossible for the expected side to call the converter or the core):
  * `schema` — every column: name, area, kind (enum/int/float/list/set/string/bool), BO2 unit,
    per-column epsilon (floats only, unit round-trip noise), citation.
  * `csv` — one writer/reader for both files.
  * `diff` — the join on `(weapon, combo)` and the clustered report (`report.md`, long-form
    `diff.csv`); missing rows on either side are a cluster too.
  * `bo2/` — the oracle: `combos` (G15 legality + caps), `resolve` (G14 uniques, parts, stats,
    sounds, effects, hide, anims), `rows` (expected rows), `cac` (expected CaC rows).
* `zhbo-formats`: parser additions the oracle needs (unique index and links, override bit sets,
  the default attachment, variant rotations, the remaining attachment fields, the Havok Lua
  reader). Additions only: nothing the mod already reads changes value.
* `zhbo-assets::conformance` — the actual side (in-memory `generals::files`, the shipped weapon
  table through the core's catalog, the `.zbv`/`.zbm`/`.zcm` blobs, the CaC data), the
  `conformance` CLI subcommand, and the install-gated test.

Units: every column is in BO2's unit (milliseconds as BO2 truncates them, inches, degrees); the
actual emitter converts back through `ZH_PER_INCH` and ×1000.

Row keys: `weapon` = the BO2 variant name (an alt mode is its own variant: `gl_an94_mp`), `combo` =
the attachment shorts in `eAttachment` order joined by `+` (`""` for the bare weapon). Alt rows
keep the parent's attachment fields (G14 §5: the alt index replaces only the variant byte).

## Phases

- [x] 0. Ghidra G14 (combination resolution) and G15 (CaC legality), read-only.
- [x] 1. Crate skeleton, schema, CSV, differ + unit tests.
- [x] 2. Parser additions: none needed — the oracle reads the raw structs through the layouts
      (`bo2/raw.rs`); `t6zone::synthetic` is public for the oracle's fixture tests.
- [x] 3. Oracle: combos, resolve, expected weapon rows (G16: `getloadoutweapon`, ammo, camo).
- [ ] 4. Actual weapon rows from the in-memory conversion.
- [ ] 5. Havok Lua reader; CaC CSV on both sides.
- [ ] 6. CLI subcommand, install-gated test, docs (TESTING rung, ARCHITECTURE crate, MODLOG).
- [ ] 7. Verify: `cargo test --workspace`, clippy `--all-targets`, real-install suite.
