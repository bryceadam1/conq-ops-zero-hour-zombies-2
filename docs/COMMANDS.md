# First-person commands

How the General commands his army without leaving first person.

**Status (2026-11-08):** all six steps of the build order are built (section 13): the language,
the typed command line and the command menu in first person, the resolver, the command buttons and right-click verbs,
the coverage gates (`crates/zhbo-core/src/command/`, `host/zhbo/ZhboCommand.cpp`,
CONTRACT.md "First-person commands"), and voice: push-to-talk through Whisper, the snapper and the
fine-tuned translator (`crates/zhbo-voice`, section 11). The decisions below were agreed with the user. Section 13 lists
what is still open.

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

   A building seen once can be selected or targeted while fogged. The bridge counts an object as
   a ghost when the player's shroud status for it is fogged and it holds a ghost object. Units in
   fog or shroud cannot be resolved. A building destroyed while fogged still shows as its ghost
   overhead, but it has no object left to give an order, so the resolver cannot name it: an
   accepted gap, since an order at it would do nothing either.
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

### Grammar

This is the grammar `crates/zhbo-core/src/command/parse.rs` implements. `"x"` is a fixed word,
`<x>` is a word list built at run time (section 5), `[x]` is optional, `{x}` repeats.

```
command     = [selection] order {"then" order}

order       = verb-phrase
            | "name" "them" new-name                ; save the selection as a team name
            | "make" "them" "group" digit           ; engine control group (MSG_CREATE_TEAMn)

selection   = group {"and" group} ["except" group {"and" group}]
group       = query                                 ; never says whose: always the player's own
            | "group" digit                         ; engine control group
            | <team-name>                           ; "alpha team"
            | "them"                                ; the last resolved selection
            | "that"                                ; the object under the crosshair

query       = [determiner] [superlative] {adjective} [relation] noun {filter}
determiner  = "the" [number] | number | "half" "the"
            | "my" [number] | "half" "my"           ; targets only: "my" is the relation too
superlative = "nearest" | "farthest" | "northernmost" | "southernmost" | "easternmost"
            | "westernmost" | "weakest" | "healthiest"
adjective   = "idle" | "damaged" | "veteran" | "elite" | "heroic" | "garrisoned" | "loaded"
relation    = "enemy" | "allied" | "neutral" | "civilian"     ; targets only
noun        = <unit-type> | <unit-class> | "unit" | "units"
filter      = "near" point                           ; that point has no filters of its own
            | "in" "the" region "group"              ; cluster (section 6)
            | "in" "my" "group"                      ; the cluster the General is in

verb-phrase = <verb> [thing] [target]                ; as the verb's entry says (section 7)
thing       = <buildable> | number <buildable-plural>          ; "build", "unload", counted
            | <upgrade> | <power> | <science>        ; "upgrade", "use", "purchase"
            | <weapon-mode> | <weapon>               ; "switch to", "fire"
            | <buildable> | <upgrade>                ; "cancel"
target      = [prep] point ["facing" direction]      ; prep: the verb's ("to", "at", "on")
            | offset point ["facing" direction]      ; no prep before an offset
            | "there" ["facing" direction]           ; no prep before "there"
            | "from" location "to" location          ; a line (a wall), where the verb builds one

offset      = "near" | direction "of" | "behind" | "in" "front" "of"
point       = "me" | "there" | "that" | <place-name> | query
direction   = "north" | "south" | "east" | "west" | "northeast" | "northwest"
            | "southeast" | "southwest"
region      = "northern" | "southern" | "eastern" | "western" | "northeastern"
            | "northwestern" | "southeastern" | "southwestern" | "central"
number      = digits, no leading zero                ; counts are 2 or more
new-name    = word {word}                            ; up to "then" or the end
```

Each verb's entry (`command/vocab.rs` `VerbDef`) gives its words, its preposition, whether it
names a thing (from which list, with or without a count), and its target slot: an object, a point
or either, required or optional, and whether `facing` may follow. An object-only slot takes `me`,
`that` or a query, with no offset. In a point slot, a named object stands for its position.

Spelling rules, so that every command has exactly one spelling:

- **Selection first, always.** "Build a power plant with the north dozer" is input for the
  translator or the normaliser. Its canonical form is
  `the northernmost dozer build power plant ...`.
- **The selection ends at the first verb.** Run-time names must never equal a verb or a fixed
  word, or contain `then`. `Vocabulary::problems` checks this, and a gate will run it over the
  install's data (section 12). The parser keeps every complete parse, so any remaining ambiguity
  is an error, never a silent guess.
- **Quantity comes from the determiner and the noun's number:**
  - A bare plural means all (`rangers`). `all` is accepted input only, which the normaliser drops.
  - `the` takes a singular: `the ranger`.
  - A number takes a plural, and is 2 or more: `3 rangers`. One is spelled `the`.
  - `half the` takes a plural.
- **A superlative needs `the` or `my`:** `the nearest ranger`, `the 3 nearest rangers`,
  `my nearest war factory`. A bare number takes no `the` (`3 rangers`), and `the` with a number
  needs a superlative.
- **Whose.** A selection never says whose. A target may say `my` (in the determiner's place:
  `my war factory`, `my 2 war factories`, `half my war factories`) or a relation word after the
  adjectives (`the nearest damaged enemy tank`). Saying neither means anyone's.
