"""Fine-tune a pretrained whisper-t5-bridge bridge on spoken commands (docs/COMMANDS.md section 11,
"Audio straight to the translator"):

    venv/bin/python tools/voice/train/finetune_bridge.py --bridge-repo ~/Documents/Projects/whisper-t5-bridge \\
        --bridge ~/Documents/Projects/whisper-t5-bridge/release_v3/bridge.safetensors \\
        --audio ~/.conquerops/voice/audio2 --data tools/voice --t5 google/flan-t5-small --decoder lora --out <dir>

The bridge (whisper-t5-bridge, trained by distillation against flan-t5-small reading text) maps
Whisper base.en's encoder states into flan-t5-small's encoder-output space. Here the decoder sees
T5's encoding of an instruction carrying the match's names (`names.tsv`) followed by the bridged
audio, and learns to write the command. T5's encoder stays frozen (a trainable one learned to
ignore the audio before: modality collapse); the bridge trains at a low rate; the decoder is
frozen, given LoRA on its cross-attention (`--decoder lora`, as whisper-t5-bridge's stage 2), or
trained whole (`--decoder full`). `--t5` is the flan-t5-small the bridge was made for, or our
fine-tuned translator.

The features are `voice_audio`'s (computed with the openai-exact log-mel, `hear::log_mel`, so
they match what the bridge was trained on). Writes the best dev epoch's weights, the test rows'
predictions (`test_predictions.tsv`, for `voice_judge`) and the same with the audio replaced by
silence (`test_predictions_silent.tsv`: if the model really listens, those are much worse).
"""

import argparse
import math
import random
import sys
import time
from pathlib import Path

import torch
from transformers import AutoModelForSeq2SeqLM, AutoTokenizer
from transformers.modeling_outputs import BaseModelOutput

