"""The held-out spoken test (`voice_listen`'s rows) in Kokoro voices no model trained on
(docs/COMMANDS.md section 11):

    kokoro-venv/bin/python tools/voice/tts/listen_kokoro.py --data tools/voice --out <dir> [--rows 300]

Picks the rows `voice_listen` picks (every `len // rows`-th row of `<data>/test.tsv`) and speaks
row i in `HELDOUT[i % 8]`, Kokoro's voices from its other languages reading English through the
American pipeline: the scale-up's Kokoro clips used all 27 English voices (`corpus_kokoro.py`), so
none of those is a fair test of a model trained on them. Writes `<out>/<i>.wav` (16 kHz mono,
16-bit) and `<out>/manifest.tsv` (i, voice, sentence); `voice_listen` reads them with
`VOICE_AUDIO=<out>`.
"""

import argparse
from pathlib import Path

import numpy as np
import soundfile as sf

from generate import to_16k

# Spanish, French, Hindi, Italian and Portuguese voices: none in VOICES of corpus_kokoro.py.
HELDOUT = ["ef_dora", "em_alex", "ff_siwis", "hf_alpha", "hm_omega", "if_sara", "im_nicola", "pm_alex"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--rows", type=int, default=300)
    args = ap.parse_args()
    rows = [l.split("\t") for l in (Path(args.data) / "test.tsv").read_text().splitlines()]
    rows = [r for r in rows if len(r) >= 3]
    stride = max(len(rows) // max(args.rows, 1), 1)
    picked = rows[::stride][: args.rows]
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    from kokoro import KPipeline

    pipe = KPipeline(lang_code="a", repo_id="hexgrad/Kokoro-82M")
    lines = []
    for i, (sentence, *_rest) in enumerate(picked):
        voice = HELDOUT[i % len(HELDOUT)]
        parts = [np.asarray(audio, dtype=np.float32) for _, _, audio in pipe(sentence, voice=voice, speed=1.0)]
        sf.write(out / f"{i}.wav", to_16k(np.concatenate(parts), 24000), 16000, subtype="PCM_16")
        lines.append(f"{i}\t{voice}\t{sentence}")
    (out / "manifest.tsv").write_text("\n".join(lines) + "\n")
    print(f"{len(picked)} clips in {out}")


if __name__ == "__main__":
    main()
