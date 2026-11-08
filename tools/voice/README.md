# Voice training set

The first-person commands' voice model (docs/COMMANDS.md section 11) learns to turn what a player
says into the canonical command language. This folder holds its training set.

| File | What it is |
|---|---|
| `canonical.tsv` | 2,400 canonical commands (`id`, text, the names a paraphrase must keep), drawn from the grammar by `command::sample`, each over its own invented vocabulary (no Zero Hour names) |
| `batches/` | The same commands in 16 batches of 150, the paraphrase agents' inputs |
| `PARAPHRASE_BRIEF.md` | The rules the paraphrase agents followed |
| `paraphrases/pass1_*.tsv` | The first pass (8 Sonnet agents, 6 per command). About half was the canonical text with filler around it; `assemble` keeps one such line per command at most |
| `paraphrases/pass2_*.tsv` | The second pass (16 Sonnet agents, 6 per command) under the stricter brief: real rewording |
| `stats.txt` | What `assemble` kept and dropped, and why |
| `train.tsv`, `dev.tsv`, `test.tsv` | Not committed: built by `assemble` (`input`, canonical label), split 90/5/5 by command |

Rebuild:

```sh
# The canonical commands (deterministic for the seed; do not re-run unless starting over:
# the paraphrases are keyed to these ids).
cargo run -p zhbo-core --example voice_corpus -- sample tools/voice 2400 16 20261107
# The splits, from the canonical commands and every file in paraphrases/.
cargo run -p zhbo-core --example voice_corpus -- assemble tools/voice 20261107
```

`assemble` drops a paraphrase that loses a name, a count or group number, a compass word, an
exclusion or a sequence, keeps at most one filler-only phrasing per command, and adds a seeded
speech-recognition-noisy copy (homophones, a filler, a dropped small word; never inside a name) to
40% of the kept ones. The canonical lines themselves are training inputs too.
