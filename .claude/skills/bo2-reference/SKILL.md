---
name: BO2 Reference
description: Locate and use the local Black Ops II reference environment (decompiled Lua/GSC, menu layouts, native-res captures, Ghidra project, asset catalog) when building or porting BO2 features into the Conquer Ops mod. Use this whenever the work involves BO2 parity — porting a BO2 screen, menu, or mechanic, making something "look/feel exactly like BO2", matching BO2 behavior or rules, or answering "how does BO2 do X" — even if the user doesn't name the reference environment explicitly.
---

# BO2 Reference — building exact BO2-parity features into the mod

A complete, compliant reference mirror of Black Ops II lives OUTSIDE the mod repo at
`~/Documents/BO2-reference/`. Every fastfile is dumped; every script decompiled; every menu read;
all three exes disassembled; the MP Create-a-Class captured on camera. Your job when porting a
feature: find the ground truth in this material FIRST, implement to the mod's rules, and cite what
you used. Never guess BO2 behavior when the answer is a file read away.

## Ground rules (these protect the project)

- **Reference only.** The mod ships nothing from BO2 — no assets, no code, no copied tables.
  Structure, behavior, layout, and names are learned here; the mod's own code/data pipelines carry
  what actually ships.
- **Numbers come from the mod's single data sources** (tuning pack + weapon-table schema + the
  zhbo-formats conversion pipeline), never hand-copied out of reference files into mod code. Read
  the reference to learn HOW something is computed or ordered; the VALUES flow through the
  pipeline.
- **Cite your sources in MODLOG.md** for parity decisions (file:line in the dumps, or the capture
  path). The next agent must be able to audit why the mod is the way it is.
- **Reading is self-serve. New capture/VM/Ghidra-compute work is not.** Anything needing the
  Windows VM, new screenshots, new zone dumps, or fresh Ghidra analysis is requested via the user,
  who routes it to the BO2-reference session — parallel drivers would collide with it.

## The map — always start at an index, never grep blind

Root: `~/Documents/BO2-reference/`. Ledger of everything (with validation evidence):
`STATUS.md`.

