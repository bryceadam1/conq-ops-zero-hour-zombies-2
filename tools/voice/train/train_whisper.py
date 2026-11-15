"""Whisper trained straight on commands (docs/COMMANDS.md section 11, step 13): the audio in, the
command's label out, with no transcript and no translator between.

    venv/bin/python tools/voice/train/train_whisper.py --whisper ~/.conquerops/voice/whisper-base.en \\
        --train <augment>/lists/train.tsv --test <augment>/lists/test.tsv --noisy <augment>/test_noisy \\
        --prompts <corpus>/prompts.tsv --augment <augment> --out <dir> [--epochs 3] [--limit-steps N]

Each example is what the game will send (`zhbo_voice::hear`): the clip, augmented as it loads
(`augment.py`, a different take each epoch), then 3 s of digital silence, as log-mel features
of that window alone, never Whisper's full 30 s (the encoder runs on the window's own positions,
as `hear::WINDOW_SILENCE` has it at run time); the decoder starts from `<|startofprev|>`, the
vocabulary's prompt (`voice_prompts`: `hear::prompt`, "Commands for ..."), then Whisper's start
tokens, and learns the label and end of text; the loss covers only those. SpecAugment masks the
features lightly.

The dev set is 1% of the training vocabularies (by a hash of the id), never trained on, so its
names are new to the model as the test set's are. Evaluates greedily on dev every `--eval-every`
steps and keeps the best by exact match in `<out>/best`; at the end scores the best on the
clean and the noisy test sets and writes their predictions (`test_predictions.tsv`,
`noisy_predictions.tsv`: path, label, prediction) for `voice_judge`.
"""

import argparse
import hashlib
import json
import math
import random
import sys
import time
from pathlib import Path

import numpy as np
import torch
from transformers import WhisperForConditionalGeneration, WhisperProcessor
from transformers.modeling_outputs import BaseModelOutput

sys.path.insert(0, str(Path(__file__).parent))
import augment as A  # noqa: E402

SR = A.SR
HOP = 160
WINDOW_SILENCE = 300  # frames, hear::WINDOW_SILENCE
N_SAMPLES = 30 * SR


def h(key):
    return int.from_bytes(hashlib.sha256(key.encode()).digest()[:8], "little")


