# First-person commands

How the General commands his army without leaving first person. **Status: design (2026-11-07).
Nothing here is built yet.** The decisions below were agreed with the user. Section 13 lists what
is still open.

## 1. Goal and constraints

Today the only way to command units from first person is to leave it: the field computer (F) puts
the General under RTS control and brings back the overhead view (CONTRACT.md "Who owns what").
The command system adds a way to order the army while staying in first person.

- **Full coverage.** Everything the player can do in the overhead view can be done with a command.
  The one exception is the General himself, whom the player already controls with the first-person
  controls. A selection never includes him. He can still be a target or a place (`move to me`).
- **Text first.** A command is a sentence in a small English-like language, the **canonical
  command language** (section 4). The scroll-wheel menu, a typed entry and, later, voice all
  produce that language. Nothing downstream knows which front end the sentence came from.

## 2. The pipeline

```
 voice ─▶ ASR ─▶ NL→canonical model ─┐
 typed entry ────────────────────────┼─▶ parse ─▶ Command AST ─▶ resolve ─▶ emit ─▶ GameMessages
 scroll-wheel menu ── builds the AST directly, shows its canonical text ─┘   (IDs, points)
```

- **Parse** turns canonical text into a `Command` AST. **Print** turns an AST back into canonical
  text. Each AST has exactly one canonical spelling, and `parse(print(ast)) == ast`.
- **Resolve** turns the AST into concrete object IDs and world points, using the issuing player's
  view of the world (section 3).
- **Emit** sends the same messages the overhead UI sends: `MSG_CREATE_SELECTED_GROUP`, then the
  order (`MSG_DO_ATTACKMOVETO`, `MSG_DOZER_CONSTRUCT`, `MSG_QUEUE_UNIT_CREATE`, …). This is the
  path the autopilot already uses (`host/zhbo/ZhboAutopilot.cpp`).
- **Valid completions** answers "given this partial command and the current world, what can come
  next?" The menu shows it, the typed entry offers it as completion, and the voice decoder uses it
  as its mask (section 11). There is one implementation, and every front end uses it.

**Resolution happens on the issuing client, and only standard engine messages leave it.** That
choice has these consequences:

- The command system is presentation-side code. It never runs inside `zhbo_logic_*`, adds nothing
  to `state_hash`, and needs no new sim message. Model output can be nondeterministic without any
  risk to lockstep.
- Replays and multiplayer work unchanged. The other peers see "player N selected these units and
  ordered this," exactly as for overhead clicks.
- After a command, the engine's selection holds the commanded units, just as if they had been
  clicked overhead. If the player goes overhead, they see a normal selection with normal orders.
  The General is never added to that selection.

### Where the code goes

| Part | Place |
|---|---|
| AST, parser, printer, grammar tables | `crates/zhbo-core/src/command/` (`ast.rs`, `grammar.rs`, `parse.rs`, `print.rs`) |
| Resolver and valid completions | `command/resolve.rs`, `command/complete.rs` |
| Actions built from command-set data | `command/actions.rs` |
| Menu and typed-entry UI | `hud/command*.rs` and the client's input handling (`client/`) |
| World query (objects, templates, owners, fog/ghost state, command sets, button availability, map areas) | New `ZhboClientApi` services, implemented in the bridge. `ZhboControlBarAccess` already reads the control bar this way. CONTRACT.md "Adding an engine service" applies. |
| Emitting messages | The bridge, through the message path the autopilot uses |
| Voice (ASR, translation model, constrained decoder) | Later, in its own crate. Runs client-side only. |

## 3. Decided rules

1. **Fog of war.** The resolver sees what this player's overhead view would let them click:
   - visible objects;
   - the player's ghost objects: buildings seen before and now in fog.

   A building seen once can be selected or targeted while fogged, and a building destroyed while
   fogged still shows as its ghost, as it does overhead. Units in fog or shroud cannot be
   resolved.
2. **Own units only.** A selection only ever contains the player's own objects. Allied, neutral
   and enemy objects can be targets and places, never selections.
3. **Map place names.** If a map defines named areas (trigger polygons or named waypoints), they
   become place names for that match.
4. **Selections are fixed when the command is issued.** The resolver turns the selection into an
   ID list once, at issue time. Units that later match the description are not added.
5. **Ties break the same way every time.** For a given world, the same command resolves the same
   way: order by the stated criterion, then by distance to the General, then by object ID. This is
   not needed for lockstep. It keeps behaviour predictable and the tests reproducible.

