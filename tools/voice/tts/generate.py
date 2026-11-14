"""Speak the bake-off's lines with one TTS model (docs/COMMANDS.md section 11, "Choosing a TTS"):

    python tools/voice/tts/generate.py --model audio8|fish|chatterbox|kokoro --lines <dir>/lines.tsv --out <dir>/audio

Each line is spoken in its assigned reference speaker's voice (cloned from `refs/<clip>` with its
transcript; Kokoro, which cannot clone, uses a preset voice of the same sex instead) and written
as `<out>/<model>/<id>.flac`, 16 kHz mono, the format training uses. Lines already written are
skipped, so a run resumes. `<out>/<model>/timing.tsv` records the seconds each line took.

The models (their code is each one's own, installed by `setup_machine.sh`):
  audio8      Edge0/Audio8-TTS-Realistic-Air (Apache-2.0), pinned revision, trust_remote_code
              (its code was read for whisper-t5-bridge: no network, subprocess or exec).
  fish        Fish Audio S2-Pro (Fish Audio Research License: research and non-commercial use),
              through fish-speech's API server, started separately on :8080.
  chatterbox  Resemble AI's Chatterbox Turbo (MIT; retired August 2026; watermarked output).
  kokoro      hexgrad Kokoro-82M (Apache-2.0), preset voices only; runs on the Mac.
"""

import argparse
import time
from pathlib import Path

import numpy as np
import soundfile as sf

AUDIO8 = ("Edge0/Audio8-TTS-Realistic-Air", "23c305cc013fde378edb459f538317043c993a74")
KOKORO_VOICES = {"F": ["af_heart", "af_bella", "af_nicole", "bf_emma", "bf_isabella"], "M": ["am_michael", "am_fenrir", "am_puck", "bm_george", "bm_fable"]}


def to_16k(audio, sr):
    import torch
    import torchaudio.functional as AF

    audio = np.asarray(audio, dtype=np.float32)
    if audio.ndim > 1:
        audio = audio.mean(axis=-1 if audio.shape[-1] < audio.shape[0] else 0)
    if sr == 16000:
        return audio
    return AF.resample(torch.from_numpy(audio), sr, 16000).numpy()


class Audio8:
    def __init__(self):
        import torch
        from transformers import AutoModel, AutoProcessor

        self.torch = torch
        self.processor = AutoProcessor.from_pretrained(AUDIO8[0], revision=AUDIO8[1], trust_remote_code=True)
        self.model = AutoModel.from_pretrained(AUDIO8[0], revision=AUDIO8[1], trust_remote_code=True, dtype=torch.bfloat16).eval().to("cuda")
        self.sr = self.model.config.codec_sample_rate
        self.codes = {}

    def speak(self, text, ref, ref_text, sex):
        torch = self.torch
        if ref not in self.codes:
            import torchaudio.functional as AF

            audio, sr = sf.read(ref, dtype="float32")
            wav = AF.resample(torch.from_numpy(audio), sr, self.sr).to("cuda")
            codes, lengths = self.model.encode_audio(wav[None, None], torch.tensor([wav.numel()], device="cuda"))
            self.codes[ref] = codes[0, :, : int(lengths[0])].cpu().numpy()
        inputs = self.processor(text=[text], reference_codes=[self.codes[ref]], reference_text=[ref_text], return_tensors="pt")
        max_new = int(15 * self.sr / 2048) + 8
        width = int((inputs["prefix_attention_mask"].sum(1) + inputs["reference_code_lengths"] + inputs["suffix_attention_mask"].sum(1)).max())
        self.model.config.max_seq_len = min(2048, width + max_new + 1)
        inputs = {k: v.to("cuda") for k, v in inputs.items()}
        with torch.inference_mode():
            out = self.model.generate(**inputs, max_new_tokens=max_new, return_dict_in_generate=True, temperature=0.8, top_p=0.95, top_k=50, do_sample=True)
            wavs, lengths = self.model.decode_audio(out.codes)
        return to_16k(wavs[0, : int(lengths[0])].float().cpu().numpy(), self.sr)


class Fish:
    """fish-speech's API server (started by setup_machine.sh on :8080), one reference per speaker."""

    def __init__(self, url="http://127.0.0.1:8080"):
        import requests

        self.requests, self.url, self.added = requests, url, set()

    def speak(self, text, ref, ref_text, sex):
        rid = Path(ref).stem
        if rid not in self.added:
            with open(ref, "rb") as f:
                r = self.requests.post(f"{self.url}/v1/references/add", data={"id": rid, "text": ref_text}, files={"audio": (Path(ref).name, f, "audio/flac")}, timeout=120)
            if r.status_code not in (200, 201, 409):
                raise RuntimeError(f"adding reference {rid}: {r.status_code} {r.text[:200]}")
            self.added.add(rid)
        r = self.requests.post(f"{self.url}/v1/tts", json={"text": text, "reference_id": rid, "format": "wav", "streaming": False}, timeout=300)
        r.raise_for_status()
        import io

        audio, sr = sf.read(io.BytesIO(r.content), dtype="float32")
        return to_16k(audio, sr)


class Chatterbox:
    def __init__(self):
        from chatterbox.tts_turbo import ChatterboxTurboTTS

        self.model = ChatterboxTurboTTS.from_pretrained(device="cuda")

    def speak(self, text, ref, ref_text, sex):
        wav = self.model.generate(text, audio_prompt_path=str(ref))
        return to_16k(wav.squeeze().cpu().numpy(), self.model.sr)


class Kokoro:
    def __init__(self):
        from kokoro import KPipeline

        self.pipes = {"a": KPipeline(lang_code="a"), "b": KPipeline(lang_code="b")}
        self.voice_of = {}

    def speak(self, text, ref, ref_text, sex):
        pool = KOKORO_VOICES[sex]
        voice = self.voice_of.setdefault(ref, pool[len([v for v in self.voice_of.values() if v in pool]) % len(pool)])
        parts = [audio for _, _, audio in self.pipes[voice[0]](text, voice=voice)]
        audio = np.concatenate([np.asarray(p, dtype=np.float32) for p in parts]) if parts else np.zeros(1600, np.float32)
        return to_16k(audio, 24000)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", required=True, choices=["audio8", "fish", "chatterbox", "kokoro"])
    ap.add_argument("--lines", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    lines = Path(args.lines)
    refs = lines.parent / "refs"
    out = Path(args.out) / args.model
    out.mkdir(parents=True, exist_ok=True)
    tts = {"audio8": Audio8, "fish": Fish, "chatterbox": Chatterbox, "kokoro": Kokoro}[args.model]()
    timing = open(out / "timing.tsv", "a")
    done = 0
    for row in lines.read_text().splitlines():
        lid, text, _, _, speaker, clip, ref_text = row.split("\t")
        dest = out / f"{lid}.flac"
        if dest.exists():
            continue
        start = time.time()
        try:
            audio = tts.speak(text, refs / clip, ref_text, speaker[-1])
        except Exception as e:  # one bad line must not stop the run
            print(f"{lid}: {type(e).__name__}: {e}", flush=True)
            continue
        took = time.time() - start
        sf.write(dest, audio, 16000, format="FLAC")
        timing.write(f"{lid}\t{took:.3f}\t{len(audio) / 16000:.3f}\n")
        timing.flush()
        done += 1
        if done % 10 == 0:
            print(f"{args.model}: {done} lines", flush=True)
    print(f"{args.model}: done, {done} new lines in {out}", flush=True)


if __name__ == "__main__":
    main()
