"""Augmentation for training Whisper straight on commands (docs/COMMANDS.md section 11, step 12):
the clean TTS clips made to sound like a player's microphone in a room, during a match.

    venv/bin/python tools/voice/train/augment.py pools --dir ~/.conquerops/voice/augment
    venv/bin/python tools/voice/train/augment.py demo --dir <augment dir> --clips <list> --out <page.html>
    venv/bin/python tools/voice/train/augment.py testset --dir <augment dir> --clips <list> --out <dir>
    venv/bin/python tools/voice/train/augment.py calibrate --dir <augment dir> --clips <list> --whisper <model dir>

The recordings (downloaded into `--dir`, never shipped, never either game's sounds): MUSAN's
noise, music and LibriVox speech (CC BY 4.0) and OpenSLR's RIRS_NOISES room responses, simulated
and real (Apache 2.0). `pools` lists them in `pools.json`, each with its length and a
split drawn from a hash of its path: a fifth of every kind is held back for the noisy test set,
so no test noise or room is ever trained on.

`augment(audio, rng, pools, split)` is applied as clips are loaded, never stored: each load draws
its own chain from `rng_for(clip, epoch)`, so every epoch hears a different take and a run repeats
exactly. A fifth of the loads stay clean; the rest draw each effect by its share (`SHARES`):
push-to-talk timing, a room, background noise (noise or music 12-30 dB under the voice, mostly 20;
chatter 15-30, mostly 22; both measured over their active frames), a microphone's frequency response and, on most of
those, the Opus codec voice chat runs through (ffmpeg's libopus), then a level and now and then
clipping. Whisper's 3 s of trailing silence (`hear::WINDOW_SILENCE`) is digital silence at run
time, so it is added after augmenting, never under the noise. `spec_augment` masks the log-mel
features lightly as they go into the model.

A clip list is a file of `<path>\t<sentence>` lines (`clips` below builds one from the corpus
manifests).
"""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

import numpy as np
import soundfile as sf

SR = 16000
CLEAN = 0.2
SHARES = {"ptt": 0.3, "room": 0.4, "noise": 0.6, "mic": 0.5}
NOISE_KINDS = {"noise": 0.5, "music": 0.25, "babble": 0.25}
ROOMS = {"smallroom": 0.6, "mediumroom": 0.25, "largeroom": 0.05, "real": 0.1}
# How far under the voice a background sits, in dB (low, mode, high): the listening check
# (MODLOG, 2026-11-15) marked every take under about 15 dB too hard, chatter at 14 too.
SNR = {"noise": (12, 20, 30), "music": (12, 20, 30), "babble": (15, 22, 30)}
OPUS = 0.6
OPUS_KBPS = [12, 16, 24, 32]
CLIP = 0.05


def h(key):
    return int.from_bytes(hashlib.sha256(key.encode()).digest()[:8], "little")


def rng_for(clip, epoch, seed=0):
    """The draw for one load of a clip: the same clip and epoch always augment the same way."""
    return np.random.default_rng(h(f"{seed}:{epoch}:{clip}"))


def split_of(rel):
    return "test" if h("pool:" + rel) % 5 == 0 else "train"


def pick(rng, weights):
    keys = list(weights)
    p = np.array([weights[k] for k in keys], dtype=np.float64)
    return keys[rng.choice(len(keys), p=p / p.sum())]


# --- the recordings -----------------------------------------------------------------------------


def rir_kind(rel):
    for room in ("smallroom", "mediumroom", "largeroom"):
        if f"/{room}/" in rel:
            return room
    return "real"