## 4. The canonical command language

The model's job is easier and the menu more readable when the canonical form is close to what a
person would say. So the language uses lowercase English words, no IDs, no symbols except the
hyphen in fixed verbs (`attack-move`), and one spelling per meaning. The parser accepts only
canonical text. Looser input is turned into canonical text first. Synonyms and aliases are
handled by the normaliser ("Synonyms and aliases" below), which the typed entry and the voice path
share. Action-first word order and filler words are the translation layer's job (section 11).

### Grammar (draft)

`"x"` is a fixed word, `<x>` is a word list built at run time (section 5), `[x]` is optional,
`{x}` repeats.

```
command     = [selection] order {"then" order}

order       = verb-phrase
            | "name" "them" <new-name>              ; save the selection as a team name
            | "make" "them" "group" digit           ; engine control group (MSG_CREATE_TEAMn)

selection   = group {"and" group} ["except" group {"and" group}]
group       = [quantity] [superlative] {adjective} base {filter}
            | "group" digit                         ; engine control group
            | <team-name>                           ; "alpha team"
            | "them"                                ; the last resolved selection
            | "that"                                ; the object under the crosshair

quantity    = "all" | "the" | number | "half" "the"
superlative = "nearest" | "farthest" | "northernmost" | "southernmost" | "easternmost"
            | "westernmost" | "weakest" | "healthiest"
adjective   = "idle" | "damaged" | "veteran" | "elite" | "heroic" | "garrisoned" | "loaded"
base        = <unit-type> | <unit-class> | "units"
filter      = "near" location
            | "in" "the" region "group"              ; cluster (section 6)
            | "in" "my" "group"                      ; the cluster the General is in

verb-phrase = <verb>                                 ; "stop", "guard", "evacuate", "sell"
            | <verb> location                        ; "move to", "attack-move to", "guard"
            | <verb> object-ref                      ; "attack", "enter", "capture", "repair"
            | <verb> [count] <buildable> [placement] ; "build", "upgrade"
            | "act" "on" object-ref                  ; whatever a right-click would do
            | "set" "rally" "point" location
            | "use" <power> [("at" location) | ("on" object-ref)]
            | "purchase" <science>                   ; Generals' promotions (no selection)
            | "place" "beacon" location

placement   = location ["facing" direction]
location    = [offset] point
offset      = "near" | direction "of" | "behind" | "in" "front" "of"
point       = "me" | "there"                         ; there = crosshair hit on the ground
            | object-ref | <place-name>
object-ref  = [relation] [quantity] [superlative] {adjective} <object-type> {filter}
            | "that"
relation    = "my" | "enemy" | "allied" | "neutral" | "civilian"
direction   = "north" | "south" | "east" | "west" | "northeast" | "northwest"
            | "southeast" | "southwest"
region      = "northern" | "southern" | "eastern" | "western" | "northeastern"
            | "northwestern" | "southeastern" | "southwestern" | "central"
count       = number                                 ; "build 3 crusader tanks"
```

Notes on the grammar:

- **Selection first, always, in canonical form.** "Build a power plant with the north dozer" is
  the translator's or normaliser's input. Its canonical form is
  `the northernmost dozer build power plant ...`.
- **The selection ends at the first verb.** Run-time names must never be the same as a verb or
  fixed word. A gate test checks this for every name in the install's data (section 12).
- **Singular and plural come from the noun.** `the ranger` is one, `rangers` is all,
  `3 rangers` is three. Each name has a singular and a plural form (section 5).
- **`then` chains orders for the same selection** using the engine's queued orders (shift-click,
  `MSG_ADD_WAYPOINT`). There are no conditionals.
- **"here" means "me".** The normaliser rewrites it, and canonical text prints `me`.
- **"there" drops the preposition.** A verb's preposition (`to`, `at`, `on`) is left out before
  `there`, as in English: `move there`, `attack-move there`, `use carpet bomb there`.

### Examples

