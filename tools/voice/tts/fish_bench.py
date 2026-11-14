"""Fish S2-Pro's throughput through SGLang-Omni at several concurrencies (docs/COMMANDS.md section 11,
"Choosing a TTS"): how many seconds of speech a second of GPU time buys, which decides what the
corpus's audio costs.

    python fish_bench.py --url http://127.0.0.1:8000 --refs-url http://127.0.0.1:9000 --lines lines.tsv --out <dir>

Sends the bake-off's lines (each in its reference speaker's voice, the reference clip fetched by
the server from `--refs-url`) at concurrency 1, 4, 8 and 16, keeps every clip as 16 kHz FLAC under
`<out>/c<N>/`, and writes `<out>/bench.txt`: per concurrency, clips, seconds of audio, wall seconds
and their ratio (above 1 is faster than real time).
"""

import argparse
import io
import json
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np
import soundfile as sf


def speak(url, refs_url, row):
    lid, text, _, _, _, clip, ref_text = row
    body = {"model": "fishaudio/s2-pro", "voice": "default", "input": text, "response_format": "wav",
            "references": [{"audio_path": f"{refs_url}/{clip}", "text": ref_text}]}
    req = urllib.request.Request(f"{url}/v1/audio/speech", data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=600) as r:
        audio, sr = sf.read(io.BytesIO(r.read()), dtype="float32")
    if audio.ndim > 1:
        audio = audio.mean(1)
    return lid, audio, sr


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="http://127.0.0.1:8000")
    ap.add_argument("--refs-url", default="http://127.0.0.1:9000")
    ap.add_argument("--lines", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--levels", default="1,4,8,16")
    ap.add_argument("--per-level", type=int, default=48)
    args = ap.parse_args()
    rows = [r.split("\t") for r in Path(args.lines).read_text().splitlines()]
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    report = []
    speak(args.url, args.refs_url, rows[0])  # warm up
    for level in (int(x) for x in args.levels.split(",")):
        batch = rows[: args.per_level]
        d = out / f"c{level}"
        d.mkdir(exist_ok=True)
        start = time.time()
        audio_secs, failures = 0.0, 0
        with ThreadPoolExecutor(level) as pool:
            for fut in [pool.submit(speak, args.url, args.refs_url, r) for r in batch]:
                try:
                    lid, audio, sr = fut.result()
                except Exception as e:
                    failures += 1
                    print(f"c{level}: {type(e).__name__}: {e}", flush=True)
                    continue
                import torch
                import torchaudio.functional as AF
                a16 = AF.resample(torch.from_numpy(np.ascontiguousarray(audio)), sr, 16000).numpy() if sr != 16000 else audio
                sf.write(d / f"{lid}.flac", a16, 16000, format="FLAC")
                audio_secs += len(audio) / sr
        wall = time.time() - start
        line = f"concurrency {level}: {len(batch) - failures} clips, {failures} failed, {audio_secs:.1f} s of audio in {wall:.1f} s wall: {audio_secs / max(wall, 1e-9):.2f}x real time"
        print(line, flush=True)
        report.append(line)
        (out / "bench.txt").write_text("\n".join(report) + "\n")


if __name__ == "__main__":
    main()
