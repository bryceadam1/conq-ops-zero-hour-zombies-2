"""The TTS bake-off's test lines (docs/COMMANDS.md section 11, "Choosing a TTS"):

    python3 tools/voice/tts/lines.py --data tools/voice --voices ~/Documents/Projects/whisper-t5-bridge/data/voices --out <dir>

Takes 100 hard spoken commands from the held-out sets (`dev.tsv`, `test.tsv`): 35 with English
names (the renamed copies), the rest invented names, preferring lines with counts, hesitations
("uh", "um") and length, so a TTS is judged where it is likely to fail. Each line gets one of
10 reference speakers (5 women, 5 men) from whisper-t5-bridge's LibriSpeech selection (CC BY 4.0),
their medium-rate clip. Writes `<out>/lines.tsv` (id, text, label, names, speaker, reference
clip, reference text) and copies the 10 reference clips to `<out>/refs/`.
"""

import argparse
import json
import random
import re
import shutil
from pathlib import Path


def names_by_vocab(data: Path):
    """Each vocabulary id's names (every list), from vocab.tsv and vocab_renamed.tsv."""
    out = {}
    for f in ("vocab.tsv", "vocab_renamed.tsv"):
        p = data / f
        if not p.exists():
            continue
        for line in p.read_text().splitlines():
            cols = line.split("\t")
            names = set()
            for field in cols[1:]:
                for item in field.split("|"):
                    names.update(x for x in item.split(">") if x)
            out[cols[0]] = sorted(names, key=len, reverse=True)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", default="tools/voice")
    ap.add_argument("--voices", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--count", type=int, default=100)
    ap.add_argument("--seed", type=int, default=20261113)
    args = ap.parse_args()
    data, voices_dir, out = Path(args.data), Path(args.voices).expanduser(), Path(args.out).expanduser()
    rng = random.Random(args.seed)
    vocab_names = names_by_vocab(data)
    rows = []
    for f in ("dev.tsv", "test.tsv"):
        for line in (data / f).read_text().splitlines():
            text, label, vid = line.split("\t")[:3]
            if text == label or not re.fullmatch(r"[a-z0-9 ,.'\-?!]+", text):
                continue
            names = [n for n in vocab_names.get(vid, []) if re.search(rf"\b{re.escape(n)}\b", text)]
            if not names:
                continue
            hard = len(text.split()) / 10 + bool(re.search(r"\b\d+\b", text)) + bool(re.search(r"\b(uh|um|erm)\b", text))
            rows.append((hard + rng.random(), text, label, vid, names))
    english = sorted([r for r in rows if r[3].endswith("e")], reverse=True)
    invented = sorted([r for r in rows if not r[3].endswith("e")], reverse=True)
    # The hardest of each kind, one per command label, spread over the ranking's top.
    def take(pool, n):
        seen, picked = set(), []
        for r in pool:
            if r[2] in seen:
                continue
            seen.add(r[2])
            picked.append(r)
            if len(picked) == n:
                break
        return picked
    n_english = round(args.count * 0.35)
    picked = take(english, n_english) + take(invented, args.count - n_english)
    rng.shuffle(picked)
    voices = json.loads((voices_dir / "voices.json").read_text())
    medium = [v for v in voices if v["rate_label"] == "medium" and v["split"] == "train"]
    women = sorted({v["speaker"] for v in medium if v["sex"] == "F"})[:5]
    men = sorted({v["speaker"] for v in medium if v["sex"] == "M"})[:5]
    speakers = [next(v for v in medium if v["speaker"] == s) for s in women + men]
    (out / "refs").mkdir(parents=True, exist_ok=True)
    for v in speakers:
        shutil.copy(voices_dir / v["clip"], out / "refs" / Path(v["clip"]).name)
    lines = []
    for i, (_, text, label, vid, names) in enumerate(picked):
        v = speakers[i % len(speakers)]
        lines.append("\t".join([f"b{i:03d}", text, label, "|".join(names), v["speaker"] + v["sex"], Path(v["clip"]).name, v["text"]]))
    (out / "lines.tsv").write_text("\n".join(lines) + "\n")
    print(f"{len(lines)} lines ({n_english} English-named) for {len(speakers)} speakers in {out}")


if __name__ == "__main__":
    main()