| Canonical command | Resolves to |
|---|---|
| `rangers move to me` | All own visible Rangers. `MSG_DO_MOVETO` at the General's position. |
| `units attack-move there` | What Zero Hour's select-all selects. Attack-move to the crosshair's ground point. |
| `them attack that` | The last selection attacks the object under the crosshair. |
| `the nearest dozer build power plant near my war factory` | One dozer. `MSG_DOZER_CONSTRUCT` at the nearest legal spot to the war factory the General is nearest to. |
| `build 3 crusader tanks` | No selection given, so it is implied: the queues of the War Factories able to build it, shortest queue first (section 7). |
| `the northernmost dozer repair that` | The dozer repairs the building under the crosshair. |
| `humvees and crusader tanks in the southern group attack the nearest enemy war factory` | Union, filtered by cluster. The target is one object. |
| `units except dozers guard the nearest oil derrick` | Difference. `MSG_DO_GUARD_OBJECT` on the derrick nearest to the General. |
| `idle rangers near me name them alpha team` | Saves the IDs as `alpha team`. |
| `alpha team act on that` | Whatever right-clicking that object would do (enter, garrison, capture, …), as `CommandXlat` decides. Spoken as "alpha team, deal with that". |
| `rangers guard me` | `MSG_DO_GUARD_OBJECT` on the General: they follow and protect him. Spoken as "rangers, follow me" or "cover me". |
| `rangers move to the bridge then attack-move to north of the bridge` | A map place name, then a queued second order. |
| `use carpet bomb there` | The special power's building is the implied selection (as the shortcut button does). |
| `purchase advanced training` | Generals' promotion. No selection. |

### Synonyms and aliases

The language has two layers:
- **Canonical text**: one spelling per meaning. The parser accepts only this, the printer writes
  only this, and the model is trained to output it.
- **Accepted input**: canonical text plus synonyms and aliases.

The **normaliser** rewrites accepted input into canonical text before parsing. It is a
deterministic table lookup over phrases, longest match first. It is shared by the typed entry, the
voice path (after the model, as a second chance, and before it, to clean up recognised text) and
the training data (paraphrases use the same tables, so the model learns the synonyms players
actually say).

The tables are data, in the mod's INI files (a `CommandWords` file, keys described in CONFIG.md
when built). They are not in code. Three kinds of entry:

1. **Fixed-word synonyms.** Phrases that mean a fixed word of the grammar.
2. **Type and class aliases.** Words that name a set of types or classes, written per faction
   (`tanks` is Crusader + Paladin for the USA, Battlemaster + Overlord for China, Scorpion + Marauder
   for the GLA).
3. **The player's own synonyms.** A per-profile file with the same format as type and class
   aliases ("my boys" → rangers). These last across matches. Team names (`name them …`) are
   different: they are per match and canonical.

**Draft of the shipped tables.** The canonical word is on the left. These are first drafts and
will grow as real speech comes in.

| Canonical | Accepted synonyms |
|---|---|
| *Selections* | |
| `units` (with `all`) | everyone, everybody, all forces, my army, my forces, everything, the whole army |
| `them` | they, those, these, the same units, those guys, them again |
| `that` | this, that one, this one, it, the one I'm looking at |
| `in my group` | with me, around me, my squad, my team |
| `all` | every, each, all of the, all my |
| `the` (one) | a, an, one, any, a single |
| number | number words (one … twenty, "a dozen"), "a couple of" = 2, "both" = 2 (and requires exactly two) |
| `half the` | half of the, half my |
| `except` | but not, other than, excluding, besides, apart from, all but (as "all … except") |
| `and` (selection) | plus, along with, as well as |
| *Superlatives and adjectives* | |
| `nearest` | closest, nearest to me |
| `farthest` | furthest, farthest from me |
| `northernmost` (each direction) | most northern, northern-most, farthest north, top-most |
| `weakest` | most damaged, most hurt, lowest health |
| `healthiest` | strongest, least damaged, full-health |
| `idle` | free, inactive, unoccupied, not busy, doing nothing (after the noun) |
| `damaged` | hurt, wounded, injured, beat up |
| `veteran`, `elite`, `heroic` | vet / one-star, two-star, three-star |
| *Places* | |
| `me` | here, my position, my location, where I am, my spot |
| `there` | over there, that spot, where I'm looking, where I'm aiming, that way |
| `behind` | in back of |
| `in front of` | ahead of, before |
| `near` | next to, beside, by, close to, around |
| `north of` (each direction) | to the north of, just north of, northward of |
| *Actions* | |
| `move to` | go to, head to, get to, run to, drive to, fall back to, retreat to, come to, regroup at |
| `attack-move to` | attack move, advance to, push to, sweep to, assault, push |
| `attack` | kill, destroy, take out, shoot, fire at, engage, hit, focus |
| `force attack` | force fire at, fire on, shell, bombard |
| `stop` | halt, hold, hold position, freeze, stand down, cancel that |
| `guard` | defend, protect, watch, hold, cover |
| `guard me` | follow me, cover me, escort me, protect me, on me, stay with me |
| `build` (dozer) | construct, put up, erect, make, place |
| `build` (factory) | train, produce, make, recruit, queue, get me, I need |
| `upgrade` | research, buy, get |
| `sell` | scrap, sell off, demolish |
| `evacuate` | unload, empty, everybody out, get out, drop off, exit |
| `enter` | get in, board, load into, climb into |
| `garrison` | occupy, man, take cover in, hole up in |
| `capture` | take, seize, take over |
| `repair` | fix, patch up |
| `get repaired at` | go get repaired at, get fixed at |
| `get healed at` | go heal at, get patched up at |
| `act on` | deal with, handle, take care of, do your thing on |
| `set rally point` | rally at, gather new units at, send new units to |
| `scatter` | spread out, disperse, break up |
| `form up` | formation, get in formation, group up |
| `use` | launch, fire, call in, drop, activate, deploy |
| `purchase` | promote, unlock, spend points on |
| `place beacon` | mark, ping, put a marker |
| `name them` | call them, these are, they're |
| `make them group` | assign to group, set group, bind to group |
| `then` | and then, after that, afterwards, next, once there |

