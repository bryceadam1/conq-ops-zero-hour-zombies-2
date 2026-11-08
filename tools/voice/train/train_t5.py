"""Fine-tune a T5-style model to translate spoken commands into the canonical command language
(docs/COMMANDS.md section 11).

    venv/bin/python tools/voice/train/train_t5.py --data tools/voice --out ~/.conquerops/voice/flan-t5-small

Reads `train.tsv` / `dev.tsv` / `test.tsv` (`input<TAB>label<TAB>id`, built by `voice_corpus
assemble`; the id names the row's vocabulary in `vocab.tsv`) and, if present, `heard.tsv` (spoken
training inputs as Whisper and the snapper heard them, from the `voice_heard` example),
fine-tunes `--model` (google/flan-t5-small by default), checks exact match on dev after each epoch
with greedy decoding (no grammar mask: the mask only helps, so this is the floor), keeps the best
epoch, and writes it as safetensors with its tokenizer and `words.txt` (the ordinary words, for the
voice snapper), plus `eval.txt` (dev and test exact match,
decode speed, and a sample of the errors). The model is the player's to download at install; it
never ships in the repo.
"""

import argparse
import math
import random
import time
from pathlib import Path

import torch
from transformers import AutoModelForSeq2SeqLM, AutoTokenizer

PREFIX = "command: "


def read(path: Path):
    rows = []
    for line in path.read_text().splitlines():
        cols = line.split("\t")
        if len(cols) >= 2:
            rows.append((cols[0].strip(), cols[1].strip()))
    return rows


def batches(rows, size, shuffle):
    order = list(range(len(rows)))
    if shuffle:
        random.shuffle(order)
    for i in range(0, len(order), size):
        yield [rows[j] for j in order[i : i + size]]


def encode(tok, chunk, device, max_len):
    src = tok([PREFIX + s for s, _ in chunk], padding=True, truncation=True, max_length=max_len, return_tensors="pt")
    tgt = tok([t for _, t in chunk], padding=True, truncation=True, max_length=max_len, return_tensors="pt")
    labels = tgt.input_ids.clone()
    labels[labels == tok.pad_token_id] = -100
    return src.input_ids.to(device), src.attention_mask.to(device), labels.to(device)


