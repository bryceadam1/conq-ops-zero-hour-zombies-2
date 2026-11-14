"""Score the bake-off's audio (docs/COMMANDS.md section 11, "Choosing a TTS"):

    venv/bin/python tools/voice/tts/score.py --lines <dir>/lines.tsv --audio <dir>/audio [--models kokoro audio8 ...]

For every clip of every model:
  * intelligibility: Whisper large-v3's transcript against the line (word error rate, numbers and
    punctuation normalised) and whether every name of the line is heard whole;
  * quality: torchaudio's SQUIM objective model, which estimates STOI, PESQ and SI-SDR from the
    clip alone (no clean reference needed);
  * voice match: cosine similarity of WavLM speaker embeddings (microsoft/wavlm-base-plus-sv)
    between the clip and its reference speaker's clip;
  * a failure: word error rate over 0.3, or 0.15-1.5 s per written word broken.
Writes `<audio>/scores.tsv` (one row per clip) and `<audio>/summary.txt` (per model).
"""

import argparse
import re
from pathlib import Path

import numpy as np
import soundfile as sf
import torch

NUMBERS = {w: str(i) for i, w in enumerate("zero one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty".split())}


def norm(text):
    words = re.sub(r"[^a-z0-9' ]", " ", text.lower().replace("-", " ")).split()
    return [NUMBERS.get(w, w) for w in words]


def wer(ref, hyp):
    r, h = norm(ref), norm(hyp)
    d = list(range(len(h) + 1))
    for i in range(1, len(r) + 1):
        prev, d[0] = d[0], i
        for j in range(1, len(h) + 1):
            cur = min(d[j] + 1, d[j - 1] + 1, prev + (r[i - 1] != h[j - 1]))
            prev, d[j] = d[j], cur
    return d[len(h)] / max(len(r), 1)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--lines", required=True)
    ap.add_argument("--audio", required=True)
    ap.add_argument("--models", nargs="*")
    args = ap.parse_args()
    lines = Path(args.lines)
    audio = Path(args.audio)
    refs = lines.parent / "refs"
    rows = [r.split("\t") for r in lines.read_text().splitlines()]
    models = args.models or sorted(p.name for p in audio.iterdir() if p.is_dir())
    device = "mps" if torch.backends.mps.is_available() else "cuda" if torch.cuda.is_available() else "cpu"

    from transformers import AutoFeatureExtractor, WavLMForXVector, pipeline
    import torchaudio

    asr = pipeline("automatic-speech-recognition", model="openai/whisper-large-v3", device=device, torch_dtype=torch.float16 if device != "cpu" else torch.float32)
    squim = torchaudio.pipelines.SQUIM_OBJECTIVE.get_model().to("cpu").eval()
    fe = AutoFeatureExtractor.from_pretrained("microsoft/wavlm-base-plus-sv")
    sv = WavLMForXVector.from_pretrained("microsoft/wavlm-base-plus-sv").eval()

    def embed(wav):
        with torch.no_grad():
            x = fe(wav, sampling_rate=16000, return_tensors="pt")
            e = sv(**x).embeddings[0]
        return torch.nn.functional.normalize(e, dim=-1)

    def load16(path):
        a, sr = sf.read(path, dtype="float32")
        if a.ndim > 1:
            a = a.mean(1)
        if sr != 16000:
            a = torchaudio.functional.resample(torch.from_numpy(a), sr, 16000).numpy()
        return a

    ref_emb = {}
    out_rows = ["model\tid\twer\tnames_heard\tstoi\tpesq\tsi_sdr\tvoice_sim\tseconds\tgen_seconds\tfailed\ttranscript"]
    summary = []
    for model in models:
        timing = {}
        t = audio / model / "timing.tsv"
        if t.exists():
            for line in t.read_text().splitlines():
                lid, took, dur = line.split("\t")
                timing[lid] = float(took)
        stats = []
        for lid, text, _, names, speaker, clip, _ in rows:
            p = audio / model / f"{lid}.flac"
            if not p.exists():
                continue
            wav = load16(p)
            hyp = asr({"raw": wav, "sampling_rate": 16000}, generate_kwargs={"language": "en", "task": "transcribe"})["text"]
            w = wer(text, hyp)
            heard = " ".join(norm(hyp))
            names_ok = all(f" {' '.join(norm(n))} " in f" {heard} " for n in names.split("|") if n)
            with torch.no_grad():
                stoi, pesq, sisdr = (float(x) for x in squim(torch.from_numpy(wav)[None]))
            if clip not in ref_emb:
                ref_emb[clip] = embed(load16(refs / clip))
            sim = float((embed(wav) * ref_emb[clip]).sum())
            secs = len(wav) / 16000
            per_word = secs / max(len(text.split()), 1)
            failed = w > 0.3 or not (0.15 <= per_word <= 1.5)
            stats.append((w, names_ok, stoi, pesq, sisdr, sim, secs, timing.get(lid, float("nan")), failed))
            out_rows.append(f"{model}\t{lid}\t{w:.3f}\t{int(names_ok)}\t{stoi:.3f}\t{pesq:.2f}\t{sisdr:.1f}\t{sim:.3f}\t{secs:.2f}\t{timing.get(lid, float('nan')):.2f}\t{int(failed)}\t{hyp.strip()}")
        if not stats:
            continue
        a = np.array([[s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8]] for s in stats], dtype=float)
        rt = np.nansum(a[:, 7]) / max(np.nansum(a[:, 6]), 1e-9)
        summary.append(
            f"{model:<11} clips {len(stats):>3}  WER {a[:, 0].mean():.3f} (median {np.median(a[:, 0]):.3f})  names heard {a[:, 1].mean():.2f}  "
            f"failed {a[:, 8].mean():.2f}  STOI {a[:, 2].mean():.3f}  PESQ {a[:, 3].mean():.2f}  SI-SDR {a[:, 4].mean():.1f}  "
            f"voice match {a[:, 5].mean():.3f}  {rt:.2f} s of compute per s of audio"
        )
        print(summary[-1], flush=True)
    (audio / "scores.tsv").write_text("\n".join(out_rows) + "\n")
    (audio / "summary.txt").write_text("\n".join(summary) + "\n")


if __name__ == "__main__":
    main()