**Normaliser rules:**

- **A synonym never shadows a run-time name.** If a unit, place or team name equals a synonym, the
  name wins. A test checks the install's data, and the menu warns when a player names a team with
  a synonym.
- **Ambiguous synonyms are tried in turn.** Some phrases map to more than one canonical word:
  `make` (dozer `build` or factory `build`), `hold` (`stop` or `guard`), `take` (`capture`, or
  `enter` with a transport), `fire` (`use` or `attack`). The normaliser produces every candidate.
  The first one whose command resolves (narrowing leaves someone, the target slot fits) is used.
  If several resolve to different orders, the player is asked, using the error path in section 9.
- **"North" is the top of the radar.** Every compass word, synonyms included, means a direction on
  the overhead map as the radar draws it, never the camera's facing. Directions relative to the
  General's facing are only `behind` and `in front of`.
- **The menu shows canonical words only.** Synonyms appear as completions in the typed entry, and
  they are what the voice path is trained on.

## 5. Vocabulary built at run time

Every word list in `<…>` is built from data at the start of a match, and again when it changes.
None is written by hand.

| List | Source |
|---|---|
| `<unit-type>`, `<object-type>`, `<buildable>` | Each thing template's display name from the localized string table, lowercased, with hotkey markers stripped. Plurals use English rules plus an override table in the mod's INI data. |
| `<unit-class>` | KindOf flags: `INFANTRY` → infantry, `VEHICLE` → vehicles, `AIRCRAFT` → aircraft, `STRUCTURE` → buildings, `DOZER` → dozers, `HARVESTER` → supply trucks. The words live in the mod's INI data. |
| `"units"` | Exactly what Zero Hour's select-all key selects (the engine's own rule, to be cited in code once read). |
| `<verb>` | Section 7: command-button labels plus the context verbs. |
| `<power>`, `<science>` | Special-power and science display names. |
| `<place-name>` | The map's named trigger areas and waypoints, if it has any. |
| `<team-name>` | Names this player saved this match with `name them …` |

**Names that collide** (two templates with the same display name, such as faction variants) are
told apart by the player's own faction first. If they still collide, the general's name is added
as a prefix ("laser crusader tank"). A test checks that every name in every list is unique.

**Synonyms are a separate thing from team names.** Synonyms ("Synonyms and aliases" in section 4)
are vocabulary that lasts across matches, and canonical text never contains one. Team names are
per match: each one is a snapshot of IDs, and it is canonical.

## 6. Selection semantics

The steps run in order: **base set → adjectives → filters → set operations → superlative →
quantity**.

- **Base sets** contain only the player's own objects that can be resolved (rule 1), never the
  General.
- **Adjectives** are object state the engine already tracks: idle (the control bar's idle-worker
  test, extended to any unit with no orders), damaged (below full health), veterancy level,
  garrisoned, loaded (a transport with passengers).