@torch.no_grad()
def evaluate(model, tok, rows, device, max_len, size=64):
    model.eval()
    right, wrong, tokens = 0, [], 0
    start = time.time()
    for chunk in batches(rows, size, False):
        src = tok([PREFIX + s for s, _ in chunk], padding=True, truncation=True, max_length=max_len, return_tensors="pt").to(device)
        out = model.generate(**src, max_new_tokens=max_len, num_beams=1, do_sample=False)
        tokens += int((out != tok.pad_token_id).sum())
        for (s, t), o in zip(chunk, tok.batch_decode(out, skip_special_tokens=True)):
            if o.strip() == t:
                right += 1
            else:
                wrong.append((s, t, o.strip()))
    took = time.time() - start
    model.train()
    return right / max(len(rows), 1), wrong, tokens / max(took, 1e-9), took / max(len(rows), 1)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", default="tools/voice")
    ap.add_argument("--out", required=True)
    ap.add_argument("--model", default="google/flan-t5-small")
    ap.add_argument("--epochs", type=int, default=6)
    ap.add_argument("--batch", type=int, default=32)
    ap.add_argument("--lr", type=float, default=5e-4)
    ap.add_argument("--max-len", type=int, default=64)
    ap.add_argument("--seed", type=int, default=20261107)
    ap.add_argument("--dev-sample", type=int, default=600, help="dev rows checked after each epoch (all at the end)")
    args = ap.parse_args()

    random.seed(args.seed)
    torch.manual_seed(args.seed)
    device = "mps" if torch.backends.mps.is_available() else "cuda" if torch.cuda.is_available() else "cpu"
    data = Path(args.data)
    train, dev, test = read(data / "train.tsv"), read(data / "dev.tsv"), read(data / "test.tsv")
    # Training inputs as the game hears them (`voice_heard`): Whisper's transcripts, snapped.
    heard = read(data / "heard.tsv") if (data / "heard.tsv").exists() else []
    train += heard
    print(f"{len(train)} train ({len(heard)} heard), {len(dev)} dev, {len(test)} test rows on {device}", flush=True)

    tok = AutoTokenizer.from_pretrained(args.model)
    model = AutoModelForSeq2SeqLM.from_pretrained(args.model).to(device)
    # Every character the labels use must survive the tokenizer round trip, or exact match is
    # impossible however well the model learns.
    lost = [t for _, t in train[:2000] if tok.decode(tok(t).input_ids, skip_special_tokens=True) != t]
    print(f"labels the tokenizer cannot round-trip: {len(lost)}" + (f" (e.g. {lost[0]!r})" if lost else ""), flush=True)

    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=0.01)
    steps = args.epochs * math.ceil(len(train) / args.batch)
    warm = max(1, steps // 20)
    sched = torch.optim.lr_scheduler.LambdaLR(opt, lambda s: min(1.0, (s + 1) / warm) * max(0.0, (steps - s) / max(1, steps - warm)))
    dev_check = random.sample(dev, min(args.dev_sample, len(dev)))
    out = Path(args.out).expanduser()
    best = -1.0
    step = 0
    model.train()
    for epoch in range(1, args.epochs + 1):
        start, total = time.time(), 0.0
        for chunk in batches(train, args.batch, True):
            ids, mask, labels = encode(tok, chunk, device, args.max_len)
            loss = model(input_ids=ids, attention_mask=mask, labels=labels).loss
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()
            sched.step()
            opt.zero_grad()
            total += loss.item()
            step += 1
            if step % 200 == 0:
                print(f"  step {step}/{steps} loss {float(loss):.4f}", flush=True)
        acc, _, _, _ = evaluate(model, tok, dev_check, device, args.max_len)
        n = math.ceil(len(train) / args.batch)
        print(f"epoch {epoch}: mean loss {total / n:.4f}, dev exact match {acc:.3f} ({len(dev_check)} rows), {time.time() - start:.0f} s", flush=True)
        if acc > best:
            best = acc
            out.mkdir(parents=True, exist_ok=True)
            # The checkpoint keeps its own lm_head (transformers did not tie it to the shared
            # embeddings): say so, or a loader that trusts the config decodes through the wrong
            # matrix.
            model.config.tie_word_embeddings = False
            model.save_pretrained(out, safe_serialization=True)
            tok.save_pretrained(out)
            # The ordinary words go with the model: the snapper leaves them as heard.
            (out / "words.txt").write_bytes((data / "words.txt").read_bytes())
            print(f"  kept epoch {epoch}", flush=True)

    model = AutoModelForSeq2SeqLM.from_pretrained(out).to(device)
    lines = [f"model: {args.model} fine-tuned, best dev sample {best:.3f}"]
    for name, rows in (("dev", dev), ("test", test)):
        acc, wrong, tps, per = evaluate(model, tok, rows, device, args.max_len)
        lines.append(f"{name}: exact match {acc:.4f} of {len(rows)}; {tps:.0f} tokens/s, {per * 1000:.1f} ms per command (batched 64, {device})")
        if name == "test":
            lines.append("test errors (input / label / output), first 40:")
            lines += [f"  {s}\n    want {t}\n    got  {o}" for s, t, o in wrong[:40]]
    # One command at a time on the CPU: what a player's machine does with the game running.
    cpu = model.to("cpu")
    start = time.time()
    for s, _ in test[:50]:
        src = tok(PREFIX + s, return_tensors="pt")
        cpu.generate(**src, max_new_tokens=args.max_len, num_beams=1, do_sample=False)
    lines.append(f"one command at a time on the CPU: {(time.time() - start) / 50 * 1000:.0f} ms each (50 test inputs)")
    (out / "eval.txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines[:3] + lines[-1:]), flush=True)


if __name__ == "__main__":
    main()