| Need | Start here | Then |
|---|---|---|
| **UI logic** (screens, flows, state) — Lua | `lua-dump/MP-INDEX.md` (MP umbrella) · `CAC-INDEX.md` (CaC deep-dive) · `SP-INDEX.md` · `ZM-INDEX.md` | `lua-dump/decompiled/<zone>/…` (1,144 scripts) |
| **Game logic** (rules, gametypes, zombies, missions, Strike Force RTS) — GSC | `gsc-dump/GSC-INDEX.md` · `lua-dump/MAPS-DUMP.md` (map zones) | `gsc-dump/<mode-or-zone>/decompiled/t6/…` (3,862 files, all recompile-verified) |
| **Menu layouts** (coordinates, colors, fonts, items, key handlers) | `lua-dump/MENUS.md` | `lua-dump/menus-mp/` (all 133 menuDefs; NOT Create a Class — see gotchas) |
| **Visual truth** (pixel layout, feel, states) | `cac/005`–`009` (key screens) | `cac/sweep/<chapter>/` (295 captures: chooseclass, editor, primary, attach, perks, wildcards, nades, streaks, personalize, dialogs) |
| **Engine behavior** (what scripts can't show) | `ghidra/GHIDRA.md` | `ghidra/scripts/FindStrings.java`, `DecompileFn.java` — tested recipes; 3 programs (t6mp/t6sp/t6zm), ~90% coverage |
| **Assets** (what art/models/sounds exist) | `lua-dump/ASSETS.md` | `lua-dump/assets/<zone>.tsv` (126 zones, 98,118 distinct) |
| **Mod-side loader design** | `ZHBO-ASSET-LOADER-NOTES.md` | zhbo-formats already parses 51 T6 asset types incl. menuDef + ipak/IWi |

## The porting workflow

1. **Scope the BO2 surface.** Name the exact screens, rules, and data your feature has in BO2.
2. **Pick channels by content type** (this is the decision that saves hours):
   - CaC and any LUI-panel screen → **Lua + captures** (menuDefs do NOT contain these).
   - Any other menu (main shell, popups, options) → **menuDefs** first for layout, Lua for behavior.
   - Rules/mechanics (gametype flow, rounds, perks, streaks, RTS) → **GSC**.
   - Anything still unexplained (engine math, rendering-side behavior) → **Ghidra query**.
   - Numbers/weapon stats → the mod's own pipeline (see ground rules).
3. **Extract exact facts, with citations.** Verbatim names, orderings, caps, state transitions,
   dialog text, keybinds. Record file:line (dumps) or capture path as you go — you'll need them for
   MODLOG and for review.
4. **Resolve localized strings, don't transcribe.** `@MENU_*` / `@MPUI_*`-style references in Lua
   and menuDefs point into stringtables in the dump (per-zone `.csv`/stringtable assets). Resolve
   the reference to the real text; don't guess display strings from memory.
5. **Implement to the repo's rules** (its AGENTS.md): Rust where possible, determinism in the sim,
   single data sources, tests in the right layer per docs/TESTING.md.
6. **Verify parity side-by-side.** The mod's own capture tooling vs the reference captures
   (`cac/sweep/`), plus the in-game autotest. Look for: item order, highlight/selection states,
   footer keybind labels, counters, empty/locked/error states — the captures cover them.
7. **Journal it.** MODLOG.md gets what you ported and the exact citations behind the decisions.

## Exactness checklist (for "identical look and feel")

- Layout metrics / colors / fonts: menuDefs (`menus-mp/`) for menu screens; captures
  (`cac/sweep/`) for Lua screens. The captures are native 3024×1890 — measure directly.
- Display text: resolved from stringtables (step 4), verbatim.
- Item order and contents (weapons, perks, attachments, streaks): Lua selection grids + the
  matching sweep chapter.
- Rules and costs: GSC/Lua logic — e.g. allocation math in `cacutility.lua` (see CAC-INDEX).
- Art: look up names in `ASSETS.md`, pull through the mod's zhbo-formats/zhbo-assets conversion
  (per `ZHBO-ASSET-LOADER-NOTES.md`) — reference art never enters the repo.
- Behavior in edge states (locked items, empty slots, invalid loadouts, reset dialogs):
  `checkclasses.lua` + the `j-dialogs/` capture chapter.

## Gotchas that have bitten before (check here first)

- **Create a Class is Lua LUI panels, not menuDefs.** The 133 menuDefs cover everything EXCEPT
  CaC. CaC layout comes from Lua + captures.
- **BO2 never says "Pick 10."** The system is *allocation* (`Engine.GetMaxAllocation`,
  `remainingAllocation`, 10 points). Zero hits for `pick10` in the dumps.
- `patch_mp.ff` + `patch_ui_mp.ff` are **one co-dependent Lua namespace**; search both.
- `common/` is single-player; `common_mp/` is multiplayer (a cataloging trap already sprung once).
- The orange "counter" top-right in MP menus is the **build-version watermark** (white, e.g.
  `43.1736.11`, dvar `ui_drawBuildNumber`); the real allocation counter is the `0/10` below it.
- menuDef RPN ui-function tokens are only partially decoded (`cmd[<id>]`); a few listbox/flag
  fields are partial — noted in `MENUS.md`.
- gsc-tool ignores `#include` contents: "recompile-verified" ≠ link-complete. Includes resolve to
  sibling files in the same tree.
- Decompiled Lua uses mechanical `L1_1` locals and `arg` tables for named globals — read for
  structure and data, not idiomatic style.
- ~245 `menu_*` materials alias images of different names; the mod's loader resolves these at pull
  time (don't conclude an image is "missing" from the catalog).
- `frontend_sf_a/b/c.gsc` = Strike Force war-room choreography; the RTS combat logic is the 12
  shared `_so_rts_*` modules (see MAPS-DUMP).

## If the answer isn't in the dump

Ask the user to route it to the BO2-reference session, which owns: the VM capture pipeline (any
screen/state, incl. things never swept), Ghidra compute (engine answers with evidence), additional
zone/asset pulls (e.g. DLC, specific textures via ipak), and new tooling builds. Describe the
question concretely (what screen/rule/function, and what you already checked) — the turnaround is
fast.