- **`near X`** uses a radius from the command tuning keys (section 10). `near` on a selection
  keeps the members within that radius of X.
- **Clusters.** Single-linkage clustering of the candidate set with a fixed link distance (a
  tuning key): two units share a cluster if a chain of units within the link distance joins them.
  Each cluster is named by the compass region (`northern`, …, `central`) from the map centre to
  its centroid. If two clusters get the same name, the one farther along that direction keeps it
  and the other gets the next nearest region. `in my group` is the cluster that contains the General. The menu
  shows the clusters so the player learns what the names mean.
- **Superlatives** sort the set: by position along a compass axis, by distance to the General
  (`nearest`, `farthest`), or by health fraction. Ties break as in rule 5.
- **Quantity.** `the` means 1 and needs a singular noun. A number means that many. `half the`
  rounds up. A plural noun with no quantity means all.
- **`them`** is the last resolved selection, minus any members that have died. `that` is the
  crosshair's object. As a selection it must be the player's own. As a target it may be anything
  resolvable.

## 7. Actions

### From command-set data

Actions come from each object's **current** command set, the one the control bar would show.
Upgrades and state can swap command sets, so the resolver reads the live one.

- **Each button's `Command` type** (`GUICommandType`: `UNIT_BUILD`, `DOZER_CONSTRUCT`,
  `OBJECT_UPGRADE`, `PLAYER_UPGRADE`, `SPECIAL_POWER`, `FIRE_WEAPON`, `SELL`, `EVACUATE`,
  `GUARD*`, `STOP`, `ATTACK_MOVE`, `SET_RALLY_POINT`, `SWITCH_WEAPON`, `TOGGLE_OVERCHARGE`,
  `HACK_INTERNET`, `COMBATDROP`, …) chooses the verb and the message to emit.
- **Its options** choose the target slot: `NEED_TARGET_POS` takes a location,
  `NEED_TARGET_ENEMY_OBJECT`/`ALLY_OBJECT`/`NEUTRAL_OBJECT` take an object reference with that
  relation, and `NEED_SPECIAL_POWER_SCIENCE` makes the verb conditional on the science.
- **Its label** gives the verb's or object's words.
- **Availability** is the engine's own button state. Missing money, missing prerequisites,
  cooldown and a full queue all reject the command with the engine's reason (section 9).

### Context verbs (right-click in the overhead view)

Right-clicking a target makes `CommandXlat` choose an action, using `ActionManager`'s `can*`
queries. Each query becomes an explicit verb so it can be asked for by name, and `act on <object>`
asks the engine to choose, as a right-click does. `go to <object>` is only a synonym of `move to`,
never the right-click: when someone says "go to the oil derrick", they mean move there. An
Engineer capturing the derrick instead would be a surprise.

| Verb (draft wording) | ActionManager query | Message |
|---|---|---|
| `move to` | — | `MSG_DO_MOVETO` |
| `force move to` | — | `MSG_DO_FORCEMOVETO` |
| `attack` / `force attack` | `canFireWeaponAtObject` | `MSG_DO_ATTACK_OBJECT` / `MSG_DO_FORCE_ATTACK_OBJECT` |
| `force attack` (location) | `canFireWeaponAtLocation` | `MSG_DO_FORCE_ATTACK_GROUND` |
| `enter`, `garrison` | `canEnterObject`, `canGarrison` | `MSG_ENTER` |
| `capture` | `canCaptureBuilding` | `MSG_CAPTUREBUILDING` |
| `repair` | `canRepairObject` | `MSG_DO_REPAIR` |
| `resume building` | `canResumeConstructionOf` | `MSG_RESUME_CONSTRUCTION` |
| `get repaired at`, `get healed at` | `canGetRepairedAt`, `canGetHealedAt` | `MSG_GET_REPAIRED`, `MSG_GET_HEALED` |
| `dock at`, `deliver supplies to` | `canDockAt`, `canTransferSuppliesAt` | `MSG_DOCK` |
| `hijack`, `convert to car bomb` | `canHijackVehicle`, `canConvertObjectToCarBomb` | `MSG_ENTER`, `MSG_CONVERT_TO_CARBOMB` |
| `snipe` | `canSnipeVehicle` | `MSG_SNIPE_VEHICLE` |
| `hack`, `steal cash from`, `disable` | `canDisableVehicleViaHacking`, `canDisableBuildingViaHacking`, `canStealCashViaHacking` | `MSG_DISABLEVEHICLE_HACK`, `MSG_DISABLEBUILDING_HACK`, `MSG_STEALCASH_HACK` |
| `sabotage`, `cut power to`, `bribe`, `make defector`, `pick up` | `canSabotageBuilding`, `canCutBuildingPower`, `canBribeUnit`, `canMakeObjectDefector`, `canPickUpPrisoner` | per the engine's mapping in `CommandXlat` |
| `salvage` | — | `MSG_DO_SALVAGE` |

