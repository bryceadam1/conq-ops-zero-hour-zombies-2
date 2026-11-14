"""Speak the scale-up's spoken sentences with Kokoro on the Mac (docs/COMMANDS.md section 11, step 11):

    python tools/voice/tts/corpus_kokoro.py --spoken <corpus>/spoken.tsv --out <dir> \
        [--split train] [--per-command 1] [--shard 0/4] [--limit N]

Takes the first `--per-command` kept sentences of each command in `--split` (spoken.tsv is
written in a fixed order, so the choice is the same every run), and speaks each one in a Kokoro
English voice at a speed in 0.85-1.2, both drawn from the row's id so a rerun gives the same
audio. Writes `<out>/<cid>_<k>.flac` (16 kHz mono, the training format) and appends
`<out>/manifest-<shard>.tsv`: file, sentence, label, command, vocabulary, split, voice, speed,
seconds of audio, seconds taken. Clips already written are skipped, so a run resumes; `--shard i/n`
splits the work between processes.
"""

import argparse
import hashlib
import time
from pathlib import Path

import numpy as np
import soundfile as sf

from generate import to_16k

# Kokoro-82M's English voices ('a' American, 'b' British; f/m), all but the novelty one.
VOICES = [
    "af_alloy", "af_aoede", "af_bella", "af_heart", "af_jessica", "af_kore", "af_nicole", "af_nova", "af_river", "af_sarah", "af_sky",
    "am_adam", "am_echo", "am_eric", "am_fenrir", "am_liam", "am_michael", "am_onyx", "am_puck",
    "bf_alice", "bf_emma", "bf_isabella", "bf_lily", "bm_daniel", "bm_fable", "bm_george", "bm_lewis",
]


def draw(key):
    """The row's voice and speed, from a hash of its id."""
    h = int.from_bytes(hashlib.sha256(key.encode()).digest()[:8], "little")
    return VOICES[h % len(VOICES)], 0.85 + 0.35 * ((h >> 16) % 1000) / 999


def rows(spoken, split, per_command):
    taken = {}
    for line in Path(spoken).read_text().splitlines():
        sentence, label, cid, vid, row_split = line.split("\t")
        if row_split != split:
            continue
        k = taken.get(cid, 0)
        if k >= per_command:
            continue
        taken[cid] = k + 1
        yield f"{cid}_{k}", sentence, label, cid, vid, row_split


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--spoken", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--split", default="train")
    ap.add_argument("--per-command", type=int, default=1)
    ap.add_argument("--shard", default="0/1")
    ap.add_argument("--limit", type=int, default=0)
    args = ap.parse_args()
    shard, shards = (int(x) for x in args.shard.split("/"))
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    from kokoro import KPipeline

    pipes = {}
    manifest = open(out / f"manifest-{shard}.tsv", "a")
    done = 0
    for i, (rid, sentence, label, cid, vid, split) in enumerate(rows(args.spoken, args.split, args.per_command)):
        if i % shards != shard:
            continue
        if args.limit and done >= args.limit:
            break
        dest = out / f"{rid}.flac"
        if dest.exists():
            continue
        voice, speed = draw(rid)
        start = time.time()
        try:
            pipe = pipes.setdefault(voice[0], KPipeline(lang_code=voice[0], repo_id="hexgrad/Kokoro-82M"))
            parts = [np.asarray(audio, dtype=np.float32) for _, _, audio in pipe(sentence, voice=voice, speed=speed)]
        except Exception as e:  # one bad line must not stop the run
            print(f"{rid}: {type(e).__name__}: {e}", flush=True)
            continue
        if not parts:
            print(f"{rid}: no audio", flush=True)
            continue
        audio = to_16k(np.concatenate(parts), 24000)
        took = time.time() - start
        sf.write(dest, audio, 16000, format="FLAC")
        manifest.write(f"{dest.name}\t{sentence}\t{label}\t{cid}\t{vid}\t{split}\t{voice}\t{speed:.3f}\t{len(audio) / 16000:.3f}\t{took:.3f}\n")
        manifest.flush()
        done += 1
        if done % 100 == 0:
            print(f"shard {shard}: {done} clips", flush=True)
    print(f"shard {shard}: done, {done} new clips in {out}", flush=True)


if __name__ == "__main__":
    main()