def scan(root):
    """Every recording under `root`: (path relative to root, kind, group). The split goes by group:
    a simulated room's responses (one folder per room) or a real room's (`--room-id`) stay
    together, so no test room is heard in training from another position."""
    root = Path(root)
    found = []
    for kind, pattern in (("noise", "musan/noise/**/*.wav"), ("music", "musan/music/**/*.wav"), ("babble", "musan/speech/librivox/**/*.wav"),
                          ("rir", "RIRS_NOISES/simulated_rirs/**/*.wav")):
        for p in sorted(root.glob(pattern)):
            rel = str(p.relative_to(root))
            found.append((rel, kind, str(Path(rel).parent) if kind == "rir" else rel))
    # RIRS_NOISES' point-source noises are MUSAN's Freesound noises again (its README), so they're
    # left out: one recording under two paths could land in both splits. The real responses are
    # listed beside the isotropic noises in the same folder.
    rir_list = root / "RIRS_NOISES/real_rirs_isotropic_noises/rir_list"
    if rir_list.exists():
        for line in rir_list.read_text().splitlines():
            f = line.split()
            if (root / f[-1]).exists():
                found.append((f[-1], "rir", f[f.index("--room-id") + 1]))
    return found


def build_pools(root):
    pools = {}
    for rel, kind, group in scan(root):
        info = sf.info(str(Path(root) / rel))
        name = rir_kind(rel) if kind == "rir" else kind
        pools.setdefault(name, []).append([rel, round(info.frames / info.samplerate, 3), split_of(group)])
    return pools