The exact wording is still a draft. The rule is fixed: one verb per `can*` query, and the message
emitted is whatever `CommandXlat` emits for that case.

### Orders bound to hotkeys, and player-level orders

| Overhead input | Command |
|---|---|
| Scatter, formation, cheer | `scatter`, `form up`, `cheer` (`MSG_DO_SCATTER`, `MSG_CREATE_FORMATION`, `MSG_DO_CHEER`) |
| Ctrl+digit, digit | `make them group N`, `group N` as a selection (`MSG_CREATE_TEAMn`, `MSG_SELECT_TEAMn`) |
| Waypoint mode, shift-queued orders | `then` |
| Beacons | `place beacon <location>` (`MSG_PLACE_BEACON`) |
| Generals' promotions | `purchase <science>` (`MSG_PURCHASE_SCIENCE`) |
| Shortcut special powers, superweapons | `use <power> …`, with the owning building as the implied selection |

### Narrowing and implied selection

- **Narrowing by capability.** If the selection is mixed, the order goes to the members that can
  carry it out (`units build power plant` → the dozers). Orders the engine already filters per
  unit, such as attack-move, are passed through unchanged. If nobody can carry out the order, it
  is rejected (section 9).
- **Implied selection.** An order with no selection gets the capable set the order implies:
  - production: the factories that can build it, shortest queue first, splitting a count across
    them;
  - construction or repair: the nearest idle capable dozer, otherwise the nearest capable dozer;
  - a power: its owning building;
  - anything else: the last selection (`them`).
- **One target or many.** Object slots that take one object (`attack`, `enter`, `repair`) resolve
  a plural or ambiguous reference to the candidate nearest the General. Slots that take many
  (`garrison` across several buildings) are marked as such in the verb table.
- **Placement.** `build X near Y` asks the engine's legality check for the nearest legal spot to
  Y, searching outward. `facing` sets the building's angle. Without it, the engine's default
  angle is used.

## 8. Front ends

- **Scroll-wheel menu.** A held or toggled command mode (key still to be chosen) borrows the
  wheel and a confirm key. Movement and aiming stay live. The menu walks the grammar. Each step
  lists only valid completions, with live counts ("rangers (12)", "the northernmost dozer (1)").
  While a command is being built, the units it would select are outlined in the world, and the
  canonical text so far is shown. The menu is how players learn the language they will later
  speak.
- **Typed entry.** A text line using the engine's text input, with valid-completion suggestions.
  It's the power-user path and is what the headless tests drive.
- **Voice.** Section 11.