sys.path.insert(0, str(Path(__file__).parent))
from train_bridge import load_split  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bridge-repo", required=True)
    ap.add_argument("--bridge", required=True, help="the pretrained bridge's safetensors")
    ap.add_argument("--audio", required=True)
    ap.add_argument("--data", default="tools/voice")
    ap.add_argument("--t5", default="google/flan-t5-small")
    ap.add_argument("--decoder", choices=["frozen", "lora", "full"], default="lora")
    ap.add_argument("--prompt", default="Translate the spoken order into a command. Names: {names}. Order:")
    ap.add_argument("--out", required=True)
    ap.add_argument("--epochs", type=int, default=6)
    ap.add_argument("--batch", type=int, default=32)
    ap.add_argument("--lr-bridge", type=float, default=1e-4)
    ap.add_argument("--lr-decoder", type=float, default=3e-4)
    ap.add_argument("--seed", type=int, default=20261110)
    args = ap.parse_args()

    sys.path.insert(0, str(Path(args.bridge_repo).expanduser()))
    from whisper_t5_bridge.bridge import load_bridge, save_bridge
    from whisper_t5_bridge.student import add_cross_attn_lora, lora_state

    random.seed(args.seed)
    torch.manual_seed(args.seed)
    device = "mps" if torch.backends.mps.is_available() else "cpu"
    audio, data, out = Path(args.audio).expanduser(), Path(args.data), Path(args.out).expanduser()
    out.mkdir(parents=True, exist_ok=True)
    names = dict(line.split("\t", 1) for line in (data / "names.tsv").read_text().splitlines() if "\t" in line)
    train, dev, test = load_split(audio, "train"), load_split(audio, "dev"), load_split(audio, "test")
    t5_id = str(Path(args.t5).expanduser()) if Path(args.t5).expanduser().exists() else args.t5
    tok = AutoTokenizer.from_pretrained(t5_id)
    # Apple's GPU has no fused attention with dropout: T5 runs its plain attention (keeping its
    # dropout), and the bridge's layers train without theirs (one setting covers both its
    # attention and residual dropout).
    t5 = AutoModelForSeq2SeqLM.from_pretrained(t5_id, dtype=torch.float32, attn_implementation="eager").to(device)
    bridge = load_bridge(str(Path(args.bridge).expanduser())).to(device)
    if device == "mps":
        for layer in bridge.layers:
            layer.dropout = 0.0
    for p in t5.parameters():
        p.requires_grad = False
    groups = [{"params": list(bridge.parameters()), "lr": args.lr_bridge}]
    adapters = {}
    if args.decoder == "lora":
        adapters = add_cross_attn_lora(t5, rank=8)
        groups.append({"params": [p for a in adapters.values() for p in a.parameters() if p.requires_grad], "lr": args.lr_decoder})
    elif args.decoder == "full":
        for p in t5.decoder.parameters():
            p.requires_grad = True
        for p in t5.lm_head.parameters():
            p.requires_grad = True
        groups.append({"params": list(t5.decoder.parameters()) + list(t5.lm_head.parameters()), "lr": args.lr_decoder})
    print(f"{len(train)} train, {len(dev)} dev, {len(test)} test on {device}; t5 {args.t5}, decoder {args.decoder}", flush=True)

    def states(rows, silent=False):
        longest = max(r[3].shape[0] for r in rows)
        feats = torch.zeros(len(rows), longest, 512)
        lens = torch.tensor([r[3].shape[0] for r in rows])
        for i, r in enumerate(rows):
            if not silent:
                feats[i, : r[3].shape[0]] = r[3].float()
        a, amask = bridge(feats.to(device), lens.to(device))
        text = tok([args.prompt.replace("{names}", names.get(r[2], "")) for r in rows], padding=True, truncation=True, max_length=384, return_tensors="pt").to(device)
        with torch.no_grad():
            t = t5.encoder(input_ids=text.input_ids, attention_mask=text.attention_mask).last_hidden_state
        return torch.cat([t, a], 1), torch.cat([text.attention_mask.bool(), amask], 1)

    def predict(rows, silent=False, size=32):
        bridge.eval()
        t5.eval()
        preds = []
        with torch.no_grad():
            for i in range(0, len(rows), size):
                s, m = states(rows[i : i + size], silent)
                gen = t5.generate(encoder_outputs=BaseModelOutput(last_hidden_state=s), attention_mask=m.long(), max_new_tokens=64, num_beams=1, do_sample=False)
                preds += [x.strip() for x in tok.batch_decode(gen, skip_special_tokens=True)]
        bridge.train()
        t5.train()
        return preds

    params = [p for g in groups for p in g["params"]]
    opt = torch.optim.AdamW(groups, weight_decay=0.01)
    steps = args.epochs * math.ceil(len(train) / args.batch)
    warm = max(1, steps // 20)
    sched = torch.optim.lr_scheduler.LambdaLR(opt, lambda s: min(1.0, (s + 1) / warm) * max(0.0, (steps - s) / max(1, steps - warm)))
    best = -1.0
    bridge.train()
    t5.train()
    for epoch in range(1, args.epochs + 1):
        start, total, n = time.time(), 0.0, 0
        order = list(range(len(train)))
        random.shuffle(order)
        for i in range(0, len(order), args.batch):
            rows = [train[j] for j in order[i : i + args.batch]]
            s, m = states(rows)
            tgt = tok([r[1] for r in rows], padding=True, truncation=True, max_length=64, return_tensors="pt")
            labels = tgt.input_ids.clone()
            labels[labels == tok.pad_token_id] = -100
            loss = t5(encoder_outputs=BaseModelOutput(last_hidden_state=s), attention_mask=m.long(), labels=labels.to(device)).loss
            loss.backward()
            torch.nn.utils.clip_grad_norm_(params, 1.0)
            opt.step()
            sched.step()
            opt.zero_grad()
            total, n = total + loss.item(), n + 1
        preds = predict(dev)
        acc = sum(p == r[1] for p, r in zip(preds, dev)) / max(len(dev), 1)
        print(f"epoch {epoch}: mean loss {total / max(n, 1):.4f}, dev exact match {acc:.3f}, {time.time() - start:.0f} s", flush=True)
        if acc > best:
            best = acc
            save_bridge(bridge, str(out / "bridge.safetensors"))
            if args.decoder == "lora":
                torch.save(lora_state(adapters), out / "lora.pt")
            elif args.decoder == "full":
                torch.save({k: v for k, v in t5.state_dict().items() if k.startswith(("decoder.", "lm_head."))}, out / "decoder.pt")
            print(f"  kept epoch {epoch}", flush=True)

    # The best epoch back, then the test rows, as heard and with the audio silenced.
    from safetensors.torch import load_file
    bridge.load_state_dict(load_file(str(out / "bridge.safetensors")))
    if args.decoder == "lora":
        for k, v in torch.load(out / "lora.pt").items():
            name, which = k.rsplit(".", 1)
            getattr(adapters[name], {"lora_A": "A", "lora_B": "B"}[which]).data.copy_(v.to(device))
    elif args.decoder == "full":
        t5.load_state_dict(torch.load(out / "decoder.pt"), strict=False)
    for suffix, silent in (("", False), ("_silent", True)):
        preds = predict(test, silent)
        (out / f"test_predictions{suffix}.tsv").write_text("".join(f"{r[2]}\t{r[1]}\t{p}\n" for r, p in zip(test, preds)))
        acc = sum(p == r[1] for p, r in zip(preds, test)) / max(len(test), 1)
        print(f"test{suffix or ''} exact match {acc:.4f} of {len(test)}", flush=True)


if __name__ == "__main__":
    main()