class Pools:
    """The recordings of `pools.json`, by kind and split, read a segment at a time."""

    def __init__(self, root, pools=None):
        self.root = Path(root)
        pools = pools if pools is not None else json.loads((self.root / "pools.json").read_text())
        self.by = {(kind, split): [r for r in rows if r[2] == split] for kind, rows in pools.items() for split in ("train", "test")}

    def choose(self, rng, kind, split):
        rows = self.by.get((kind, split)) or []
        return rows[rng.integers(len(rows))] if rows else None

    def segment(self, rng, kind, split, n):
        """`n` samples of a recording of `kind` from a drawn offset, looped if it is shorter."""
        row = self.choose(rng, kind, split)
        if row is None:
            return None
        path = self.root / row[0]
        frames = sf.info(str(path)).frames
        if frames >= n:
            start = int(rng.integers(frames - n + 1))
            x, sr = sf.read(str(path), start=start, frames=n, dtype="float32", always_2d=True)
        else:
            x, sr = sf.read(str(path), dtype="float32", always_2d=True)
        x = x[:, 0]
        if sr != SR:
            x = resample(x, sr)
        if len(x) == 0:
            return None
        if len(x) < n:
            x = np.tile(x, n // len(x) + 1)
        return x[:n]

    def rir(self, rng, split):
        room = pick(rng, ROOMS)
        row = self.choose(rng, room, split)
        if row is None:
            return None, room
        x, sr = sf.read(str(self.root / row[0]), dtype="float32", always_2d=True)
        x = x[:, int(rng.integers(x.shape[1]))]
        return (resample(x, sr) if sr != SR else x), room


def resample(x, sr):
    import torch
    import torchaudio.functional as AF

    return AF.resample(torch.from_numpy(np.ascontiguousarray(x, dtype=np.float32)), sr, SR).numpy()


# --- the effects --------------------------------------------------------------------------------


def db(x):
    return 10 * np.log10(max(float(x), 1e-12))


def frame_power(x, n=320):
    """Mean power of each 20 ms frame."""
    k = len(x) // n
    if k == 0:
        return np.array([np.mean(x * x) if len(x) else 0.0])
    return np.mean(x[: k * n].reshape(k, n) ** 2, axis=1)


def speech_power(x):
    """The voice's power: the mean over frames within 40 dB of the loudest, so silence doesn't count."""
    p = frame_power(x)
    top = p.max()
    if top <= 0:
        return 0.0
    return float(p[p >= top * 1e-4].mean())


def speech_span(x, n=320):
    """First and last sample of the frames within 40 dB of the loudest."""
    p = frame_power(x, n)
    active = np.nonzero(p >= p.max() * 1e-4)[0] if p.max() > 0 else np.array([0, len(p) - 1])
    return int(active[0]) * n, min(len(x), (int(active[-1]) + 1) * n)


def push_to_talk(x, rng):
    """The key pressed late or let go early, and silence the noise will fill around the words: the
    clip's own lead and tail cut to the speech, now and then 50 ms into it, then up to 0.5 s before
    and 0.3 s after."""
    a, b = speech_span(x)
    cut = int(0.05 * SR)
    r = rng.random()
    if r < 0.15:
        a = min(a + int(rng.integers(cut)), b - 1)
    elif r < 0.3:
        b = max(b - int(rng.integers(cut)), a + 1)
    lead, tail = int(rng.uniform(0, 0.5) * SR), int(rng.uniform(0, 0.3) * SR)
    return np.concatenate([np.zeros(lead, np.float32), x[a:b], np.zeros(tail, np.float32)]), {"ptt": [a, len(x) - b, lead, tail]}


def convolve(x, kernel):
    n = len(x) + len(kernel) - 1
    size = 1 << (n - 1).bit_length()
    return np.fft.irfft(np.fft.rfft(x, size) * np.fft.rfft(kernel, size), size)[:n].astype(np.float32)


def reverberate(x, rir):
    """`x` in the room of `rir`: aligned to its direct path, at the dry clip's loudness."""
    start = max(0, int(np.argmax(np.abs(rir))) - int(0.002 * SR))
    rir = rir[start:]
    y = convolve(x, rir)[: len(x)]
    dry, wet = speech_power(x), speech_power(y)
    return y * np.sqrt(dry / wet) if wet > 0 else x


def add_noise(x, noise, snr):
    """`noise` under `x` at `snr` dB, both powers taken over their active frames only, so music's
    quiet passages and chatter's pauses don't make the loud parts louder than `snr` says."""
    p = speech_power(noise)
    if p <= 0:
        return x
    return x + noise * np.sqrt(speech_power(x) / (p * 10 ** (snr / 10)))


def noise_bed(rng, pools, split, n):
    """A background for `n` samples: a recording of noise, music, or one to five other voices."""
    kind = pick(rng, NOISE_KINDS)
    if kind != "babble":
        return pools.segment(rng, kind, split, n), kind, 1
    voices = int(rng.integers(1, 6))
    parts = [pools.segment(rng, "babble", split, n) for _ in range(voices)]
    parts = [p / np.sqrt(np.mean(p * p)) for p in parts if p is not None and np.mean(p * p) > 0]
    return (np.sum(parts, axis=0) if parts else None), kind, voices


def mic_response(n, rng):
    """A microphone's magnitude response over `n` rfft bins: a headset's or a laptop's band edges,
    a tilt, and on laptops a resonance."""
    f = np.fft.rfftfreq(2 * (n - 1), 1 / SR)
    f[0] = 1.0
    laptop = rng.random() < 0.4
    lo = rng.uniform(150, 400) if laptop else rng.uniform(60, 200)
    hi = rng.uniform(4000, 7000) if laptop else rng.uniform(5500, 7800)
    order = int(rng.integers(2, 5))
    gain = 1 / np.sqrt(1 + (f / hi) ** (2 * order)) / np.sqrt(1 + (lo / f) ** (2 * order))
    tilt = rng.uniform(-3, 3)
    gain = gain * 10 ** (tilt * np.log2(f / 1000) / 20)
    recipe = {"mic": "laptop" if laptop else "headset", "band": [round(lo), round(hi)], "order": order, "tilt": round(tilt, 2)}
    if laptop:
        fc, q, peak = rng.uniform(2000, 5000), rng.uniform(1, 4), rng.uniform(3, 8)
        gain = gain * (1 + (10 ** (peak / 20) - 1) / (1 + (q * (f / fc - fc / f)) ** 2))
        recipe["resonance"] = [round(fc), round(q, 2), round(peak, 1)]
    return gain, recipe


def microphone(x, rng):
    pad = 2048
    size = 1 << (len(x) + pad - 1).bit_length()
    spec = np.fft.rfft(x, size)
    gain, recipe = mic_response(len(spec), rng)
    y = np.fft.irfft(spec * gain, size)[: len(x)].astype(np.float32)
    return y, recipe


def opus(x, kbps):
    """`x` through libopus at `kbps` (voice mode) and back, the same length."""
    enc = subprocess.run(["ffmpeg", "-loglevel", "error", "-f", "f32le", "-ar", str(SR), "-ac", "1", "-i", "pipe:0",
                          "-c:a", "libopus", "-b:a", f"{kbps}k", "-application", "voip", "-f", "ogg", "pipe:1"],
                         input=np.ascontiguousarray(x, dtype=np.float32).tobytes(), capture_output=True, check=True).stdout
    dec = subprocess.run(["ffmpeg", "-loglevel", "error", "-i", "pipe:0", "-f", "f32le", "-ar", str(SR), "-ac", "1", "pipe:1"],
                         input=enc, capture_output=True, check=True).stdout
    y = np.frombuffer(dec, dtype=np.float32)
    return np.pad(y, (0, max(0, len(x) - len(y))))[: len(x)].copy()


def level(x, rng):
    """A peak level from -35 to -3 dBFS; on a few clips, gain past full scale and clipping."""
    peak = float(np.max(np.abs(x))) if len(x) else 0.0
    if peak <= 0:
        return x, {}
    if rng.random() < CLIP:
        over = rng.uniform(3, 9)
        return np.clip(x * (10 ** (over / 20) / peak), -1, 1).astype(np.float32), {"clip_db": round(over, 1)}
    target = rng.uniform(-35, -3)
    return (x * (10 ** (target / 20) / peak)).astype(np.float32), {"peak_db": round(target, 1)}


def augment(audio, rng, pools, split="train", force=False):
    """One take of `audio` (16 kHz float): the clip and the recipe drawn. `force` skips the clean
    share (the demo and the noisy test set)."""
    x = np.asarray(audio, dtype=np.float32)
    if not force and rng.random() < CLEAN:
        return x, {"clean": True}
    on = {k: rng.random() < share for k, share in SHARES.items()}
    if force and not any(on.values()):
        on[pick(rng, SHARES)] = True
    recipe = {}
    if on["ptt"]:
        x, r = push_to_talk(x, rng)
        recipe.update(r)
    if on["room"]:
        rir, room = pools.rir(rng, split)
        if rir is not None:
            x = reverberate(x, rir)
            recipe["room"] = room
    if on["noise"]:
        bed, kind, voices = noise_bed(rng, pools, split, len(x))
        if bed is not None:
            snr = rng.triangular(*SNR[kind])
            x = add_noise(x, bed, snr)
            recipe.update({"noise": kind, "snr": round(float(snr), 1)} | ({"voices": voices} if kind == "babble" else {}))
    if on["mic"]:
        x, r = microphone(x, rng)
        recipe.update(r)
        if rng.random() < OPUS:
            kbps = int(rng.choice(OPUS_KBPS))
            x = opus(x, kbps)
            recipe["opus"] = kbps
    x, r = level(x, rng)
    recipe.update(r)
    return x, recipe


def spec_augment(mel, rng, frames=None):
    """Light SpecAugment on log-mel features `[bins, positions]`: two frequency masks up to an
    eighth of the bins and two time masks up to 5% of the speech's `frames` each (the padded
    silence is never masked), filled with the features' mean."""
    mel = mel.copy()
    bins, total = mel.shape
    frames = min(frames or total, total)
    fill = mel.mean()
    for _ in range(2):
        w = int(rng.integers(0, bins // 8 + 1))
        f0 = int(rng.integers(0, bins - w + 1))
        mel[f0 : f0 + w, :] = fill
    for _ in range(2):
        w = int(rng.integers(0, max(1, frames // 20) + 1))
        t0 = int(rng.integers(0, frames - w + 1))
        mel[:, t0 : t0 + w] = fill
    return mel


# --- tools --------------------------------------------------------------------------------------


def read_clips(path):
    rows = []
    for line in Path(path).read_text().splitlines():
        p = line.split("\t")
        rows.append((p[0], p[1] if len(p) > 1 else ""))
    return rows


def load(path):
    x, sr = sf.read(path, dtype="float32", always_2d=True)
    x = x[:, 0]
    return resample(x, sr) if sr != SR else x


def cmd_pools(args):
    pools = build_pools(args.dir)
    (Path(args.dir) / "pools.json").write_text(json.dumps(pools))
    for kind, rows in sorted(pools.items()):
        test = [r for r in rows if r[2] == "test"]
        print(f"{kind}: {len(rows)} recordings, {sum(r[1] for r in rows) / 3600:.1f} h; held back {len(test)} ({sum(r[1] for r in test) / 3600:.1f} h)")


def cmd_clips(args):
    """A clip list from the corpus: Fish's lines and Kokoro's manifests, of one split."""
    rows = []
    for d in args.fish or []:
        d = Path(d)
        lines = {p[0]: p for p in (l.split("\t") for l in (d.parent.parent / "lines.tsv").read_text().splitlines())}
        for l in (d / "manifest.tsv").read_text().splitlines():
            rid = l.split("\t")[0].rsplit(".", 1)[0]
            if lines[rid][5] == args.split:
                rows.append(f"{d / (rid + '.flac')}\t{lines[rid][1]}")
    for d in args.kokoro or []:
        d = Path(d)
        for m in sorted(d.glob("manifest-*.tsv")):
            for l in m.read_text().splitlines():
                p = l.split("\t")
                if p[5] == args.split:
                    rows.append(f"{d / p[0]}\t{p[1]}")
    Path(args.out).write_text("\n".join(rows) + "\n")
    print(f"{len(rows)} clips in {args.out}")


def sample(clips, count, seed):
    return sorted(clips, key=lambda c: h(f"{seed}:{c[0]}"))[:count]


def cmd_testset(args):
    """The noisy test set: each clip once, augmented with the held-back recordings, written to disk."""
    pools = Pools(args.dir)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    rows = []
    for path, sentence in sample(read_clips(args.clips), args.count, "testset"):
        y, recipe = augment(load(path), rng_for(path, 0, seed="testset"), pools, "test", force=True)
        name = Path(path).name
        sf.write(out / name, y, SR, format="FLAC")
        rows.append(f"{name}\t{sentence}\t{path}\t{json.dumps(recipe)}")
    (out / "manifest.tsv").write_text("\n".join(rows) + "\n")
    print(f"{len(rows)} noisy test clips in {out}")


def cmd_demo(args):
    """A listening page: clips before and after, with each recipe."""
    import base64
    import html
    import io

    pools = Pools(args.dir)
    items = []
    for i, (path, sentence) in enumerate(sample(read_clips(args.clips), args.count, "demo")):
        x = load(path)
        y, recipe = augment(x, rng_for(path, i, seed="demo"), pools, "train", force=True)
        uris = []
        # Played back at the clean take's peak: Whisper evens out loudness, a listener can't.
        peak = float(np.max(np.abs(y)))
        heard = y * (float(np.max(np.abs(x))) / peak) if peak > 0 and "clip_db" not in recipe else y
        for a in (x, heard):
            buf = io.BytesIO()
            sf.write(buf, np.clip(a, -1, 1), SR, format="FLAC")
            uris.append("data:audio/flac;base64," + base64.b64encode(buf.getvalue()).decode())
        items.append(f"""<section><p class="s">{html.escape(sentence)}</p>
<div class="row"><label>clean<audio controls preload="none" src="{uris[0]}"></audio></label>
<label>augmented<audio controls preload="none" src="{uris[1]}"></audio></label></div>
<p class="r">{html.escape(describe(recipe))}</p>
<div class="rate" data-i="{i}">{''.join(f'<button data-v="{v}">{v}</button>' for v in ("fine", "too hard", "unrealistic"))}</div></section>""")
    Path(args.out).write_text(DEMO.replace("{items}", "\n".join(items)).replace("{count}", str(len(items))))
    print(f"{len(items)} clips in {args.out}")


def describe(r):
    parts = []
    if "ptt" in r:
        parts.append("push-to-talk timing")
    if r.get("room"):
        parts.append(f"room echo ({r['room']})" if isinstance(r["room"], str) else "room echo")
    if "noise" in r:
        what = {"noise": "noise", "music": "music", "babble": f"{r.get('voices', 1)} other voice(s)"}[r["noise"]]
        parts.append(f"{what} at {r['snr']} dB under the voice")
    if "mic" in r:
        parts.append(f"{r['mic']} mic {r['band'][0]}-{r['band'][1]} Hz")
    if "opus" in r:
        parts.append(f"Opus {r['opus']} kbps")
    if "clip_db" in r:
        parts.append(f"clipped, {r['clip_db']} dB over")
    elif "peak_db" in r:
        parts.append(f"peak {r['peak_db']} dBFS")
    return "; ".join(parts)


def words(s):
    import re

    return re.sub(r"[^a-z0-9' ]", " ", s.lower().replace("-", " ")).split()


def wer(ref, hyp):
    r, h_ = words(ref), words(hyp)
    d = list(range(len(h_) + 1))
    for i, a in enumerate(r, 1):
        prev, d[0] = d[0], i
        for j, b in enumerate(h_, 1):
            prev, d[j] = d[j], min(d[j] + 1, d[j - 1] + 1, prev + (a != b))
    return d[len(h_)], len(r)


def cmd_calibrate(args):
    """Whisper's word error on clean and augmented takes of the same clips, by effect and by SNR:
    the augmentation should make speech harder, not unintelligible."""
    import torch
    from transformers import WhisperForConditionalGeneration, WhisperProcessor

    device = "mps" if torch.backends.mps.is_available() else "cpu"
    proc = WhisperProcessor.from_pretrained(args.whisper)
    model = WhisperForConditionalGeneration.from_pretrained(args.whisper).to(device).eval()
    pools = Pools(args.dir)

    def hear(batch):
        feats = proc.feature_extractor([np.concatenate([a, np.zeros(3 * SR, np.float32)]) for a in batch], sampling_rate=SR, return_tensors="pt").input_features
        with torch.no_grad():
            ids = model.generate(feats.to(device), max_new_tokens=96)
        return proc.batch_decode(ids, skip_special_tokens=True)

    clips = sample(read_clips(args.clips), args.count, "calibrate")
    buckets = {}
    out = open(args.out, "w") if args.out else None
    for k in range(0, len(clips), 16):
        part = clips[k : k + 16]
        clean = [load(p) for p, _ in part]
        aug = [augment(x, rng_for(p, 0, seed="calibrate"), pools, args.split, force=True) for x, (p, _) in zip(clean, part)]
        for (path, sentence), hc, (_, recipe), ha in zip(part, hear(clean), aug, hear([a for a, _ in aug])):
            ec, n = wer(sentence, hc)
            ea, _ = wer(sentence, ha)
            keys = ["all"] + [e for e in ("ptt", "room", "mic", "opus", "clip_db") if e in recipe]
            if "noise" in recipe:
                snr = recipe["snr"]
                keys += [f"noise:{recipe['noise']}", f"snr {int(snr // 5) * 5}-{int(snr // 5) * 5 + 5}"]
            for key in keys:
                b = buckets.setdefault(key, [0, 0, 0, 0])
                b[0] += ec
                b[1] += ea
                b[2] += n
                b[3] += 1
            if out:
                out.write(f"{path}\t{sentence}\t{hc}\t{ha}\t{json.dumps(recipe)}\n")
        print(f"{min(k + 16, len(clips))}/{len(clips)}", flush=True)
    print(f"{'':12} {'clips':>6} {'clean WER':>10} {'augmented':>10}")
    for key, (ec, ea, n, c) in sorted(buckets.items()):
        print(f"{key:12} {c:6} {ec / n:10.3f} {ea / n:10.3f}")


DEMO = r"""<title>Augmentation Check</title>
<meta name="claude:viewport-fit-sides" content="cover">
<style>
:root { --bg: #f4f3ef; --surface: #fff; --ink: #1f2124; --muted: #62666d; --line: #d9d7d0; --accent: #2d5f8a; --pick: #2d5f8a; --pick-ink: #fff; }
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { --bg: #16181b; --surface: #1f2226; --ink: #e7e5df; --muted: #9a9ea6; --line: #33373d; --accent: #8fb8de; --pick: #8fb8de; --pick-ink: #10161c; color-scheme: dark } }
:root[data-theme="dark"] { --bg: #16181b; --surface: #1f2226; --ink: #e7e5df; --muted: #9a9ea6; --line: #33373d; --accent: #8fb8de; --pick: #8fb8de; --pick-ink: #10161c; color-scheme: dark }
body { background: var(--bg); color: var(--ink); font: 15px/1.5 system-ui, sans-serif; padding-block: 24px 48px; padding-left: max(16px, env(safe-area-inset-left, 0px)); padding-right: max(16px, env(safe-area-inset-right, 0px)); }
main { max-width: 760px; margin: 0 auto; display: grid; gap: 14px; }
h1 { font-size: 1.5rem; margin: 0; text-wrap: balance; }
.lede { color: var(--muted); margin: 0; }
section { background: var(--surface); border: 1px solid var(--line); border-radius: 10px; padding: 14px; display: grid; gap: 8px; }
.s { margin: 0; font-weight: 600; }
.row { display: flex; flex-wrap: wrap; gap: 12px; }
.row label { display: grid; gap: 4px; font-size: 12px; text-transform: uppercase; letter-spacing: .05em; color: var(--muted); min-width: 0; flex: 1 1 260px; }
audio { width: 100%; }
.r { margin: 0; font: 12px/1.4 ui-monospace, Menlo, monospace; color: var(--muted); }
.rate { display: flex; gap: 8px; flex-wrap: wrap; }
.rate button { font: inherit; padding: 4px 12px; border-radius: 999px; border: 1px solid var(--line); background: transparent; color: var(--ink); cursor: pointer; }
.rate button.on { background: var(--pick); color: var(--pick-ink); border-color: var(--pick); }
#sum { position: sticky; bottom: 0; background: var(--surface); border: 1px solid var(--line); border-radius: 10px; padding: 10px 14px; font: 13px ui-monospace, Menlo, monospace; display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
#sum code { flex: 1 1 200px; min-width: 0; overflow-wrap: anywhere; }
#sum button { font: inherit; padding: 4px 12px; border-radius: 6px; border: 1px solid var(--accent); background: transparent; color: var(--accent); cursor: pointer; }
</style>
<main>
<h1>Augmentation check</h1>
<p class="lede">{count} training clips, each clean and as the training run might hear it. Mark each augmented take: fine, too hard (a player couldn't be understood either), or unrealistic (no microphone sounds like that). Each take plays at the clean one's loudness; the level it was recorded at is in its description. Copy the line at the bottom back.</p>
{items}
<div id="sum"><code id="line">no marks yet</code><button id="copy">Copy</button></div>
</main>
<script>
const marks = {};
document.querySelectorAll('.rate').forEach(r => r.addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b) return;
  r.querySelectorAll('button').forEach(x => x.classList.toggle('on', x === b));
  marks[r.dataset.i] = b.dataset.v; draw();
}));
function draw() {
  const ks = Object.keys(marks).sort((a, b) => a - b);
  document.getElementById('line').textContent = ks.length ? 'augment marks: ' + ks.map(k => k + '=' + marks[k]).join(' ') : 'no marks yet';
}
document.getElementById('copy').addEventListener('click', () => {
  const t = document.getElementById('line').textContent;
  navigator.clipboard.writeText(t).catch(() => { const r = document.createRange(); r.selectNodeContents(document.getElementById('line')); getSelection().removeAllRanges(); getSelection().addRange(r); });
});
</script>
"""


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(required=True)
    p = sub.add_parser("pools")
    p.add_argument("--dir", required=True)
    p.set_defaults(fn=cmd_pools)
    p = sub.add_parser("clips")
    p.add_argument("--fish", nargs="*")
    p.add_argument("--kokoro", nargs="*")
    p.add_argument("--split", default="train")
    p.add_argument("--out", required=True)
    p.set_defaults(fn=cmd_clips)
    p = sub.add_parser("demo")
    p.add_argument("--dir", required=True)
    p.add_argument("--clips", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--count", type=int, default=20)
    p.set_defaults(fn=cmd_demo)
    p = sub.add_parser("testset")
    p.add_argument("--dir", required=True)
    p.add_argument("--clips", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--count", type=int, default=3000)
    p.set_defaults(fn=cmd_testset)
    p = sub.add_parser("calibrate")
    p.add_argument("--dir", required=True)
    p.add_argument("--clips", required=True)
    p.add_argument("--whisper", required=True)
    p.add_argument("--split", default="test")
    p.add_argument("--count", type=int, default=400)
    p.add_argument("--out")
    p.set_defaults(fn=cmd_calibrate)
    args = ap.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
