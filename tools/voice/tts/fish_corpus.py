"""The scale-up's Fish S2-Pro clips (docs/COMMANDS.md section 11, step 11). Two steps:

On the Mac, the lines and their reference voices:

    python3 tools/voice/tts/fish_corpus.py lines --spoken <corpus>/spoken.tsv \
        --voices ~/Documents/Projects/whisper-t5-bridge/data/voices --out <dir> [--train 120000] [--test 2000]

  Takes each command's second kept sentence (Kokoro speaks the first, so the two engines say
  different words), `--test` lines of the test split first and then `--train` lines of train, each
  part in the order of a hash of its id, so a run cut short still leaves an even sample. Each line
  gets a reference clip, drawn from its id: train lines from the 32 `train` speakers of
  whisper-t5-bridge's LibriSpeech selection (CC BY 4.0) at any of their three rates, test lines
  from the 8 `heldout` ones, so no test voice is ever trained on. Writes `<dir>/lines.tsv` (id,
  sentence, label, command, vocabulary, split, clip, clip text) and copies the clips to
  `<dir>/refs/`. `--exclude <file>` (command ids, one per line) skips commands that already have
  audio; a command left with only one kept sentence then speaks that one (id `<cid>_0`).

On the GPU machine, against SGLang-Omni's server (fish_sglang.sh starts it):

    python3 fish_corpus.py speak --lines lines.tsv --refs /tmp/refs --out <dir> [--concurrency 32]

  Writes `<dir>/<id>.flac` (16 kHz mono) and appends `<dir>/manifest.tsv` (file, seconds of audio);
  lines already written are skipped, so a restarted run resumes. Logs its rate every 500 clips.
"""

import argparse
import hashlib
import io
import json
import shutil
import time
from pathlib import Path


def h(key):
    return int.from_bytes(hashlib.sha256(key.encode()).digest()[:8], "little")


def lines(args):
    voices = json.loads((Path(args.voices) / "voices.json").read_text())
    pools = {"train": [v for v in voices if v["split"] == "train"], "test": [v for v in voices if v["split"] == "heldout"]}
    exclude = set(Path(args.exclude).read_text().split()) if args.exclude else set()
    second = {}
    seen = {}
    for line in Path(args.spoken).read_text().splitlines():
        sentence, label, cid, vid, split = line.split("\t")
        k = seen.get(cid, 0)
        seen[cid] = k + 1
        if k <= 1 and split in pools and cid not in exclude and (k == 1 or cid not in second):
            second[cid] = (f"{cid}_{k}", sentence, label, cid, vid, split)
    if not exclude:  # as first run: only commands with a second sentence
        second = {cid: r for cid, r in second.items() if r[0].endswith("_1")}
    out = Path(args.out)
    (out / "refs").mkdir(parents=True, exist_ok=True)
    rows = []
    for split, count in (("test", args.test), ("train", args.train)):
        part = sorted((r for r in second.values() if r[5] == split), key=lambda r: h(r[0]))[:count]
        for r in part:
            v = pools[split][h("voice" + r[0]) % len(pools[split])]
            clip = Path(v["clip"]).name
            shutil.copy(Path(args.voices) / v["clip"], out / "refs" / clip)
            rows.append("\t".join(r + (clip, v["text"])))
    (out / "lines.tsv").write_text("\n".join(rows) + "\n")
    print(f"{len(rows)} lines, {len(list((out / 'refs').iterdir()))} reference clips, in {out}")


def speak(args):
    import urllib.request
    from concurrent.futures import ThreadPoolExecutor

    import numpy as np
    import soundfile as sf
    import torch
    import torchaudio.functional as AF

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    todo = [r.split("\t") for r in Path(args.lines).read_text().splitlines()]
    todo = [r for r in todo if not (out / f"{r[0]}.flac").exists()]
    manifest = open(out / "manifest.tsv", "a")

    def one(r):
        rid, sentence, clip, clip_text = r[0], r[1], r[6], r[7]
        body = {"model": "fishaudio/s2-pro", "voice": "default", "input": sentence, "response_format": "wav",
                "references": [{"audio_path": f"{args.refs}/{clip}", "text": clip_text}]}
        req = urllib.request.Request(f"{args.url}/v1/audio/speech", data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=600) as resp:
            audio, sr = sf.read(io.BytesIO(resp.read()), dtype="float32")
        if audio.ndim > 1:
            audio = audio.mean(1)
        if sr != 16000:
            audio = AF.resample(torch.from_numpy(np.ascontiguousarray(audio)), sr, 16000).numpy()
        sf.write(out / f"{rid}.flac", audio, 16000, format="FLAC")
        return rid, len(audio) / 16000

    start, done, secs, failed = time.time(), 0, 0.0, 0
    with ThreadPoolExecutor(args.concurrency) as pool:
        for fut in [pool.submit(one, r) for r in todo]:
            try:
                rid, s = fut.result()
            except Exception as e:  # one bad line must not stop the run
                failed += 1
                print(f"{type(e).__name__}: {e}", flush=True)
                continue
            manifest.write(f"{rid}.flac\t{s:.3f}\n")
            manifest.flush()
            done, secs = done + 1, secs + s
            if done % 500 == 0:
                wall = time.time() - start
                print(f"{done} clips, {failed} failed, {secs:.0f} s of audio in {wall:.0f} s: {secs / wall:.2f}x", flush=True)
    print(f"done: {done} clips, {failed} failed, {secs:.0f} s of audio in {time.time() - start:.0f} s", flush=True)


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(required=True)
    p = sub.add_parser("lines")
    p.add_argument("--spoken", required=True)
    p.add_argument("--voices", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--train", type=int, default=120000)
    p.add_argument("--test", type=int, default=2000)
    p.add_argument("--exclude")
    p.set_defaults(fn=lines)
    p = sub.add_parser("speak")
    p.add_argument("--lines", required=True)
    p.add_argument("--refs", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--url", default="http://127.0.0.1:8000")
    p.add_argument("--concurrency", type=int, default=32)
    p.set_defaults(fn=speak)
    args = ap.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