- **Mass nouns** (the same word for singular and plural, like `infantry` or `aircraft`) are
  singular only after `the` or with a superlative. `my infantry` means all of them.
- **Adjectives** come in the fixed order above, each at most once.
- **Filters** come in the order above, each at most once. The point after `near` has no filters,
  so a trailing filter always belongs to the noun before `near`:
  `rangers near the war factory in the northern group` is the rangers in the northern group.
- **Prepositions.** A verb's preposition is left out before `there` and before an offset, which
  is a preposition itself: `move there`, `move north of the bridge`, `build power plant near my
  war factory`. Otherwise it is spoken: `move to the bridge`.
- **`then` chains orders for the same selection**, using the engine's queued orders (shift-click,
  `MSG_ADD_WAYPOINT`). There are no conditionals.
- **"here" means "me".** The normaliser rewrites it, and canonical text prints `me`.

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
| `the nearest dozer build america wall from me to there` | `MSG_DOZER_CONSTRUCT_LINE` from the General to the crosshair's spot (a `LINEBUILD` template). |
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

The tables are data, not code (built: `data/ConquerOps/commands.ini`, read by
`command::normal`, which also builds the same file in as its default). Three kinds of entry:

1. **Fixed-word synonyms** (`[Words]`): `phrase = reading | other reading | …`, each reading in
   canonical words, `<none>` to drop the phrase. A phrase that is canonical words itself (`make`,
   `on me`) lists itself first.
2. **Aliases** (`[Aliases]`): `name = plural, plural, …`, a word for several types or classes as
   their canonical plurals (`tanks = crusaders, paladins, laser tanks, battlemasters, overlords,
   emperors, scorpions, marauder tanks`, the install's English display names). Only the ones the
   match has count, so one line serves every faction. An alias stands for all of them where a bare
   plural goes (`idle tanks stop` → `idle crusaders and idle paladins stop`); after a determiner
   (`the nearest tank`) it stays as said, since the grammar has no one-of-several noun.
3. **The player's own synonyms**: `ConquerOps.commands.ini` beside the profile in Zero Hour's
   user-data folder, in the same sections, read after the mod's file (its entries win). The bridge
   loads both at init (`zhbo_load_command_words`). These last across matches. Team names
   (`name them …`) are different: they are per match and canonical.

**How a line is read** (`command::session::run`): the line as typed first; if it is canonical and
resolves, nothing else is tried. Then each combination of its phrases' readings (at most 32), with
aliases expanded. The first that parses and resolves is used, and the echo shows its canonical
text, so the player learns the language. If two readings resolve to different commands, the
player is asked ("that could mean 'rangers stop' or 'rangers guard': say which").

**The shipped tables' first entries** (the full set is the file). The canonical word is on the
left. They will grow as real speech comes in.

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

- **A synonym never shadows a run-time name.** Before it looks a phrase up, the normaliser keeps
  any stretch of the line that is one of the match's names (a type's or class's either form, a
  place, a team), longest first.
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
| `<unit-class>` | Grammar words tied to the bridge's KindOf flags (`command/world.rs` `CLASSES`): `INFANTRY` → infantry, `VEHICLE` → vehicles, `AIRCRAFT` → aircraft, `STRUCTURE` → buildings, `DOZER` → builders, `HARVESTER` → harvesters. `builders` and `harvesters` rather than `dozers` and `supply trucks`, because those are display names (the USA's Dozer, the Supply Truck); a type name equal to a class word wins over it, and the synonyms table maps `dozers` and `supply trucks` to the classes. |
| `"units"` | Exactly what Zero Hour's select-all key selects (the engine's own rule, to be cited in code once read). |
| `<verb>` | Section 7: command-button labels plus the context verbs. |
| `<power>`, `<science>` | Special-power and science display names. |
| `<place-name>` | The map's named areas (built): its polygon triggers that are not water (`ZhboClientApi.map_places`), their identifiers split at underscores and case changes (`InnerPerimeter_1` → `inner perimeter 1`), a name that is a grammar word or a unit's left out and logged. A place stands for the area's centre. Waypoints are left out: on skirmish maps they are the AI's paths and the start positions. Alpine Assault names six: combat zone, inner perimeter 1 and 2, outer perimeter 1 and 2, skirmish world. |
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
- **Clusters (built, `command::cluster`).** Single-linkage clustering of the query's candidates
  (after its type, adjectives and `near`) with the link distance `CommandClusterLink`: two units
  share a group if a chain of them, each within the link of the next, joins them. Each group is
  named by the compass region (`northern`, …, `central`) of its centre from the map's centre (the
  logic's extent); within `CommandCentralShare` of the map's half-size it is central. Two groups
  never share a name: the one farther out keeps it and the other takes the free region nearest its
  bearing. `in my group` is the group the General joins, as one more point in the clustering. Not
  built yet: showing the groups in the menu.
- **Superlatives** sort the set: by position along a compass axis, by distance to the General
  (`nearest`, `farthest`), or by health fraction. Ties break as in rule 5.
- **Quantity.** `the` means 1 and needs a singular noun. A number means that many. `half the`
  rounds up. A plural noun with no quantity means all.
- **`them`** is the last resolved selection, minus any members that have died. `that` is the
  crosshair's object. As a selection it must be the player's own. As a target it may be anything
  resolvable.

## 7. Actions

