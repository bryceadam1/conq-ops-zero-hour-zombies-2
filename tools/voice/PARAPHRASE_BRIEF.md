# Paraphrase brief (voice training set)

You are writing training data for a small model that turns what a player SAYS into a strict command
language for a real-time strategy game. The player is in first person and talks to their army.

Input: one batch file, `tools/voice/batches/batch_NN.tsv`. Each line is
`id<TAB>canonical command<TAB>names` where `names` is a `|`-separated list (may be empty).

Output: write `tools/voice/paraphrases/batch_NN.tsv` (same NN). For EVERY input line, write exactly
6 lines `id<TAB>paraphrase`, one paraphrase per line, nothing else in the file (no header, no
comments, no blank lines). Use the Write tool once at the end (or a few appends); do not print the
paraphrases in your reply. Reply only with the number of lines written.

## How to work (read first)

- **Write every paraphrase yourself.** Do not write or run a script, a template or a loop that
  generates them, and do not build them by adding words around the canonical text. A first pass
  that did that was thrown away: a filter keeps at most ONE phrasing per command that is the
  canonical text plus filler ("uh", "please", "now", "right", "okay", "hey", …), and drops the rest.
- Work in chunks of about 25 input lines: write that chunk's 150 output lines with the Write tool
  (first chunk) or by appending (later chunks; read the file back and Write it whole, or use a
  shell `cat >> file <<'EOF'` with the lines you composed). Keep going until every input line has 6.
- **Of each command's 6 phrasings, at least 4 must reword the command itself**: a different verb
  phrase, a different way to name the selection or target, or a different word order (action
  first, target first, the selection as an address: "korvels, take out that tank"). At most one may
  stay close to the canonical text.

## What a paraphrase is

Something a person might actually say out loud in a game to mean exactly the canonical command.
Vary them across the 6:
- casual speech and commands ("rangers, get over here"), short and long;
- action first where natural ("build three korvel tanks", "send the nearest drone at that");
- filler and hesitation sometimes ("uh, korvels, attack that one", "okay everyone hold up");
- contractions, "please", "now", "quick", a self-correction now and then ("the tanks, no, the
  korvel tanks, stop");
- synonyms: everyone/everybody (units), they/those (them), this/that one (that), closest
  (nearest), furthest (farthest), go to/head to/get over to (move to), attack move/push to
  (attack-move to), kill/take out/hit (attack), halt/hold position (stop), defend/protect/cover
  (guard), follow me/cover me (guard me), make/train/produce/queue (build), research (upgrade),
  launch/call in (use), deal with/handle (act on), spread out (scatter), here/my position (me),
  over there/where I'm looking (there), numbers as words or digits;
- at most one of the 6 may be the canonical text itself, lightly varied.

## Hard rules (a filter drops paraphrases that break them)

1. Keep every name in `names` EXACTLY as written, same words, same singular/plural, lowercase is
   fine. These are invented proper nouns ("dorkors", "the zartancal crossing"): never translate,
   respell, shorten or pluralise them.
2. Keep the meaning exactly: the same units, the same action, the same target, the same counts,
   group numbers and compass directions (north/south/east/west and their compounds), the same
   superlatives (nearest/farthest/northernmost/weakest/healthiest), the same states (idle,
   damaged, veteran, elite, heroic, garrisoned, loaded), the same owner (my/enemy/allied/neutral/
   civilian).
3. `except` must still be said as an exclusion (except/but not/other than/besides…); `then` as a
   sequence (then/after that/and then…).
4. "that" means the thing under the crosshair; "there" means the spot under the crosshair; "me"
   means the speaker (the General).
5. No commands the canonical line does not contain. No explanations, no quotes, no numbering.

## Example

Input: `00003	the 2 weakest zarbras cheer then move northeast of the weakest enemy ravhalrav	ravhalrav|zarbras`

Output lines (6):
```
00003	my two weakest zarbras, give a cheer, then head northeast of the weakest enemy ravhalrav
00003	the 2 most damaged zarbras celebrate and after that get over northeast of the enemy's weakest ravhalrav
00003	uh, zarbras, the two weakest ones, cheer and then move to the northeast side of the weakest enemy ravhalrav
00003	have the two most beat-up zarbras cheer, then send them northeast of the enemy ravhalrav that's weakest
00003	two weakest zarbras: cheer, then go northeast of the weakest enemy ravhalrav
00003	the 2 weakest zarbras cheer then move northeast of the weakest enemy ravhalrav
```

More examples of rewording (not prefixing):
- `korvels attack the nearest enemy drone` → "korvels, take out the closest enemy drone",
  "send the korvels after that nearest enemy drone", "nearest enemy drone, korvels, hit it".
- `build 3 mirdri tanks` → "queue up three mirdri tanks", "I need 3 more mirdri tanks", "get me
  three mirdri tanks built".
- `them guard me` → "you guys, cover me", "stay on me", "keep me covered".
