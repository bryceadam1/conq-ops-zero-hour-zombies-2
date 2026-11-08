"""Audio straight to the command language (docs/COMMANDS.md section 11, "Audio straight to the
translator"): Whisper's encoder, a trained bridge, and the fine-tuned translator's decoder, with no
Whisper decoder in between.

    venv/bin/python tools/voice/train/train_bridge.py --audio ~/.conquerops/voice/audio \\
        --data tools/voice --t5 ~/.conquerops/voice/commands-t5-small --mode audio --out <dir>

Reads the features `voice_audio` wrote (`train.tsv.N` / `train.bf16.N` shards, `dev`, `test`:
each row's input, label, vocabulary id, offset and positions, the values bf16 `[positions, 512]`).

`--mode audio` (the decoder sees the bridged audio alone) or `--mode both` (it sees flan-T5's
encoding of an instruction with the match's names, from `names.tsv`, followed by the bridged
audio). The bridge: a stride-2 convolution (halving Whisper's 50 positions a second) and two
transformer layers. Trained with the decoder (and, in `both`, the T5 encoder); Whisper's encoder
is fixed (its features are precomputed). Keeps the best dev epoch (greedy exact match), and writes
the test rows' predictions (`test_predictions.tsv`: id, label, prediction) for `voice_judge`.
"""

import argparse
import math
import random
import time
from pathlib import Path

import numpy as np
import torch
from torch import nn
from transformers import AutoModelForSeq2SeqLM, AutoTokenizer
from transformers.modeling_outputs import BaseModelOutput


def load_split(audio: Path, name: str):
    """Rows (input, label, id, features as a bf16 tensor) from a split's shards."""
    rows = []
    tsvs = sorted(audio.glob(f"{name}.tsv*"))
    for tsv in tsvs:
        suffix = tsv.name[len(f"{name}.tsv"):]
        values = np.memmap(audio / f"{name}.bf16{suffix}", dtype=np.uint16, mode="r")
        for line in tsv.read_text().splitlines():
            f = line.split("\t")
            if len(f) < 5:
                continue
            offset, positions = int(f[3]), int(f[4])
            raw = np.array(values[offset : offset + positions * 512])
            feats = torch.from_numpy(raw.astype(np.int16)).view(torch.bfloat16).view(positions, 512)
            rows.append((f[0], f[1], f[2], feats))
    return rows


class Bridge(nn.Module):
    """Whisper's encoder states to states T5's decoder can attend to."""

    def __init__(self, d_in=512, d_model=512, layers=2, heads=8):
        super().__init__()
        self.down = nn.Conv1d(d_in, d_model, kernel_size=3, stride=2, padding=1)
        layer = nn.TransformerEncoderLayer(d_model, heads, 4 * d_model, dropout=0.1, batch_first=True, norm_first=True)
        self.layers = nn.TransformerEncoder(layer, layers)
        self.norm = nn.LayerNorm(d_model)

    def forward(self, x, mask):
        # x [batch, positions, 512], mask [batch, positions] (1 = audio)
        y = nn.functional.gelu(self.down(x.transpose(1, 2))).transpose(1, 2)
        m = mask[:, ::2][:, : y.shape[1]]
        y = self.layers(y, src_key_padding_mask=(m == 0))
        return self.norm(y), m


class AudioTranslator(nn.Module):
    def __init__(self, t5, mode):
        super().__init__()
        self.t5, self.mode = t5, mode
        self.bridge = Bridge(d_model=t5.config.d_model)

    def states(self, feats, fmask, text_ids=None, text_mask=None):
        audio, amask = self.bridge(feats, fmask)
        if self.mode == "both":
            text = self.t5.encoder(input_ids=text_ids, attention_mask=text_mask).last_hidden_state
            return torch.cat([text, audio], 1), torch.cat([text_mask, amask], 1)
        return audio, amask

    def forward(self, feats, fmask, labels, text_ids=None, text_mask=None):
        states, mask = self.states(feats, fmask, text_ids, text_mask)
        return self.t5(encoder_outputs=BaseModelOutput(last_hidden_state=states), attention_mask=mask, labels=labels).loss

    @torch.no_grad()
    def generate(self, feats, fmask, text_ids=None, text_mask=None, max_len=64):
        states, mask = self.states(feats, fmask, text_ids, text_mask)
        return self.t5.generate(encoder_outputs=BaseModelOutput(last_hidden_state=states), attention_mask=mask, max_new_tokens=max_len, num_beams=1, do_sample=False)


def batch(rows, tok, names, prompt, device, mode, max_len=64):
    longest = max(r[3].shape[0] for r in rows)
    feats = torch.zeros(len(rows), longest, 512, dtype=torch.float32)
    fmask = torch.zeros(len(rows), longest, dtype=torch.long)
    for i, r in enumerate(rows):
        feats[i, : r[3].shape[0]] = r[3].float()
        fmask[i, : r[3].shape[0]] = 1
    tgt = tok([r[1] for r in rows], padding=True, truncation=True, max_length=max_len, return_tensors="pt")
    labels = tgt.input_ids.clone()
    labels[labels == tok.pad_token_id] = -100
    out = {"feats": feats.to(device), "fmask": fmask.to(device), "labels": labels.to(device)}
    if mode == "both":
        text = tok([prompt.replace("{names}", names.get(r[2], "")) for r in rows], padding=True, truncation=True, max_length=384, return_tensors="pt")
        out["text_ids"], out["text_mask"] = text.input_ids.to(device), text.attention_mask.to(device)
    return out


