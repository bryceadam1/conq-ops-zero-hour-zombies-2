# Spoken-command brief (voice training set, speech to command)

You are writing what a player SAYS OUT LOUD to their army in a real-time strategy game, for a
model that will hear it (through text-to-speech) and turn it into a strict command language. The
player is in first person, mid-battle, talking to their troops. Your sentences will be read aloud
by a speech synthesiser, so they must read naturally as spoken English.

Input: one batch file `<batch>.tsv`. Each line is `id<TAB>canonical command<TAB>names`, where
`names` is a `|`-separated list (may be empty). The canonical command is the strict machine form;
you write how a person would actually say it.

Output: write `<out>.tsv`. For EVERY input line, exactly 5 lines `id<TAB>spoken sentence`, nothing
else in the file (no header, no blank lines, no numbering). Reply only with the number of lines
written.

## How to work

- **Write every sentence yourself.** No scripts, templates or loops, and never build a sentence by
  adding filler around the canonical text: those are thrown away.
- Work in chunks of about 25 input lines (125 output lines): Write the first chunk to the output
  file, then append the rest (`cat >> <out>.tsv <<'EOF'` with the lines you composed), until every
  input line has 5.
- **All 5 must reword the command**: a different verb phrase, a different way to name the units or
  the target, a different order (the units addressed first: "Rangers, take out that tank."; the
  action first: "Take out that tank, rangers."), a different length.

## How it should sound

- **Correct, natural spoken English with normal punctuation**: commas where a speaker pauses, a
  full stop or an exclamation mark at the end, capital letter at the start. Contractions are good
  ("let's", "don't", "we're").
- **Conversational, the way people really talk in a game**: short orders ("Everyone, on me!"),
  longer ones, polite ones ("Could the tanks hold here, please?"), urgent ones ("Go, go, take the
  bridge now!"), indirect ones ("I need those rovers over at the outpost.").
- **Fillers and hesitation are welcome, sometimes**: "uh", "um", "okay", "alright", "so", "right",
  "please", "now", "quick", a restart ("The tanks, no, the korvel tanks, stop.").
- Small counts as words or digits, either is fine ("three tanks", "3 tanks").
- **Natural word order and articles, as a native speaker says it.** "Fix that up" (not "fix up
  that"); "Cancel the glaive depot" (not "cancel glaive depot"); "Go deal with whatever's over
  there" or "Head over there and handle it" for an action at a spot (not "take care of over
  there"). Read each sentence back: if a person would not say it that way, rewrite it.
- **Avoid echoing the canonical word order.** At most one of the 5 may follow the canonical
  order with small changes; the rest should restructure the sentence.
- **Never parser-speak.** Do not copy the canonical form's machine phrasing. Say "attack and move
  to", "push up to", "fight your way to" instead of "attack-move to"; "rally at", "set the rally
  point at" instead of "set rally point at"; "deal with", "take care of" instead of "act on";
  "group three" instead of "group 3 move"; never hyphenate words a person would not hyphenate.
- **Never transcription errors.** No "two" for "to", "their" for "there", "for" for "four", no
  dropped words: write it the way a careful writer would spell what was said.
- Synonyms to vary with: everyone, everybody, all units (units); you guys, those, them (them); that
  one, this (that); closest (nearest); furthest (farthest); go to, head to, get over to (move to);
  kill, take out, hit, go after (attack); halt, hold up, hold position (stop); defend, protect,
  cover (guard); follow me, cover me, stay on me (guard me); make, train, queue up, get me (build);
  research (upgrade); launch, call in (use); spread out (scatter); here, my position (me); over
  there, where I'm looking (there).

## Hard rules (a filter drops sentences that break them)

1. Keep every name in `names` EXACTLY as written: same words, same singular or plural. They are
   invented or unusual proper nouns ("dorkors", "the zartancal crossing", "granite towers"): never
   translate, respell, shorten or pluralise them. Capitalising them is fine.
2. Keep the meaning exactly: the same units, action and target; the same counts and group numbers;
   the same compass directions; the same superlatives (nearest, farthest, northernmost, weakest,
   healthiest); the same states (idle, damaged, veteran, elite, heroic, garrisoned, loaded); the
   same owner (my, enemy, allied, neutral, civilian).
3. `except` must still be an exclusion (except, but not, other than, besides); `then` a sequence
   (then, after that, and then, once that's done).
4. "that" is the thing under the crosshair, "there" the spot under it, "me" the speaker.
5. Nothing the canonical line does not say. No explanations, no quotation marks.

## Example

Input: `c000017	the 2 weakest zarbras cheer then move northeast of the weakest enemy ravhalrav	ravhalrav|zarbras`

Output (5 lines):
```
c000017	My two weakest zarbras, give a cheer, then head northeast of the weakest enemy ravhalrav.
c000017	Okay, the two most beat-up zarbras, celebrate and then get over to the northeast side of the enemy's weakest ravhalrav.
c000017	Have the 2 weakest zarbras cheer, and after that send them northeast of the weakest enemy ravhalrav.
c000017	Uh, zarbras, just the two weakest ones, cheer! Then move northeast of the weakest enemy ravhalrav.
c000017	Two weakest zarbras: cheer, then go northeast of the weakest enemy ravhalrav, please.
```

More rewording, never prefixing:
- `korvels attack-move to the granite outpost` → "Korvels, fight your way to the granite
  outpost.", "Push up to the granite outpost, korvels, and hit anything on the way."
- `build 3 mirdri tanks` → "Queue up three mirdri tanks.", "I need three more mirdri tanks,
  please.", "Get me 3 mirdri tanks built."
- `them guard me` → "You guys, cover me!", "Stay on me.", "Alright, keep me covered."