Every verb and the button or message behind it is in one table, `command/actions.rs`: `BUTTONS`
lists each engine command type (`GUICommandType`) and `MESSAGES` each message a player's machine
sends the logic, each with the verb or grammar that produces it, or why it is not a player's order.
The resolver reads which buttons a verb presses from there (section 12 holds it to the engine).

### From command-set data (built)

The bridge lists every button of the player's command sets (`ZhboClientApi.command_buttons`):
each of his objects' **current** command set, the one the control bar shows when the object is
selected, and his own sets, the special-power shortcuts and the Generals' promotions (rank 1, 3
and 8). The General's own set is left out: his actions are first person's (constraint 1's
exception). Each button carries:

- **its command type**, which picks the verb (`actions::BUTTONS`);
- **its options**: `NEED_TARGET_POS` takes a spot, the `NEED_TARGET_*_OBJECT` options and
  `CONTEXTMODE_COMMAND` take a unit or building;
- **its thing**: the template it builds, or the name it gives (an upgrade's display name, else the
  button's label: a power, a promotion, a weapon mode, a weapon);
- **its availability**, `ControlBar::getCommandAvailability` now. A command whose buttons are all
  unaffordable, not ready or restricted is refused with that reason ("you can't afford to upgrade
  composite armor").

The names join the vocabulary's lists (`upgrades`, `powers`, `sciences`, `modes`, `weapons`), and
the templates the buttons build join the type names, so `build crusader tank` works before any
Crusader is on the map.

| Verb | Buttons | What is sent |
|---|---|---|
| `build [N] <type> [placement]` | `UNIT_BUILD` | One press per unit at the producer with the shortest queue, ties to the nearer (`MSG_QUEUE_UNIT_CREATE`) |
| | `DOZER_CONSTRUCT` | The nearest idle builder (else the nearest), `MSG_DOZER_CONSTRUCT` at the nearest legal place to the spot named (else the builder), searched outward with the overhead placement's own legality flags (`PlaceEventTranslator`); `facing` sets the angle, else the template's `PlacementViewAngle` (`InGameUI::placeBuildAvailable`) |
| `build <wall> from <place> to <place>` | `DOZER_CONSTRUCT` of a `LINEBUILD` template | `MSG_DOZER_CONSTRUCT_LINE` from one end to the other (the walls and the GLA's burning barricade) |
| | `SPECIAL_POWER_CONSTRUCT[_FROM_SHORTCUT]` | `MSG_DO_SPECIAL_POWER_AT_LOCATION` with the angle, at a legal place, as `PlaceEventTranslator` sends it |
| `upgrade <upgrade>` | `PLAYER_UPGRADE`, `OBJECT_UPGRADE` | `MSG_QUEUE_UPGRADE` (a player upgrade at one producer; an object upgrade on every unit named that has it) |
| `use <power> [target]` | `SPECIAL_POWER[_FROM_SHORTCUT]` | The shortcut first (it fires from the most ready building), else the nearest owner; a power with a target is finished as the world click would be |
| `purchase <science>` | `PURCHASE_SCIENCE` | `MSG_PURCHASE_SCIENCE` of the first rank the player can buy |
| `cancel <type or upgrade>` | the queue | The last one queued at the first producer that has one (`MSG_CANCEL_UNIT_CREATE` by production id, `MSG_CANCEL_UPGRADE`) |
| `unload [N] <type>` | `EXIT_CONTAINER` | `MSG_EXIT` of that many such passengers from the containers named |
| `switch to <mode>`, `fire <weapon> [target]` | `SWITCH_WEAPON`, `FIRE_WEAPON` | `MSG_SWITCH_WEAPONS`, `MSG_DO_WEAPON` (or the world click for an aimed weapon) |
| `sell`, `evacuate`, `overcharge`, `hack internet`, `depart`, `cancel construction` | `SELL`, `EVACUATE`, `TOGGLE_OVERCHARGE`, `HACK_INTERNET`, `EXECUTE_RAILED_TRANSPORT`, `DOZER_CONSTRUCT_CANCEL` | The button's message, from each unit named that has it |
| `combat drop <target>` | `COMBATDROP` | The world click with the button pending |

**How a press is sent.** An untargeted button sends what `ControlBar::processCommandUI` sends for
its type (ported case by case, with its checks: money, queue, single-use buttons). A button that
needs a target is set as the pending GUI command and finished by
`CommandTranslator::evaluateContextCommand` at the target, the code a world click runs. The bridge
owns its own `CommandTranslator`; the call reads only the UI's selection and the pending command.
A button that uses the mine-clearing weapon set sends `MSG_SET_MINE_CLEARING_DETAIL` first, as the
control bar does.

### Right-click verbs (built)

`act on <unit, building or spot>` is the right-click: `evaluateContextCommand` decides, as it does
overhead (on open ground: a move, a rally point, a special power's new destination). The other
verbs name one thing the right-click can do. The bridge evaluates the right-click first without
sending anything; if what it would do is not one the verb allows, the command is refused ("that
would do something else"). Only then is it sent.

| Verb | Allowed outcome of the right-click |
|---|---|
| `act on` | anything |
| `enter`, `garrison`, `hijack` | `MSG_ENTER` |
| `capture` | `MSG_CAPTUREBUILDING`, or a special power (a hacker's capture) |
| `repair` | `MSG_DO_REPAIR` |
| `resume building` | `MSG_RESUME_CONSTRUCTION` |
| `get repaired at`, `get healed at` | `MSG_GET_REPAIRED`, `MSG_GET_HEALED` |
| `dock at`, `deliver supplies to` | `MSG_DOCK` |
| `car-bomb` | `MSG_CONVERT_TO_CARBOMB` or `MSG_ENTER` |
| `sabotage` | `MSG_ENTER`, a special power, or another action |
| `salvage` | `MSG_DO_SALVAGE` |
| `disable`, `steal cash from` | a hack (`MSG_DISABLEVEHICLE_HACK`, `MSG_DISABLEBUILDING_HACK`, `MSG_STEALCASH_HACK`) or a special power |
| `set rally point at <spot>` | `MSG_SET_RALLY_POINT` (the right-click on open ground with a factory selected) |

`hijack`, `car-bomb` and `sabotage` are also context-mode buttons; where the units named have the
button, it is pressed at the target instead. The right-click verbs the first draft listed for
`ActionManager`'s other queries (`snipe`, `cut power`, `bribe`, `convert`, `pick up`) are not
right-clicks in Zero Hour: they are special-power buttons (`use <power> on …`), or compiled out
(`ALLOW_SURRENDER`). The engine sends `MSG_SNIPE_VEHICLE` from nowhere.

`go to <object>` is only a synonym of `move to`, never the right-click: when someone says "go to
the oil derrick", they mean move there. An Engineer capturing the derrick instead would be a
surprise.

### Direct orders and hotkeys (built)

| Overhead input | Command |
|---|---|
| Right-click on the ground, attack, force attack, stop | `move to`, `force move to`, `attack-move to`, `attack`, `force attack`, `stop` |
| Guard buttons | `guard`, `guard without pursuit`, `guard against aircraft` (`MSG_DO_GUARD_POSITION`/`OBJECT` with the guard mode; with no target, where they stand) |
| Scatter, formation, cheer | `scatter`, `form up`, `cheer` (`MSG_DO_SCATTER`, `MSG_CREATE_FORMATION`, `MSG_DO_CHEER`) |
| Ctrl+digit, digit, Shift+digit | `make them group N`, `group N`, `and group N` |
| Waypoint mode, shift-queued moves | `then` (moves only: Zero Hour queues waypoints, nothing else) |
| Beacons | `place beacon at <spot>` (under the multiplayer settings' cap, as `MSG_META_PLACE_BEACON` allows), `remove beacon` |

Not orders, and so not in the language (`actions::MESSAGES` gives each reason): deselecting,
closing a pop-up, a beacon's chat text, the replay camera, the quit menu's surrender, the logic
CRC, retaliation mode (a game option).

### Implied selection, and who acts

- **Implied selection.** An order with no units named:
  - `build`, `upgrade`, `use`, `purchase`, `cancel`: every object of the player's that has the
    button (the player's own sets included), then the rules in the table above;
  - anything else: the last selection (`them`).

  The units that acted become `them` for the next command.
- **Narrowing.** A button verb goes to the members of the selection that have the button; if none
  has it, the command is refused ("none of them can build crusader tank"). Orders the engine
  filters per unit itself, such as attack-move, go to the whole selection.
- **One target.** A target that names several objects resolves to the one nearest the General.
- **What the engine refuses** comes back from `issue_order` (`ZHBO_ISSUED_*`: no room to build,
  the queue is full, the target does not suit the button, the right-click would do something
  else, nothing like that to cancel). A command none of whose orders went out is refused and
  changes nothing; a partial one says how many went out.

## 8. Front ends

- **Scroll-wheel menu (built).** Z (the `command_menu` binding) toggles it in first person. It
  builds a command a word or phrase at a time from the grammar's valid completions
  (`command::parse::completions`): the wheel or Up/Down walks the options, the left button or
  Enter picks one, the right button or Backspace takes the last pick back, and `(done)`, first in
  the list once the picks are a whole command, runs it. Escape or Z closes it. While it is up it
  borrows the wheel and the mouse buttons (no weapon swap, fire or aim); movement and look stay
  with first person. Names show how many of the player's objects they name ("rangers (12)"). The
  options are recomputed after the frame each time the picks change, from a fresh world snapshot
  (a few milliseconds over a full match's vocabulary). Completions validate one representative of
  each group a list's names form (a noun's singulars, plurals, mass nouns), since names are atomic
  in the grammar. Not built yet: outlining in the world the units the command would select.
  The menu is how players learn the language they will later speak.
- **Typed entry (built).** In first person, Enter opens a line at the lower left, where BO2 puts
  chat. Letters, digits, space and `-` type, Backspace deletes, Enter submits and Escape cancels.
  While the line is up it holds the keyboard: no movement, look, map or scoreboard, and Escape does
  not open Zero Hour's menu. It reads the platform key set (`Key`), not the engine's text input,
  like the class screen's name field. Valid-completion suggestions come with step 4.
- **Voice.** Section 11.

Every command is echoed after it runs, under the line: the count and the canonical text
("3 units: rangers move to me") in green, or the line and why it was refused in red, fading out
after five seconds. The echo is English for now; it moves into the string table with the menu.

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

- `CommandNearRadius` (150): the radius for `near`.
- `CommandOffsetDistance` (150): how far `north of X` is from X.
- `CommandClusterLink` (120): the clustering link distance.
- `CommandCentralShare` (0.25): how much of the map's half-size the central group spans.
- `CommandLowConfidence` (with voice): the confirmation threshold.

These are client-side numbers and never reach the sim. Even so, they belong in the tuning pack,
like every other gameplay number.

## 11. Voice

Voice runs entirely on the player's machine. **Decided (2026-11-07):** Whisper for recognition,
downloaded by the launcher at install, never bundled (it is MIT-licensed, code and weights);
flan-t5-small as the first translation model; candle as the runtime. **Built:** the training set,
the fine-tuning script, the translator and the constrained decoder, Whisper with the snapper, the
`conquerops-voice` process with the microphone, push-to-talk in game, and the launcher's download
(`crates/zhbo-voice`, `zhbo-launcher`'s `voice`), measured below.

- **Speech recognition (built, `zhbo_voice::hear`).** Whisper base.en, prompted each utterance
  with the match's names (`hear::prompt`, Whisper's "previous text"), so proper nouns come out
  spelled as the vocabulary spells them.
  - **The model.** base.en over small.en: after snapping both hear about 90% of names whole, but
    small.en takes 940 ms against 351 ms on the CPU and 925 MB against 279 MB (`voice_listen`,
    120 spoken commands). tiny.en heard too little.
  - **Speed, our own network (`zhbo_voice::whisper`).** candle's Whisper caches only the
    cross-attention, so with a prompt of a hundred-odd names every step re-read the whole
    sequence: 1.1 s of decoding. Our port of the network over the same weights caches the
    self-attention too (the prompt is read once): 0.4 s, the same transcriptions. The encoder
    sees the audio plus 3 s of silence instead of Whisper's full 30 s window
    (`hear::WINDOW_SILENCE`, whisper.cpp's `audio_ctx` idea): 380 ms down to 95 ms, with nothing
    lost in what was heard. The faster-whisper and whisper.cpp ports were considered: neither is
    Rust, and the two changes above were where our time went.
  - **The filterbank** is computed (librosa's Slaney mel, as OpenAI ships it precomputed) and
    matches transformers' to the last bit (`the_mel_filters_are_whispers`).
  - **The log-mel is openai's** (`hear::log_mel`, `hear::window_samples`): a centred,
    reflect-padded periodic-Hann STFT over the audio plus 3 s, padded to a whole number of frame
    pairs (`the_log_mel_is_openais`). candle's `pcm_to_mel` frames the audio differently: its
    encoder states differed from the reference by about 17% of their size (cosine 0.987); now they
    agree to bf16 rounding (cosine 0.999999), which a bridge trained on reference features needs.
    The pipeline's own score moved within noise (0.783 to 0.787 spoken).
- **Snapping (built, `zhbo_voice::snap`).** Whisper writes what a name sounds like (`velgal team`
  for `belcal team`, `enami`, `play spican` for `place beacon`), and the translator, which copies
  names, then guesses. The snapper respells each word it does not know into the nearest-sounding
  phrase it does: the match's names (a place's too without its `the`), the grammar's words and
  verbs, the synonym table's phrases and aliases.
  - Known: those phrases' words, numbers, and the **ordinary words**, every word of the training
    inputs outside names that the sampler could not have invented (`sample::is_invented`), seen
    twice: `words.txt`, written by `voice_corpus assemble` and shipped with the translator.
  - The distance weighs sounds: a letter for one in the same class (b/p/v/f; c/k/g/s/…; d/t; m/n;
    l/r), or a vowel, `h` or `w` put in or left out, costs half. A span of up to four words around
    an unknown word is respelt when its distance per letter of the span's *unknown* words is at
    most 0.34 (dividing by the whole span let a short word vanish beside a long name). An unknown
    hyphenated word (`a-8-korselvases`) is read as its parts.
  - **Whisper's loop.** Whisper can write a phrase again and again past what was said
    (`attack move to the bridge` nine times). The decoder stops at ten tokens a second of audio,
    and a run of three words or more said again straight after itself is kept once
    (`hear::unrepeat`).
  - Measured (`voice_listen`, base.en): names heard whole 0.58 prompted, 0.90 snapped; the
    command's meaning right 0.48 and 0.63.
- **Translation: flan-t5-small first** (77M parameters, Apache-2.0), fine-tuned on the training
  set; flan-t5-base (250M) is the step up if it falls short, and a decoder-only model of about
  0.5B (Qwen-class) the comparison to run on the same data. Why this order:
  - The task is narrow: mostly copying names and reordering words into a fixed grammar. Recent
    small-model work finds that grammar-constrained decoding makes every output well formed, but
    meaning still depends on size and fine-tuning; fine-tuning plus constraints is the strongest
    pairing for small models (arXiv 2609.23742, SLOT arXiv 2505.04016). For a decoder-only 0.5B
    model, inference-time search only partly closed the gap on a harder constrained task
    (arXiv 2608.25761).
  - An encoder-decoder reads the sentence once and decodes 10–30 tokens: fast on a CPU, and small
    (about 300 MB in f32, a quarter of that quantised), which matters beside a running game.
  - A decoder-only model's advantage, a situation summary in its prompt, is worth measuring once
    the small model's errors are known.

  The model runs on the GPU if there is room, otherwise on the CPU.
- **Runtime: candle.** Pure Rust (the project's rule), with T5, Whisper and Qwen implementations,
  Metal on Apple Silicon and the decoding loop in our own code, which the mask needs. ONNX Runtime
  (`ort`) was the alternative; it would mean shipping its native libraries. The weights are read as
  safetensors, downloaded at install like Whisper's.
- **Constrained decoding (built, `zhbo_voice::constrain`).** At each step only the tokens the
  grammar allows may be chosen:
  - At each word boundary the grammar's completions (`completions_with`, the menu's function with
    every count from 2 to 50) are tokenized into a trie, built once per prefix and cached.
  - The mask enforces meaning as well as syntax, through the valid-completions function: only
    buildable things after `build`, only existing groups after `group`.
  - A name the player gives (`name them …`) has no list: `Completions::name` opens the slot and
    any word may be spelled there, token by token; the grammar decides where it may end.
  - One token can mean two things. In T5's tokenizer an unfamiliar word starts with a bare `▁`, so
    after `the` the `▁` may go on into the place `the torwren crossing` or start `korgor tower`.
    The mask follows every reading at once and allows what any of them allows. Following only one
    cost 1.4 points of exact match until it was fixed; `voice_eval` now forces every label
    through the mask and counts refusals (there are none).
- **Guarding against valid-but-wrong output.** Three guards, used together:
  1. **The abstain production** with its not-found slot (section 9).
  2. **A confidence check.** The command's log-probability and the probability mass the mask
     removed along the way. Too much removed means the model "wanted" something invalid, so the
     command asks for confirmation instead of running. Measured (the released model, `voice_eval`):
     the largest share removed at one step averages 0.000 on right answers and 0.27 on wrong ones;
     asking above 0.1 asks on 0.1% of right answers and catches 31% of the wrong ones.
  3. **Fine-tuning on the canonical language**, so the mask is a safety net, not the main
     mechanism.
- **The game's side (built).** `conquerops-voice` is its own process beside the game (the
  models' memory and threads, and any crash, stay out of the engine; candle stays out of its
  build). The game starts it the first time the General is in first person and talks to it over
  pipes, one tab-separated line per message (the protocol is in the binary's docs): `vocab`
  (`Vocabulary::to_line`, sent when it changed), `listen`/`stop` around the push-to-talk key,
  `hear-file` for the autotest; back come `ready`/`unavailable` and
  `heard <masked> <stuck> <no speech> <transcript> <command>`.
  - **Push-to-talk:** hold T (`Bindings::voice`) in first person; never while the line or the
    menu is up. The echo says "Listening...".
  - **What happens to it** (`command::voice::act`): a command whose largest masked share is at
    most `CommandVoiceGuard` (0.1) runs as if typed; above it, it waits in the command line for
    Enter (Escape drops it); nothing runs when Whisper's no-speech probability is above
    `CommandVoiceSilence` (0.6, OpenAI's `no_speech_threshold`) or the mask left nothing.
  - Presentation only: what comes back is a line the session resolves like a typed one, and
    only the orders' messages reach the sim.
- **The download (built, `zhbo-launcher` `voice`).** `data/ConquerOps/voice.ini` lists each
  model's folder and files with their URLs, sizes and SHA-256s: Whisper pinned to a Hugging Face
  commit, the translator as this project's GitHub release assets. At launch, if a file is
  missing, the launcher starts itself again with `--fetch-voice` in the background and starts the
  game; the fetch uses the system's `curl` into `<name>.part`, checks size and hash, and renames,
  so a file under its own name is whole. Without the models the game plays as before and the
  push-to-talk key says voice is off.
- **Training data (built, `tools/voice/`):**
  1. **The sampler** (`command::sample`) draws canonical commands, each over its own invented
     vocabulary (made-up unit, place, team and power names; the real class words), weighted toward
     common commands. Every draw round-trips through the parser; every verb turns up.
     `cargo run -p zhbo-core --example voice_corpus -- sample tools/voice 2400 16 20261107` wrote
     2,400 commands in 16 batches.
  2. **Paraphrases:** Sonnet agents wrote six spoken phrasings per command to
     `tools/voice/PARAPHRASE_BRIEF.md`'s rules; the labels stay correct because they came from the
     sampler. A first pass (8 agents, 300 commands each) came back about half templated: the
     canonical text with filler around it. A second pass (16 agents, 150 each) under a stricter
     brief (no scripts, at least four of six reworded, written in chunks) came back 17%
     filler-only. Both passes are kept, with at most one filler-only phrasing per command.
  3. **Meaning checks** stand in for the back-translation pass and drop a paraphrase that loses a
     name (every name verbatim), a count or group number, a compass word, an exclusion or a
     sequence (`voice_corpus assemble`).
  4. **Speech noise** in code, seeded: homophones (for/four, there/their, guard/god, …), a filler,
     a dropped small word, never inside a name, on 40% of the kept phrasings.
  5. **Splits** by command, 90/5/5 (`train.tsv`, `dev.tsv`, `test.tsv`, built, not committed), the
     canonical lines among the training inputs; `stats.txt` counts what was kept and dropped and
     why. The first build: 20,643 paraphrases kept of 28,800 (287 lost a name, 22 a count,
     exclusion or sequence, 7,848 were filler-only), 8,241 noisy copies; 28,054 training rows,
     1,507 dev, 1,469 test. The normaliser alone already reads 6,722 of the kept phrasings back
     to their label; the model's job is the rest.
  6. **Control characters** drop a paraphrase: a script in two pass-1 batches left `\x00one\x00`
     placeholders in 296 of them.
  7. **English names.** A third of the commands are copied again with every invented word
     renamed into an English one (`sample::Renaming`: `korgor tower` → `granite tower`), in
     every split, under a vocabulary `<id>e` (`vocab_renamed.tsv`). A match's names are English
     words; a model that only saw invented ones learned to read English in a name's place as noise
     (below). The word list holds none of Zero Hour's names, the grammar's words or the synonym
     table's (`english_name_words_collide_with_nothing`). The build now: 38,249 training rows,
     2,091 dev, 2,046 test, 875 commands renamed.
  8. **As Whisper hears them** (`voice_heard`): training inputs spoken by `say` in 14 voices the
     tests never use, at drawn speaking rates, through Whisper, its prompt and the snapper, into
     `heard*.tsv`, which the training script adds. A row stays only if the transcript still has
     every name of its label: where Whisper lost a name, the row taught the model to make one up.
     4,541 rows kept of 6,842 (a third of them English-named).

- **Fine-tuning (built, `tools/voice/train/train_t5.py`).** flan-t5-small on the training set and
  the heard rows, with a prompt that carries the match's names (`--prompt
  tools/voice/prompts/names.txt`: `Translate the spoken order into a command. Names: {names}.
  Order: {heard}`; the runtime fills it the same way from `prompt.txt`, the names from
  `Vocabulary::spoken_names`), five epochs on Apple's GPU (MPS) at about 20 minutes each, keeping
  the best epoch on a dev sample (the fourth); then three more epochs with the multiple-choice
  questions added (`--choices`, below), which made its plain generation better on speech. The weights, stored in f16 (half the download; the same answers to the
  last row), go to `~/.conquerops/voice/commands-t5-small/`, never into the repo; players get them
  from this project's GitHub release `voice-commands-v1`. Gotcha: transformers saves T5's own
  `lm_head` while the config still says the embeddings are tied, and candle trusting the config
  decodes nonsense; the script writes `tie_word_embeddings = false` and the loader goes by the
  weights.
- **Results**, the released model. Typed text (`voice_eval`, 2,046 held-out rows, greedy, one
  command at a time on the CPU):

  | | exact match | same meaning | ms per command |
  |---|---|---|---|
  | unconstrained | 0.961 | 0.970 (0.8% not a command) | 217 |
  | constrained | 0.961 | 0.970 (none stuck) | 229 |

  Spoken (`voice_listen`, 300 held-out commands in 8 voices, base.en, prompted and snapped): the
  meaning right 0.783 (0.802 on its 81 English-named commands), against 0.713 for the first model
  (trained on text alone). Over a match's kind of names (`voice_match`, 16 commands about Rangers,
  Humvees and a War Factory): 12 right (the miss asks rather than runs), against 13 for the first model and 2 for a model trained on unfiltered heard rows without
  English names, which read `rangers` as noise and answered `group 2 move to me`. The synthetic
  test cannot show that: its names are all invented, so `voice_match` is part of choosing a
  model. "Same meaning" counts `a and b` as `b and a`. The remaining errors: an invented
  selection on a command with an implied one, a group dropped from a long union, near-synonyms
  (`at` against `near`), and `the barracks` against `barracks`.
- **Experiments (2026-11-08), what was tried and kept or not:**

  | | spoken, same meaning | typed | notes |
  |---|---|---|---|
  | flan-t5-small untuned, `command:` prefix | | 0.04 (masked) | it cannot write the language unseen |
  | untuned, instruction prompt with names and three examples | | 0.05 (masked) | a 77M model does not learn a grammar from a prompt |
  | fine-tuned, `command:` prefix (r3) | 0.757 | 0.967 | guard: asks on 1.8% of right answers |
  | fine-tuned, names prompt | 0.760 | 0.973 | guard: 0.1%; +90 ms |
  | **names prompt, then with multiple-choice questions (released)** | **0.783** | **0.970** | guard: 0.1% for 31% of wrong ones |
  | the same, deciding by multiple choice where unsure (`decode_choice`, below 0.9) | 0.753 | 0.955 (200 rows, against 0.835 never asking) | 0.3–0.5 questions per command; about 700 ms |
  | untuned flan-t5-small answering the questions | | 0.820 (against 0.830) | |
  | options, beam 4 (`options::decode`) | 0.743 | 0.943 | below |
  | token beam 4 under the mask (`decode_tokens`) | 0.747 | 0.98 (100 rows) | below |
  | Whisper encoder → bridge → T5 decoder (`train_bridge.py --mode audio`) | 0.347 (unconstrained) | | 48% not a command; 240 ms in all |
  | T5-encoded names + bridged audio (`--mode both`) | 0.043 | | modality collapse |
  | whisper-t5-bridge's pretrained bridge (R3) fine-tuned, LoRA on untuned flan-t5-small (`finetune_bridge.py`) | 0.413 (unconstrained) | | 45% not a command |
  | R3 fine-tuned, LoRA on our translator | 0.450 | | 35% not a command |
  | R3 fine-tuned, our translator's whole decoder trained | 0.533 | | 28% not a command; silenced audio 0.000 |

  - **Option scoring** (`options`: every option the grammar allows run through the decoder as
    if written, compared by probability, an exact first-token bound pruning them; our own T5
    port in `t5net`, equal to candle's on 300 of 300 rows and faster). Greedy over whole options
    loses to the token mask (0.82 against 0.95 typed), because a short option (`the`) is at
    least as likely as any longer one it begins (`the mirdorhal bridge`); a beam of 4 recovers it
    but does not beat the mask, on speech either, where plain greedy under the mask is best. The
    weakest chosen option is a clean "unknown" signal (under 0.2 on none of the right answers and
    12% of the wrong ones) for the abstain production when it is built.
  - **Multiple choice** (`choice`, the user's idea): where the generator is unsure, the grammar's
    likeliest options go into the encoder beside the transcript and the command so far, and the
    model answers with a letter. Untuned flan-t5-small does not help. Fine-tuned on 70,102
    questions (`voice_choices`: each training command cut into the grammar's steps,
    `parse::phrases`; the wrong options the hardest ones, an overlapping phrase or the nearest in
    sound, at random letters), it lifts option decoding a lot (0.835 to 0.955 typed, 26 fixed and
    2 broken) but stays below the token mask on speech. Training on the questions, though, made
    the same model's plain generation better on speech (0.760 to 0.783), and that model ships.
  - **The audio bridges** (12,000 spoken training commands as Whisper encoder states from
    `voice_audio`, a stride-2 convolution and two transformer layers into the translator's
    decoder). Audio alone writes the command's shape but spells names from their sound
    (`tordordors`), and 12,000 utterances overfit by the eighth epoch. With the names as
    T5-encoded text beside the audio and T5 left trainable, the decoder learned to write
    plausible commands from the names and ignore the audio. Both point at the same need: far more
    audio, and a frozen T5 so only the bridge learns (BLSP's behaviour alignment, arXiv
    2309.00916): a separate project.
  - **That project's bridge, fine-tuned** (whisper-t5-bridge's R3, distilled on FLAN with T5
    frozen; `tools/voice/train/finetune_bridge.py`, T5's encoder frozen, on the same 12,000 spoken
    commands, features now from the openai-exact log-mel). It listens (silenced audio: nothing
    right) and starts far ahead of a bridge from scratch, best with our translator's decoder
    trained whole: 0.533 against 0.347. But it stays well below Whisper's own decoder: only 11 of
    its 140 errors are a wrong name alone; the rest drop or swap parts (`attack the enemy jet` for
    `thunder jet`), the lexical gap whisper-t5-bridge's own report found. The pipeline stays.

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
- **Coverage gate (constraint 1), built, in three parts:**
  1. Every `GUICommandType` in the engine's `ControlBar.h` is in `actions::BUTTONS`, and nothing
     else is (`command::actions::tests`, read from the engine submodule).
  2. Every message a player's machine sends the logic (`MessageStream.h`, from
     `MSG_BEGIN_NETWORK_MESSAGES` to the debug ones) is in `actions::MESSAGES`, with the verb or
     grammar that sends it, or why it is not a player's order or never sent.
  3. Every `CommandButton` in the player's own install (`INIZH.big`, `PatchINI.big`) uses a
     command type a verb or the grammar covers (`zhbo-assets` `command_gate`, real-install suite:
     813 buttons).
- **Lockstep.** A command issued from first person produces the same message stream, and the same
  state hash, as the equivalent overhead clicks in the harness.
- **In game.** An autotest leg that issues typed commands (move, build, produce, attack) and
  checks the engine's orders.

## 13. Build order and open questions

**Build order:**
1. **Built.** AST, grammar, parser and printer, the round-trip test. The parser covers the whole
   grammar, so the AST is stable for the later steps.
2. **Built.** The engine services (`command_objects`, `template_name`, `issue_order`), the world
   snapshot and vocabulary, the resolver, and the typed entry with its echo, plus the `--command`
   autotest leg. The resolver covers:
   - selections: types, class words, `units`, adjectives, `near`, superlatives and quantities,
     `and`/`except`, control groups, team names, `them`, `that`; with no selection, the last one;
   - targets: `me`, `there` and `that` from the crosshair, object queries (a fogged building's
     ghost included), offsets;
   - orders: move, force move, attack-move, stop, attack, force attack (object or ground), guard
     (object, point, or where they stand), `name them`, `make them group`; `then` queues moves
     only (Zero Hour queues waypoints, not other orders).

   Left for later steps, each refused with a message saying so: cluster filters (`in the … group`,
   `in my group`) and place names.
3. **Built.** The command buttons (`command_buttons`) and their verbs, the right-click verbs
   through the engine's own `evaluateContextCommand`, construction (lines included), cancel,
   unload, the hotkey orders, and the three-part coverage gate (section 7, section 12).
4. **Built.** The scroll-wheel menu over valid completions (section 8), with counts and the
   `--command` leg building a command through it.
5. **Built.** Adjectives, `near`, groups (clusters), place names, team names, and the synonym and
   alias tables with the normaliser, the player's own file included.
6. Voice. **Built:** the sampler and dataset, the fine-tuned flan-t5-small, the candle
   translator and constrained decoder, Whisper with the snapper, the voice process with the
   microphone, push-to-talk, the confidence act and its tuning keys, and the launcher's download
   (`crates/zhbo-voice`, `command::voice`, the launcher's `voice`), with the `--command` leg's
   spoken line. Left: the abstain production, real recorded speech in the evaluation, and the
   Windows and Linux microphone checks.

**Open questions:**
- Whether the menu should outline the units its command would select (the engine's local
  selection decals would do it, through one more client service).
- How a distance is spoken: coarse words only (`near`, `north of`), or numbers too, and in what
  unit.
- Whether the General's own field-computer orders (CONTRACT.md "The General under RTS control")
  get command forms. That is the exception in constraint 1, so the current answer is no.