def predict(model, rows, tok, names, prompt, device, mode, size=32):
    model.eval()
    out = []
    for i in range(0, len(rows), size):
        b = batch(rows[i : i + size], tok, names, prompt, device, mode)
        b.pop("labels")
        gen = model.generate(**b)
        out += [s.strip() for s in tok.batch_decode(gen, skip_special_tokens=True)]
    model.train()
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--audio", required=True)
    ap.add_argument("--data", default="tools/voice")
    ap.add_argument("--t5", required=True, help="the fine-tuned translator whose decoder (and encoder) start the model")
    ap.add_argument("--mode", choices=["audio", "both"], default="audio")
    ap.add_argument("--prompt", default="Names: {names}.")
    ap.add_argument("--out", required=True)
    ap.add_argument("--epochs", type=int, default=8)
    ap.add_argument("--batch", type=int, default=32)
    ap.add_argument("--lr", type=float, default=3e-4)
    ap.add_argument("--seed", type=int, default=20261110)
    args = ap.parse_args()

    random.seed(args.seed)
    torch.manual_seed(args.seed)
    device = "mps" if torch.backends.mps.is_available() else "cuda" if torch.cuda.is_available() else "cpu"
    audio, data, out = Path(args.audio).expanduser(), Path(args.data), Path(args.out).expanduser()
    out.mkdir(parents=True, exist_ok=True)
    names = dict(line.split("\t", 1) for line in (data / "names.tsv").read_text().splitlines() if "\t" in line)
    train, dev, test = load_split(audio, "train"), load_split(audio, "dev"), load_split(audio, "test")
    print(f"{len(train)} train, {len(dev)} dev, {len(test)} test utterances on {device}, mode {args.mode}", flush=True)

    tok = AutoTokenizer.from_pretrained(Path(args.t5).expanduser())
    t5 = AutoModelForSeq2SeqLM.from_pretrained(Path(args.t5).expanduser(), torch_dtype=torch.float32)
    model = AudioTranslator(t5, args.mode).to(device)
    if args.mode == "audio":
        for p in model.t5.encoder.parameters():
            p.requires_grad = False
    params = [p for p in model.parameters() if p.requires_grad]
    opt = torch.optim.AdamW(params, lr=args.lr, weight_decay=0.01)
    steps = args.epochs * math.ceil(len(train) / args.batch)
    warm = max(1, steps // 20)
    sched = torch.optim.lr_scheduler.LambdaLR(opt, lambda s: min(1.0, (s + 1) / warm) * max(0.0, (steps - s) / max(1, steps - warm)))
    best = -1.0
    model.train()
    for epoch in range(1, args.epochs + 1):
        start, total, n = time.time(), 0.0, 0
        order = list(range(len(train)))
        random.shuffle(order)
        for i in range(0, len(order), args.batch):
            b = batch([train[j] for j in order[i : i + args.batch]], tok, names, args.prompt, device, args.mode)
            loss = model(**b)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(params, 1.0)
            opt.step()
            sched.step()
            opt.zero_grad()
            total, n = total + loss.item(), n + 1
        preds = predict(model, dev, tok, names, args.prompt, device, args.mode)
        acc = sum(p == r[1] for p, r in zip(preds, dev)) / max(len(dev), 1)
        print(f"epoch {epoch}: mean loss {total / max(n, 1):.4f}, dev exact match {acc:.3f}, {time.time() - start:.0f} s", flush=True)
        if acc > best:
            best = acc
            torch.save(model.state_dict(), out / "audio_translator.pt")
            print(f"  kept epoch {epoch}", flush=True)

    model.load_state_dict(torch.load(out / "audio_translator.pt", map_location=device))
    preds = predict(model, test, tok, names, args.prompt, device, args.mode)
    (out / "test_predictions.tsv").write_text("".join(f"{r[2]}\t{r[1]}\t{p}\n" for r, p in zip(test, preds)))
    acc = sum(p == r[1] for p, r in zip(preds, test)) / max(len(test), 1)
    # One utterance at a time on the CPU: the bridge and the decoder, Whisper's encoder not counted.
    cpu = model.to("cpu").eval()
    start = time.time()
    for r in test[:30]:
        b = batch([r], tok, names, args.prompt, "cpu", args.mode)
        b.pop("labels")
        cpu.generate(**b)
    per = (time.time() - start) / 30 * 1000
    lines = [f"mode {args.mode}: best dev exact match {best:.3f}; test exact match {acc:.4f} of {len(test)}", f"bridge and decoder on the CPU, one at a time: {per:.0f} ms"]
    (out / "eval.txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines), flush=True)


if __name__ == "__main__":
    main()