Every command is echoed after parsing, in a short form ("12 rangers → attack-move → north
ridge"), so a misparse is visible before or as it runs.

## 9. Errors and confirmation

Several checks combine to decide what the player is told:

| Source | Example message |
|---|---|
| Parse or translation failure; the model takes the grammar's abstain production | "Didn't understand *'flank them'*." |
| The meaning mask or resolver finds no such object (`<not found: overlord>`) | "No Overlord visible." / "You have no dozers." |
| Narrowing leaves nobody | "None of those units can build a Power Plant." |
| The engine's button availability says no | The engine's own reason, the same text as the overhead tooltip |
| Low confidence (voice; section 11) | Asks for confirmation: "Rangers attack-move to north ridge?" |

The abstain production has a slot that may name any word from the full roster vocabulary, not
just what is resolvable now. That lets a failed command say what it couldn't find.

## 10. Tuning

New gameplay-feel numbers go in the tuning pack (CONFIG.md, `tuning.rs` + `tuning.ini`), each
with its reason (there is no BO2 source, so the comment says it is a mod rule):

- `CommandNearRadius`: the radius for `near`.
- `CommandOffsetDistance`: how far `north of X` is from X.
- `CommandClusterLink`: the clustering link distance.
- `CommandLowConfidence`: the voice confirmation threshold.

These are client-side numbers and never reach the sim. Even so, they belong in the tuning pack,
like every other gameplay number.

## 11. Voice (later)

Voice runs entirely on the player's machine.

- **Speech recognition.** A Whisper-class model, prompt-biased each match with the run-time
  vocabulary (unit, place and team names), so proper nouns are heard correctly ("oil derrick",
  not "oil Derek").
- **Translation.** A small generative model translates the recognised text into canonical
  language. Two kinds will be prototyped and measured on the same data:
  - a T5-style encoder-decoder (about 60–250M parameters), which is quick and suited to narrow
    translation;
  - a small decoder-only model, which can also take instructions and a short, fixed-format
    situation summary (own unit types and counts, the last selection, nearby place names) built
    from the resolver's data.

  The model runs on the GPU if there is room, otherwise on the CPU or the Apple Neural Engine.
  Commands are 10–30 tokens, so either is fast enough. Inference runs from Rust (candle or ONNX
  Runtime) with our own logit hook.
- **Constrained decoding.** At each step the logits of every token the grammar does not allow are
  set to −∞:
  - The grammar is precompiled against the model's token list into a state machine. The fixed
    skeleton is compiled once and shipped. Each run-time word list is a token trie, rebuilt only
    when its list changes.
  - The mask enforces meaning as well as syntax, through the valid-completions function: only
    buildable things after `build`, only existing groups after `group`.
- **Guarding against valid-but-wrong output.** Three guards, used together:
  1. **The abstain production** with its not-found slot (section 9).
  2. **A confidence check.** The command's log-probability and the probability mass the mask
     removed along the way. Too much removed means the model "wanted" something invalid, so the
     command asks for confirmation instead of running.
  3. **Fine-tuning on the canonical language**, so the mask is a safety net, not the main
     mechanism.
- **Training data** comes from the grammar:
  1. A Rust sampler walks the grammar and writes ASTs with their canonical text, weighted toward
     common commands.
  2. A subagent writes many natural paraphrases of each line. The labels stay correct because
     they came from the sampler.
  3. A back-translation pass drops pairs whose paraphrase no longer means the label.
  4. Code adds speech-recognition noise (homophones, dropped words, number confusions) so it can
     be reproduced.

  Training uses **placeholder names** in place of real unit and place names, so the model learns
  to copy names instead of memorising Zero Hour's roster. That generalises to mods and other
  factions, and it keeps Zero Hour content out of the weights (the asset rule: nothing from
  either game ships). Evaluation is exact match on the resolved AST, using a held-out synthetic
  set and a small set of real recorded speech, with greedy decoding.

## 12. Tests

- **Round trip.** A property test: random ASTs print and parse back equal. A random canonical
  string parses and prints back identical.
- **Vocabulary.** No run-time name equals a verb or fixed word. Every name is unique within its
  list. Run over the install's data in the real-install suite.
- **Resolver.** Selection, filters, clusters, superlatives, implied selection, narrowing, fog and
  ghost rules over a fake world in the headless tests.
- **Coverage gate (constraint 1), in two parts:**
  1. Every `CommandButton` in the install's command sets can be produced as a command.
  2. Every message type in `MessageStream.h` that a player can issue as an order has a command
     that emits it. Message types that are not orders (hints, system, camera, debug) are
     classified in code, each with its reason, the way `state_hash` skips are.
- **Lockstep.** A command issued from first person produces the same message stream, and the same
  state hash, as the equivalent overhead clicks in the harness.
- **In game.** An autotest leg that issues typed commands (move, build, produce, attack) and
  checks the engine's orders.

## 13. Build order and open questions

**Build order:**
1. AST, grammar, parser and printer, the round-trip test, and typed entry with base sets and
   superlatives only.
2. The world-query services, emitting messages, and move, attack-move, stop and attack.
3. Actions from command-set data and the context verbs, with the coverage gate.
4. The scroll-wheel menu over valid completions.
5. Adjectives, filters, clusters, place names, team names and synonyms.
6. Voice: the sampler and dataset, the model prototypes, constrained decoding.

**Open questions:**
- Which key opens the command mode, and whether it's held or toggled.
- How a distance is spoken: coarse words only (`near`, `north of`), or numbers too, and in what
  unit.
- The exact wording of the context verbs in section 7.
- Whether the General's own field-computer orders (CONTRACT.md "The General under RTS control")
  get command forms. That is the exception in constraint 1, so the current answer is no.