def window_samples(x):
    """The clip and 3 s of silence after it, to a whole number of frame pairs (hear::window_samples)."""
    n = min(-(-(len(x) + WINDOW_SILENCE * HOP) // (2 * HOP)) * 2 * HOP, N_SAMPLES)
    out = np.zeros(n, np.float32)
    out[: min(len(x), n)] = x[:n]
    return out


def log_mel(x, filters):
    """openai/whisper's log_mel_spectrogram (hear::log_mel): `[n_mels, samples / 160]`."""
    audio = torch.from_numpy(x)
    stft = torch.stft(audio, 400, HOP, window=torch.hann_window(400), return_complex=True)
    power = stft[..., :-1].abs() ** 2
    mel = filters @ power
    log = torch.clamp(mel, min=1e-10).log10()
    log = torch.maximum(log, log.max() - 8.0)
    return ((log + 4.0) / 4.0).numpy()


def read_rows(path):
    rows = []
    for line in Path(path).read_text().splitlines():
        p = line.split("\t")
        rows.append({"path": p[0], "sentence": p[1], "label": p[2], "vid": p[3], "seconds": float(p[4])})
    return rows


class Clips(torch.utils.data.Dataset):
    """Rows as the model's inputs: features (augmented in `augment` mode) and token ids."""

    def __init__(self, rows, tok, prompts, filters, pools=None, split="train"):
        self.rows, self.tok, self.prompts, self.filters, self.pools, self.split = rows, tok, prompts, filters, pools, split
        self.epoch = 0
        self.prev = tok.convert_tokens_to_ids("<|startofprev|>")
        self.start = tok.convert_tokens_to_ids(["<|startoftranscript|>", "<|notimestamps|>"])
        self.eot = tok.convert_tokens_to_ids("<|endoftext|>")
        self.room = 448 // 2 - 1  # the prompt may use half the decoder's context (hear.rs)

    def prefix(self, vid):
        ids = self.tok.encode(" " + self.prompts[vid].strip(), add_special_tokens=False)
        return [self.prev] + ids[-self.room :] + self.start

    def __len__(self):
        return len(self.rows)

    def __getitem__(self, i):
        r = self.rows[i]
        x = A.load(r["path"])
        frames = None
        if self.pools is not None:
            x, _ = A.augment(x, A.rng_for(r["path"], self.epoch), self.pools, self.split)
            frames = len(x) // HOP
        mel = log_mel(window_samples(x), self.filters)
        if self.pools is not None:
            mel = A.spec_augment(mel, A.rng_for("spec:" + r["path"], self.epoch), frames)
        prefix = self.prefix(r["vid"])
        target = self.tok.encode(" " + r["label"], add_special_tokens=False) + [self.eot]
        return {"mel": mel, "prefix": prefix, "target": target, "i": i}


def collate(batch):
    """Features zero-padded to the longest window (silence's log-mel is the floor, so padding is
    padding), decoder inputs and labels with the prefix masked out of the loss."""
    frames = max(b["mel"].shape[1] for b in batch)
    mel = np.stack([np.pad(b["mel"], ((0, 0), (0, frames - b["mel"].shape[1])), constant_values=b["mel"].min()) for b in batch])
    seqs = [b["prefix"] + b["target"] for b in batch]
    n = max(len(s) for s in seqs) - 1
    inputs = torch.full((len(batch), n), 50256, dtype=torch.long)
    labels = torch.full((len(batch), n), -100, dtype=torch.long)
    for k, (b, s) in enumerate(zip(batch, seqs)):
        inputs[k, : len(s) - 1] = torch.tensor(s[:-1])
        start = len(b["prefix"]) - 1
        labels[k, start : len(s) - 1] = torch.tensor(s[len(b["prefix"]) :])
    return {"mel": torch.from_numpy(mel), "inputs": inputs, "labels": labels, "prefix": [b["prefix"] for b in batch], "i": [b["i"] for b in batch]}


def encode(model, mel):
    """Whisper's encoder on the window's own positions (transformers insists on 30 s)."""
    enc = model.model.encoder
    x = torch.nn.functional.gelu(enc.conv1(mel))
    x = torch.nn.functional.gelu(enc.conv2(x)).permute(0, 2, 1)
    x = x + enc.embed_positions.weight[: x.shape[1]]
    x = torch.nn.functional.dropout(x, p=enc.dropout, training=enc.training)
    for layer in enc.layers:
        x = layer(x, None)
        x = x[0] if isinstance(x, tuple) else x
    return enc.layer_norm(x)


def loss_of(model, batch, device):
    """Cross-entropy over the label's tokens. The vocabulary's 51,864 logits are taken only where the
    loss looks: at every position (the prompt is most of each row) they held tens of GB."""
    states = encode(model, batch["mel"].to(device))
    hidden = model.model.decoder(input_ids=batch["inputs"].to(device), encoder_hidden_states=states).last_hidden_state
    labels = batch["labels"].to(device)
    keep = labels != -100
    return torch.nn.functional.cross_entropy(model.proj_out(hidden[keep]).float(), labels[keep])


@torch.no_grad()
def greedy(model, batch, device, eot, max_new=64):
    """Greedy decoding from each row's prefix with the decoder's cache, text tokens only (hear.rs:
    nothing past end of text). Rows are left-padded, masked, and given their own positions from 0,
    so each sees its prefix exactly as the game lays it out."""
    states = encode(model, batch["mel"].to(device))
    dec = model.model.decoder
    prefixes = batch["prefix"]
    n = max(len(p) for p in prefixes)
    ids = torch.full((len(prefixes), n), eot, dtype=torch.long)
    mask = torch.zeros((len(prefixes), n), dtype=torch.long)
    pos = torch.zeros((len(prefixes), n), dtype=torch.long)
    for k, p in enumerate(prefixes):
        ids[k, n - len(p) :] = torch.tensor(p)
        mask[k, n - len(p) :] = 1
        pos[k, n - len(p) :] = torch.arange(len(p))
    ids, mask, pos = ids.to(device), mask.to(device), pos.to(device)
    step = dec(input_ids=ids, attention_mask=mask, position_ids=pos, encoder_hidden_states=states, use_cache=True)
    last = pos[:, -1:]
    done = torch.zeros(len(prefixes), dtype=torch.bool, device=device)
    out = []
    for _ in range(max_new):
        nxt = model.proj_out(step.last_hidden_state[:, -1])[:, : eot + 1].argmax(-1)
        nxt = torch.where(done, torch.full_like(nxt, eot), nxt)
        out.append(nxt)
        done |= nxt == eot
        if bool(done.all()):
            break
        mask = torch.cat([mask, torch.ones_like(mask[:, :1])], 1)
        last = last + 1
        step = dec(input_ids=nxt[:, None], attention_mask=mask, position_ids=last, encoder_hidden_states=states,
                   past_key_values=step.past_key_values, use_cache=True)
    tokens = torch.stack(out, 1).tolist()
    return [row[: row.index(eot)] if eot in row else row for row in tokens]


def evaluate(model, loader, tok, device, rows, path=None):
    model.eval()
    eot = tok.convert_tokens_to_ids("<|endoftext|>")
    right, total, lines = 0, 0, []
    for batch in loader:
        for i, ids in zip(batch["i"], greedy(model, batch, device, eot)):
            pred = tok.decode(ids).strip()
            label = rows[i]["label"]
            right += pred == label
            total += 1
            lines.append(f"{rows[i]['path']}\t{label}\t{pred}")
    if path:
        Path(path).write_text("\n".join(lines) + "\n")
    model.train()
    return right / max(total, 1)


def batches_by_length(rows, size, seed):
    """Batches of rows of similar length, in a shuffled order: less padding, the same work."""
    order = sorted(range(len(rows)), key=lambda i: (rows[i]["seconds"], h(f"{seed}:{i}")))
    chunks = [order[k : k + size] for k in range(0, len(order), size)]
    random.Random(seed).shuffle(chunks)
    return chunks


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--whisper", required=True)
    ap.add_argument("--train", required=True)
    ap.add_argument("--test", required=True)
    ap.add_argument("--noisy")
    ap.add_argument("--prompts", required=True)
    ap.add_argument("--augment", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--epochs", type=int, default=3)
    ap.add_argument("--batch", type=int, default=32)
    ap.add_argument("--lr", type=float, default=3e-5)
    ap.add_argument("--warmup", type=int, default=500)
    ap.add_argument("--eval-every", type=int, default=2000)
    ap.add_argument("--dev-count", type=int, default=1000)
    ap.add_argument("--workers", type=int, default=7)
    ap.add_argument("--limit-steps", type=int, default=0)
    ap.add_argument("--bf16", action="store_true")
    ap.add_argument("--no-final", action="store_true", help="skip dev and test at the end (timing runs)")
    args = ap.parse_args()

    device = "mps" if torch.backends.mps.is_available() else "cpu"
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    proc = WhisperProcessor.from_pretrained(args.whisper)
    tok = proc.tokenizer
    filters = torch.from_numpy(np.asarray(proc.feature_extractor.mel_filters, dtype=np.float32).T)
    prompts = dict(l.split("\t", 1) for l in Path(args.prompts).read_text().splitlines())
    pools = A.Pools(args.augment)

    rows = read_rows(args.train)
    dev_rows = [r for r in rows if h("dev:" + r["vid"]) % 100 == 0]
    train_rows = [r for r in rows if h("dev:" + r["vid"]) % 100 != 0]
    dev_rows = sorted(dev_rows, key=lambda r: h(r["path"]))[: args.dev_count]
    test_rows = read_rows(args.test)
    print(f"train {len(train_rows)}, dev {len(dev_rows)} (vocabularies held out), test {len(test_rows)}", flush=True)

    train = Clips(train_rows, tok, prompts, filters, pools, "train")
    loader_kw = dict(collate_fn=collate, num_workers=args.workers, prefetch_factor=4 if args.workers else None)
    dev_loader = torch.utils.data.DataLoader(Clips(dev_rows, tok, prompts, filters), batch_size=64, **loader_kw)

    model = WhisperForConditionalGeneration.from_pretrained(args.whisper).to(device)
    model.config.forced_decoder_ids = None
    model.train()
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=0.01)
    steps_per_epoch = math.ceil(len(train_rows) / args.batch)
    total = args.limit_steps or steps_per_epoch * args.epochs
    sched = torch.optim.lr_scheduler.LambdaLR(opt, lambda s: min(1.0, (s + 1) / args.warmup) * max(0.0, 1 - s / total))
    log = open(out / "train.log", "a")
    best, step, start = -1.0, 0, time.time()
    for epoch in range(args.epochs):
        train.epoch = epoch
        loader = torch.utils.data.DataLoader(train, batch_sampler=batches_by_length(train_rows, args.batch, epoch), **loader_kw)
        running = 0.0
        for batch in loader:
            with torch.autocast(device, dtype=torch.bfloat16, enabled=args.bf16):
                loss = loss_of(model, batch, device)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()
            sched.step()
            opt.zero_grad(set_to_none=True)
            step += 1
            running += loss.item()
            if step % 50 == 0:
                rate = step * args.batch / (time.time() - start)
                msg = f"epoch {epoch} step {step}/{total} loss {running / 50:.4f} lr {sched.get_last_lr()[0]:.2e} {rate:.1f} clips/s"
                print(msg, flush=True)
                log.write(msg + "\n")
                log.flush()
                running = 0.0
            if step % args.eval_every == 0 or (step == total and not args.no_final):
                acc = evaluate(model, dev_loader, tok, device, dev_rows)
                msg = f"step {step}: dev exact match {acc:.4f}" + (" (best)" if acc > best else "")
                print(msg, flush=True)
                log.write(msg + "\n")
                log.flush()
                if acc > best:
                    best = acc
                    model.save_pretrained(out / "best")
                    proc.save_pretrained(out / "best")
            if step >= total:
                break
        if step >= total:
            break

    if args.no_final:
        return
    model = WhisperForConditionalGeneration.from_pretrained(out / "best").to(device)
    results = {"dev": best}
    test_loader = torch.utils.data.DataLoader(Clips(test_rows, tok, prompts, filters), batch_size=64, **loader_kw)
    results["test"] = evaluate(model, test_loader, tok, device, test_rows, out / "test_predictions.tsv")
    if args.noisy:
        by_path = {r["path"]: r for r in test_rows}
        noisy_rows = []
        for line in (Path(args.noisy) / "manifest.tsv").read_text().splitlines():
            name, _, src, _ = line.split("\t")
            r = dict(by_path[src])
            r["path"] = str(Path(args.noisy) / name)
            noisy_rows.append(r)
        noisy_loader = torch.utils.data.DataLoader(Clips(noisy_rows, tok, prompts, filters), batch_size=64, **loader_kw)
        results["noisy"] = evaluate(model, noisy_loader, tok, device, noisy_rows, out / "noisy_predictions.tsv")
    (out / "results.json").write_text(json.dumps(results, indent=2))
    print(json.dumps(results), flush=True)


if __name__ == "__main__":
    main()
